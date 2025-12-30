//! Manager for graceful reload operations.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::super::RoundRobin;
use super::super::backend::Backend;
use super::types::{DrainingBackend, ReloadConfig, ReloadResult, ReloadSignal};

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
                if new_addresses.contains(&addr) {
                    unchanged.push(backend.clone());
                } else {
                    draining.push(backend.clone());
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
        let unchanged_addrs: Vec<String> =
            unchanged.iter().map(|b| b.address().to_string()).collect();

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
            backends_read
                .iter()
                .find(|b| b.address() == address)
                .cloned()
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
