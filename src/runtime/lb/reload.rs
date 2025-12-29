//! Graceful reload support for the L7 load balancer.
//!
//! This module provides mechanisms for dynamically updating the backend list
//! without restarting the load balancer. It supports:
//!
//! - Adding new backends immediately
//! - Draining existing backends before removal
//! - Signal-based reload triggering
//! - Configurable drain timeouts
//!
//! # Example
//!
//! ```ignore
//! use mik::runtime::lb::{LoadBalancer, ReloadConfig, ReloadHandle};
//! use std::time::Duration;
//!
//! let config = ReloadConfig {
//!     drain_timeout: Duration::from_secs(30),
//! };
//!
//! let (handle, receiver) = ReloadHandle::new();
//!
//! // In another task, trigger a reload
//! handle.trigger_reload(vec!["127.0.0.1:3001".to_string()]);
//! ```

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{watch, RwLock};
use tracing::{debug, info, warn};

use super::backend::Backend;
use super::RoundRobin;

/// Configuration for graceful reload operations.
#[derive(Debug, Clone)]
pub struct ReloadConfig {
    /// Maximum time to wait for a backend to drain requests before forcefully removing it.
    /// Default is 30 seconds.
    pub drain_timeout: Duration,
}

impl Default for ReloadConfig {
    fn default() -> Self {
        Self {
            drain_timeout: Duration::from_secs(30),
        }
    }
}

impl ReloadConfig {
    /// Create a new reload configuration with custom drain timeout.
    #[allow(dead_code)]
    pub fn with_drain_timeout(drain_timeout: Duration) -> Self {
        Self { drain_timeout }
    }
}

/// Signal payload for reload operations.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ReloadSignal {
    /// New list of backend addresses.
    pub backends: Vec<String>,
    /// Timestamp when the reload was requested.
    pub requested_at: Instant,
}

impl ReloadSignal {
    /// Create a new reload signal with the given backend addresses.
    pub fn new(backends: Vec<String>) -> Self {
        Self {
            backends,
            requested_at: Instant::now(),
        }
    }
}

/// Handle for triggering reload operations.
///
/// This handle can be cloned and shared across tasks to trigger reloads
/// from anywhere in the application.
#[derive(Clone)]
pub struct ReloadHandle {
    sender: watch::Sender<Option<ReloadSignal>>,
}

impl ReloadHandle {
    /// Create a new reload handle and receiver pair.
    ///
    /// The handle is used to trigger reloads, while the receiver is used
    /// by the load balancer to listen for reload signals.
    pub fn new() -> (Self, watch::Receiver<Option<ReloadSignal>>) {
        let (sender, receiver) = watch::channel(None);
        (Self { sender }, receiver)
    }

    /// Trigger a reload with the given backend addresses.
    ///
    /// This sends a signal to update the backend list. The load balancer
    /// will apply the changes gracefully, draining removed backends before
    /// removing them.
    #[allow(dead_code)]
    pub fn trigger_reload(&self, backends: Vec<String>) -> bool {
        let signal = ReloadSignal::new(backends);
        self.sender.send(Some(signal)).is_ok()
    }

    /// Check if there are any active receivers.
    #[allow(dead_code)]
    pub fn has_receivers(&self) -> bool {
        self.sender.receiver_count() > 0
    }
}

impl Default for ReloadHandle {
    fn default() -> Self {
        Self::new().0
    }
}

/// Manager for graceful reload operations.
///
/// This struct manages the lifecycle of backend updates, including:
/// - Tracking which backends are draining
/// - Coordinating drain timeouts
/// - Updating the shared backend list atomically
pub struct ReloadManager {
    config: ReloadConfig,
    backends: Arc<RwLock<Vec<Backend>>>,
    selection: Arc<RwLock<RoundRobin>>,
    /// Backends currently being drained (address -> drain start time).
    draining: Arc<RwLock<Vec<DrainingBackend>>>,
}

/// A backend that is being drained.
#[derive(Debug, Clone)]
struct DrainingBackend {
    /// The backend being drained.
    backend: Backend,
    /// When the drain started.
    started_at: Instant,
}

impl ReloadManager {
    /// Create a new reload manager.
    pub fn new(
        config: ReloadConfig,
        backends: Arc<RwLock<Vec<Backend>>>,
        selection: Arc<RwLock<RoundRobin>>,
    ) -> Self {
        Self {
            config,
            backends,
            selection,
            draining: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Apply a reload signal, updating the backend list gracefully.
    ///
    /// This method:
    /// 1. Adds new backends immediately
    /// 2. Puts removed backends into draining state
    /// 3. Returns backends that need drain monitoring
    #[allow(dead_code)]
    pub async fn apply_reload(&self, signal: &ReloadSignal) -> ReloadResult {
        let new_addresses: HashSet<String> = signal.backends.iter().cloned().collect();

        let mut added = Vec::new();
        let mut draining = Vec::new();
        let mut unchanged: Vec<Backend> = Vec::new();

        {
            let current_backends = self.backends.read().await;
            let current_addresses: HashSet<String> = current_backends
                .iter()
                .map(|b| b.address().to_string())
                .collect();

            // Find backends to add (in new but not in current)
            for addr in &new_addresses {
                if !current_addresses.contains(addr) {
                    added.push(addr.clone());
                }
            }

            // Find backends to drain (in current but not in new)
            for backend in current_backends.iter() {
                let addr = backend.address().to_string();
                if !new_addresses.contains(&addr) {
                    draining.push(backend.clone());
                } else {
                    unchanged.push(backend.clone());
                }
            }
        }

        // Log the changes
        if !added.is_empty() {
            info!(count = added.len(), "Adding new backends");
            for addr in &added {
                debug!(address = %addr, "New backend added");
            }
        }

        if !draining.is_empty() {
            info!(count = draining.len(), "Draining backends before removal");
            for backend in &draining {
                debug!(
                    address = %backend.address(),
                    active_requests = backend.active_requests(),
                    "Backend marked for draining"
                );
            }
        }

        // Add draining backends to the drain list
        {
            let mut drain_list = self.draining.write().await;
            for backend in &draining {
                drain_list.push(DrainingBackend {
                    backend: backend.clone(),
                    started_at: Instant::now(),
                });
            }
        }

        // Collect unchanged addresses before moving the vector
        let unchanged_addrs: Vec<String> = unchanged.iter().map(|b| b.address().to_string()).collect();

        // Update the backend list (add new ones, keep unchanged, mark draining as unhealthy)
        {
            let mut backends_write = self.backends.write().await;
            let mut selection_write = self.selection.write().await;

            // Build the new backend list
            let mut new_backend_list: Vec<Backend> = unchanged;

            // Add draining backends (marked as unhealthy so they don't receive new requests)
            for backend in &draining {
                let draining_backend = backend.clone();
                draining_backend.mark_unhealthy();
                new_backend_list.push(draining_backend);
            }

            // Add new backends
            for addr in &added {
                new_backend_list.push(Backend::new(addr.clone()));
            }

            // Update the selection algorithm
            *selection_write = RoundRobin::new(new_backend_list.len());

            // Replace the backend list
            *backends_write = new_backend_list;
        }

        ReloadResult {
            added,
            draining: draining.iter().map(|b| b.address().to_string()).collect(),
            unchanged: unchanged_addrs,
        }
    }

    /// Check draining backends and remove those that are fully drained or timed out.
    ///
    /// Returns the number of backends that were removed.
    pub async fn process_draining_backends(&self) -> usize {
        let removed_count;
        let now = Instant::now();

        // First, collect backends to remove
        let backends_to_remove: Vec<String> = {
            let drain_list = self.draining.read().await;
            drain_list
                .iter()
                .filter_map(|db| {
                    let elapsed = now.duration_since(db.started_at);
                    let timed_out = elapsed >= self.config.drain_timeout;
                    let drained = db.backend.active_requests() == 0;

                    if drained {
                        debug!(
                            address = %db.backend.address(),
                            elapsed_ms = elapsed.as_millis(),
                            "Backend drained successfully"
                        );
                        Some(db.backend.address().to_string())
                    } else if timed_out {
                        warn!(
                            address = %db.backend.address(),
                            active_requests = db.backend.active_requests(),
                            timeout_secs = self.config.drain_timeout.as_secs(),
                            "Backend drain timed out, forcefully removing"
                        );
                        Some(db.backend.address().to_string())
                    } else {
                        None
                    }
                })
                .collect()
        };

        if backends_to_remove.is_empty() {
            return 0;
        }

        // Remove from drain list
        {
            let mut drain_list = self.draining.write().await;
            drain_list.retain(|db| !backends_to_remove.contains(&db.backend.address().to_string()));
        }

        // Remove from backend list
        {
            let mut backends_write = self.backends.write().await;
            let mut selection_write = self.selection.write().await;

            let initial_len = backends_write.len();
            backends_write.retain(|b| !backends_to_remove.contains(&b.address().to_string()));
            removed_count = initial_len - backends_write.len();

            // Update selection algorithm
            *selection_write = RoundRobin::new(backends_write.len());
        }

        if removed_count > 0 {
            info!(count = removed_count, "Removed drained backends");
        }

        removed_count
    }

    /// Get the number of backends currently draining.
    #[allow(dead_code)]
    pub async fn draining_count(&self) -> usize {
        self.draining.read().await.len()
    }

    /// Check if a specific backend is draining.
    #[allow(dead_code)]
    pub async fn is_draining(&self, address: &str) -> bool {
        let drain_list = self.draining.read().await;
        drain_list.iter().any(|db| db.backend.address() == address)
    }

    /// Force remove a backend immediately without waiting for drain.
    #[allow(dead_code)]
    pub async fn force_remove(&self, address: &str) -> bool {
        // Remove from drain list
        {
            let mut drain_list = self.draining.write().await;
            drain_list.retain(|db| db.backend.address() != address);
        }

        // Remove from backend list
        let removed = {
            let mut backends_write = self.backends.write().await;
            let mut selection_write = self.selection.write().await;

            let initial_len = backends_write.len();
            backends_write.retain(|b| b.address() != address);
            let removed = backends_write.len() < initial_len;

            if removed {
                *selection_write = RoundRobin::new(backends_write.len());
            }

            removed
        };

        if removed {
            warn!(address = %address, "Backend forcefully removed");
        }

        removed
    }

    /// Add a single backend immediately.
    #[allow(dead_code)]
    pub async fn add_backend(&self, address: String) -> bool {
        let mut backends_write = self.backends.write().await;
        let mut selection_write = self.selection.write().await;

        // Check if backend already exists
        if backends_write.iter().any(|b| b.address() == address) {
            return false;
        }

        backends_write.push(Backend::new(address.clone()));
        *selection_write = RoundRobin::new(backends_write.len());

        info!(address = %address, "Backend added");
        true
    }

    /// Remove a backend with graceful draining.
    ///
    /// The backend is marked as unhealthy immediately (no new requests)
    /// and will be removed after active requests complete or timeout.
    #[allow(dead_code)]
    pub async fn remove_backend(&self, address: &str) -> bool {
        let backend = {
            let backends_read = self.backends.read().await;
            backends_read.iter().find(|b| b.address() == address).cloned()
        };

        let Some(backend) = backend else {
            return false;
        };

        // Mark as unhealthy to stop receiving new requests
        backend.mark_unhealthy();

        // Add to drain list
        {
            let mut drain_list = self.draining.write().await;
            if !drain_list.iter().any(|db| db.backend.address() == address) {
                drain_list.push(DrainingBackend {
                    backend,
                    started_at: Instant::now(),
                });
            }
        }

        info!(address = %address, "Backend marked for draining");
        true
    }

    /// Wait for a specific backend to finish draining.
    ///
    /// Returns `true` if the backend drained within the timeout,
    /// `false` if it timed out.
    #[allow(dead_code)]
    pub async fn wait_for_drain(&self, address: &str) -> bool {
        let start = Instant::now();
        let check_interval = Duration::from_millis(100);

        loop {
            // Check if still draining
            let still_draining = {
                let drain_list = self.draining.read().await;
                drain_list.iter().any(|db| db.backend.address() == address)
            };

            if !still_draining {
                return true;
            }

            // Check timeout
            if start.elapsed() >= self.config.drain_timeout {
                return false;
            }

            // Process draining backends
            self.process_draining_backends().await;

            // Sleep before next check
            tokio::time::sleep(check_interval).await;
        }
    }

    /// Run a background task that periodically processes draining backends.
    ///
    /// This should be spawned as a separate task.
    #[allow(dead_code)]
    pub async fn run_drain_processor(self: Arc<Self>, check_interval: Duration) {
        let mut interval = tokio::time::interval(check_interval);

        loop {
            interval.tick().await;
            self.process_draining_backends().await;
        }
    }
}

/// Result of a reload operation.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ReloadResult {
    /// Backend addresses that were added.
    pub added: Vec<String>,
    /// Backend addresses that are being drained.
    pub draining: Vec<String>,
    /// Backend addresses that remained unchanged.
    pub unchanged: Vec<String>,
}

impl ReloadResult {
    /// Check if any changes were made.
    #[allow(dead_code)]
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.draining.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_backends(addresses: Vec<&str>) -> Arc<RwLock<Vec<Backend>>> {
        let backends: Vec<Backend> = addresses
            .into_iter()
            .map(|addr| Backend::new(addr.to_string()))
            .collect();
        Arc::new(RwLock::new(backends))
    }

    #[test]
    fn test_reload_config_default() {
        let config = ReloadConfig::default();
        assert_eq!(config.drain_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_reload_config_custom() {
        let config = ReloadConfig::with_drain_timeout(Duration::from_secs(60));
        assert_eq!(config.drain_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_reload_signal_new() {
        let backends = vec!["127.0.0.1:3001".to_string(), "127.0.0.1:3002".to_string()];
        let signal = ReloadSignal::new(backends.clone());

        assert_eq!(signal.backends, backends);
        assert!(signal.requested_at.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn test_reload_handle_creation() {
        let (handle, _receiver) = ReloadHandle::new();
        assert!(handle.has_receivers());
    }

    #[test]
    fn test_reload_handle_trigger() {
        let (handle, mut receiver) = ReloadHandle::new();

        let backends = vec!["127.0.0.1:3001".to_string()];
        assert!(handle.trigger_reload(backends.clone()));

        // Check that the signal was received
        assert!(receiver.has_changed().unwrap());
        let signal = receiver.borrow_and_update();
        assert!(signal.is_some());
        assert_eq!(signal.as_ref().unwrap().backends, backends);
    }

    #[tokio::test]
    async fn test_reload_manager_add_backend() {
        let backends = create_test_backends(vec!["127.0.0.1:3001"]);
        let selection = Arc::new(RwLock::new(RoundRobin::new(1)));
        let config = ReloadConfig::default();

        let manager = ReloadManager::new(config, backends.clone(), selection);

        // Add a new backend
        assert!(manager.add_backend("127.0.0.1:3002".to_string()).await);

        // Verify it was added
        let backends_read = backends.read().await;
        assert_eq!(backends_read.len(), 2);
        assert!(backends_read.iter().any(|b| b.address() == "127.0.0.1:3002"));

        // Adding the same backend again should fail
        drop(backends_read);
        assert!(!manager.add_backend("127.0.0.1:3002".to_string()).await);
    }

    #[tokio::test]
    async fn test_reload_manager_remove_backend() {
        let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
        let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
        let config = ReloadConfig::default();

        let manager = ReloadManager::new(config, backends.clone(), selection);

        // Remove a backend
        assert!(manager.remove_backend("127.0.0.1:3001").await);

        // Verify it's marked for draining
        assert!(manager.is_draining("127.0.0.1:3001").await);
        assert_eq!(manager.draining_count().await, 1);

        // Removing non-existent backend should return false
        assert!(!manager.remove_backend("127.0.0.1:9999").await);
    }

    #[tokio::test]
    async fn test_reload_manager_process_draining_with_no_active_requests() {
        let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
        let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
        let config = ReloadConfig::default();

        let manager = ReloadManager::new(config, backends.clone(), selection);

        // Remove a backend (it has no active requests)
        manager.remove_backend("127.0.0.1:3001").await;

        // Process draining - should remove immediately since no active requests
        let removed = manager.process_draining_backends().await;
        assert_eq!(removed, 1);

        // Verify it was removed
        let backends_read = backends.read().await;
        assert_eq!(backends_read.len(), 1);
        assert!(!backends_read.iter().any(|b| b.address() == "127.0.0.1:3001"));
    }

    #[tokio::test]
    async fn test_reload_manager_drain_timeout() {
        let backends = create_test_backends(vec!["127.0.0.1:3001"]);
        let selection = Arc::new(RwLock::new(RoundRobin::new(1)));

        // Very short timeout for testing
        let config = ReloadConfig::with_drain_timeout(Duration::from_millis(50));

        let manager = ReloadManager::new(config, backends.clone(), selection);

        // Simulate an active request
        {
            let backends_read = backends.read().await;
            backends_read[0].start_request();
        }

        // Remove the backend
        manager.remove_backend("127.0.0.1:3001").await;

        // Process draining - should not remove yet (has active request)
        let removed = manager.process_draining_backends().await;
        assert_eq!(removed, 0);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Now it should be forcefully removed
        let removed = manager.process_draining_backends().await;
        assert_eq!(removed, 1);

        // Verify it was removed
        let backends_read = backends.read().await;
        assert_eq!(backends_read.len(), 0);
    }

    #[tokio::test]
    async fn test_reload_manager_force_remove() {
        let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
        let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
        let config = ReloadConfig::default();

        let manager = ReloadManager::new(config, backends.clone(), selection);

        // Simulate an active request
        {
            let backends_read = backends.read().await;
            backends_read[0].start_request();
        }

        // Force remove should work even with active requests
        assert!(manager.force_remove("127.0.0.1:3001").await);

        // Verify it was removed immediately
        let backends_read = backends.read().await;
        assert_eq!(backends_read.len(), 1);
        assert!(!backends_read.iter().any(|b| b.address() == "127.0.0.1:3001"));
    }

    #[tokio::test]
    async fn test_reload_manager_apply_reload_add_backends() {
        let backends = create_test_backends(vec!["127.0.0.1:3001"]);
        let selection = Arc::new(RwLock::new(RoundRobin::new(1)));
        let config = ReloadConfig::default();

        let manager = ReloadManager::new(config, backends.clone(), selection);

        // Apply reload with additional backend
        let signal = ReloadSignal::new(vec![
            "127.0.0.1:3001".to_string(),
            "127.0.0.1:3002".to_string(),
        ]);

        let result = manager.apply_reload(&signal).await;

        assert_eq!(result.added.len(), 1);
        assert!(result.added.contains(&"127.0.0.1:3002".to_string()));
        assert!(result.draining.is_empty());
        assert_eq!(result.unchanged.len(), 1);

        // Verify backends
        let backends_read = backends.read().await;
        assert_eq!(backends_read.len(), 2);
    }

    #[tokio::test]
    async fn test_reload_manager_apply_reload_remove_backends() {
        let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
        let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
        let config = ReloadConfig::default();

        let manager = ReloadManager::new(config, backends.clone(), selection);

        // Apply reload with one backend removed
        let signal = ReloadSignal::new(vec!["127.0.0.1:3001".to_string()]);

        let result = manager.apply_reload(&signal).await;

        assert!(result.added.is_empty());
        assert_eq!(result.draining.len(), 1);
        assert!(result.draining.contains(&"127.0.0.1:3002".to_string()));
        assert_eq!(result.unchanged.len(), 1);

        // Backend is still present but draining
        let backends_read = backends.read().await;
        assert_eq!(backends_read.len(), 2);

        // The draining backend should be unhealthy
        let draining = backends_read.iter().find(|b| b.address() == "127.0.0.1:3002").unwrap();
        assert!(!draining.is_healthy());
    }

    #[tokio::test]
    async fn test_reload_manager_apply_reload_mixed() {
        let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
        let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
        let config = ReloadConfig::default();

        let manager = ReloadManager::new(config, backends.clone(), selection);

        // Apply reload: keep 3001, remove 3002, add 3003
        let signal = ReloadSignal::new(vec![
            "127.0.0.1:3001".to_string(),
            "127.0.0.1:3003".to_string(),
        ]);

        let result = manager.apply_reload(&signal).await;

        assert_eq!(result.added.len(), 1);
        assert!(result.added.contains(&"127.0.0.1:3003".to_string()));
        assert_eq!(result.draining.len(), 1);
        assert!(result.draining.contains(&"127.0.0.1:3002".to_string()));
        assert_eq!(result.unchanged.len(), 1);
        assert!(result.unchanged.contains(&"127.0.0.1:3001".to_string()));

        assert!(result.has_changes());
    }

    #[tokio::test]
    async fn test_reload_result_has_changes() {
        let result = ReloadResult {
            added: vec![],
            draining: vec![],
            unchanged: vec!["127.0.0.1:3001".to_string()],
        };
        assert!(!result.has_changes());

        let result = ReloadResult {
            added: vec!["127.0.0.1:3002".to_string()],
            draining: vec![],
            unchanged: vec!["127.0.0.1:3001".to_string()],
        };
        assert!(result.has_changes());
    }

    #[tokio::test]
    async fn test_reload_manager_wait_for_drain() {
        let backends = create_test_backends(vec!["127.0.0.1:3001"]);
        let selection = Arc::new(RwLock::new(RoundRobin::new(1)));
        let config = ReloadConfig::with_drain_timeout(Duration::from_millis(500));

        let manager = ReloadManager::new(config, backends.clone(), selection);

        // Remove the backend (no active requests)
        manager.remove_backend("127.0.0.1:3001").await;

        // Wait for drain should complete quickly
        let drained = manager.wait_for_drain("127.0.0.1:3001").await;
        assert!(drained);
    }
}
