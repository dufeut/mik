//! Backend server representation and state management.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use parking_lot::RwLock;

/// State of a backend server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendState {
    /// Backend is healthy and accepting requests.
    Healthy,
    /// Backend is unhealthy and should not receive requests.
    Unhealthy,
    /// Backend health is unknown (initial state).
    Unknown,
}

/// A backend server that can receive proxied requests.
#[derive(Debug)]
pub struct Backend {
    /// Address of the backend (e.g., "127.0.0.1:3001").
    address: String,
    /// Whether the backend is currently healthy.
    healthy: AtomicBool,
    /// Number of consecutive health check failures.
    failure_count: AtomicU64,
    /// Number of consecutive health check successes.
    success_count: AtomicU64,
    /// Total requests handled.
    total_requests: AtomicU64,
    /// Currently active requests.
    active_requests: AtomicU64,
    /// Last health check time.
    last_check: RwLock<Option<Instant>>,
    /// Last successful response time.
    last_success: RwLock<Option<Instant>>,
}

impl Backend {
    /// Create a new backend with the given address.
    pub fn new(address: String) -> Self {
        Self {
            address,
            healthy: AtomicBool::new(true), // Assume healthy until proven otherwise
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            active_requests: AtomicU64::new(0),
            last_check: RwLock::new(None),
            last_success: RwLock::new(None),
        }
    }

    /// Get the backend address.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Get the full URL for a given path.
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    /// Check if the backend is healthy.
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
    }

    /// Get the current state of the backend.
    pub fn state(&self) -> BackendState {
        if self.last_check.read().is_none() {
            BackendState::Unknown
        } else if self.is_healthy() {
            BackendState::Healthy
        } else {
            BackendState::Unhealthy
        }
    }

    /// Mark the backend as healthy after a successful health check.
    pub fn mark_healthy(&self) {
        self.healthy.store(true, Ordering::Release);
        self.failure_count.store(0, Ordering::Release);
        self.success_count.fetch_add(1, Ordering::AcqRel);
        *self.last_check.write() = Some(Instant::now());
        *self.last_success.write() = Some(Instant::now());
    }

    /// Mark the backend as unhealthy after a failed health check.
    pub fn mark_unhealthy(&self) {
        self.healthy.store(false, Ordering::Release);
        self.success_count.store(0, Ordering::Release);
        self.failure_count.fetch_add(1, Ordering::AcqRel);
        *self.last_check.write() = Some(Instant::now());
    }

    /// Record a successful request.
    pub fn record_success(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        *self.last_success.write() = Some(Instant::now());
    }

    /// Record a failed request.
    pub fn record_failure(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        // Don't mark unhealthy on request failure - that's the health check's job
    }

    /// Increment active request count.
    pub fn start_request(&self) {
        self.active_requests.fetch_add(1, Ordering::AcqRel);
    }

    /// Decrement active request count.
    pub fn end_request(&self) {
        self.active_requests.fetch_sub(1, Ordering::AcqRel);
    }

    /// Get the number of active requests.
    pub fn active_requests(&self) -> u64 {
        self.active_requests.load(Ordering::Acquire)
    }

    /// Get the total number of requests handled.
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    /// Get the consecutive failure count.
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Get the consecutive success count.
    pub fn success_count(&self) -> u64 {
        self.success_count.load(Ordering::Relaxed)
    }
}

impl Clone for Backend {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
            healthy: AtomicBool::new(self.healthy.load(Ordering::Acquire)),
            failure_count: AtomicU64::new(self.failure_count.load(Ordering::Relaxed)),
            success_count: AtomicU64::new(self.success_count.load(Ordering::Relaxed)),
            total_requests: AtomicU64::new(self.total_requests.load(Ordering::Relaxed)),
            active_requests: AtomicU64::new(self.active_requests.load(Ordering::Relaxed)),
            last_check: RwLock::new(*self.last_check.read()),
            last_success: RwLock::new(*self.last_success.read()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_new() {
        let backend = Backend::new("127.0.0.1:3001".to_string());
        assert_eq!(backend.address(), "127.0.0.1:3001");
        assert!(backend.is_healthy()); // Assume healthy by default
        assert_eq!(backend.state(), BackendState::Unknown); // No health check yet
    }

    #[test]
    fn test_backend_url() {
        let backend = Backend::new("127.0.0.1:3001".to_string());
        assert_eq!(backend.url("/health"), "http://127.0.0.1:3001/health");
        assert_eq!(backend.url("/run/echo/"), "http://127.0.0.1:3001/run/echo/");
    }

    #[test]
    fn test_backend_health_transitions() {
        let backend = Backend::new("127.0.0.1:3001".to_string());

        // Initial state
        assert!(backend.is_healthy());
        assert_eq!(backend.failure_count(), 0);
        assert_eq!(backend.success_count(), 0);

        // Mark healthy
        backend.mark_healthy();
        assert!(backend.is_healthy());
        assert_eq!(backend.state(), BackendState::Healthy);
        assert_eq!(backend.success_count(), 1);
        assert_eq!(backend.failure_count(), 0);

        // Mark unhealthy
        backend.mark_unhealthy();
        assert!(!backend.is_healthy());
        assert_eq!(backend.state(), BackendState::Unhealthy);
        assert_eq!(backend.failure_count(), 1);
        assert_eq!(backend.success_count(), 0); // Reset on failure

        // Mark healthy again
        backend.mark_healthy();
        assert!(backend.is_healthy());
        assert_eq!(backend.failure_count(), 0); // Reset on success
    }

    #[test]
    fn test_backend_request_tracking() {
        let backend = Backend::new("127.0.0.1:3001".to_string());

        assert_eq!(backend.active_requests(), 0);
        assert_eq!(backend.total_requests(), 0);

        backend.start_request();
        assert_eq!(backend.active_requests(), 1);

        backend.start_request();
        assert_eq!(backend.active_requests(), 2);

        backend.record_success();
        assert_eq!(backend.total_requests(), 1);

        backend.end_request();
        assert_eq!(backend.active_requests(), 1);

        backend.end_request();
        assert_eq!(backend.active_requests(), 0);
    }
}
