//! Security audit logging for HTTP daemon events.
//!
//! Provides structured audit logging for security-relevant events like
//! authentication failures, path traversal attempts, and request timeouts.

use std::net::SocketAddr;
use tracing::{info, warn};

/// Security audit events that should be logged for monitoring and alerting.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Variants reserved for future use
pub enum AuditEvent {
    /// Failed authentication attempt
    AuthFailure {
        remote_addr: SocketAddr,
        reason: String,
    },
    /// Path traversal attack blocked
    PathTraversalBlocked {
        path: String,
        remote_addr: SocketAddr,
    },
    /// Request timed out
    RequestTimeout {
        path: String,
        method: String,
        duration_ms: u64,
    },
    /// Successful authentication (for correlation)
    AuthSuccess { remote_addr: SocketAddr },
}

/// Log a security audit event with structured fields.
pub fn log_audit_event(event: AuditEvent) {
    match event {
        AuditEvent::AuthFailure {
            remote_addr,
            reason,
        } => {
            warn!(
                target: "audit",
                event_type = "auth_failure",
                %remote_addr,
                %reason,
                "Authentication failed"
            );
        },
        AuditEvent::PathTraversalBlocked { path, remote_addr } => {
            warn!(
                target: "audit",
                event_type = "path_traversal_blocked",
                %path,
                %remote_addr,
                "Path traversal attempt blocked"
            );
        },
        AuditEvent::RequestTimeout {
            path,
            method,
            duration_ms,
        } => {
            info!(
                target: "audit",
                event_type = "request_timeout",
                %path,
                %method,
                duration_ms,
                "Request timed out"
            );
        },
        AuditEvent::AuthSuccess { remote_addr } => {
            info!(
                target: "audit",
                event_type = "auth_success",
                %remote_addr,
                "Authentication succeeded"
            );
        },
    }
}
