//! Re-exports from local reliability module.
//!
//! This module re-exports the reliability primitives used by the runtime.

// Re-export circuit breaker
pub use crate::reliability::CircuitBreaker;

// Re-export security utilities
pub use crate::reliability::security::is_http_host_allowed;
