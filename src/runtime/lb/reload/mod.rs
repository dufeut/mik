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

mod handle;
mod manager;
mod types;

#[cfg(test)]
mod tests;

// Re-export public API for backward compatibility
pub use handle::ReloadHandle;
pub use manager::ReloadManager;
pub use types::{ReloadConfig, ReloadResult, ReloadSignal};
