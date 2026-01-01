//! Health check implementation for backend servers.
//!
//! Provides periodic health monitoring of backend servers to ensure requests
//! are only routed to healthy instances. Supports both HTTP and TCP health checks.
//!
//! # Key Types
//!
//! - [`HealthCheckConfig`] - Configuration for health check behavior (interval, timeout, thresholds)
//! - [`HealthCheckType`] - Enum for HTTP (path-based) or TCP (connection-based) checks
//! - [`HealthChecker`] - Service for running health checks (used by Proxy)
//!
//! Health checks run in a background task and automatically update backend health status.

use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use anyhow::{Context, Result};

use super::backend::Backend;
use super::metrics::LbMetrics;

/// Type of health check to perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthCheckType {
    /// HTTP health check - sends GET request to specified path.
    Http {
        /// Path to check (e.g., "/health").
        path: String,
    },
    /// TCP health check - just verifies the port is accepting connections.
    Tcp,
}

impl Default for HealthCheckType {
    fn default() -> Self {
        Self::Http {
            path: "/health".to_string(),
        }
    }
}

/// Configuration for health checks.
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Interval between health checks.
    pub interval: Duration,
    /// Timeout for each health check request.
    pub timeout: Duration,
    /// Type of health check to perform.
    pub check_type: HealthCheckType,
    /// Number of consecutive failures before marking unhealthy.
    pub unhealthy_threshold: u32,
    /// Number of consecutive successes before marking healthy.
    pub healthy_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            timeout: Duration::from_millis(2000),
            check_type: HealthCheckType::default(),
            unhealthy_threshold: 3,
            healthy_threshold: 2,
        }
    }
}

impl HealthCheckConfig {
    /// Create a new HTTP health check configuration.
    #[allow(dead_code)]
    pub fn http(path: impl Into<String>) -> Self {
        Self {
            check_type: HealthCheckType::Http { path: path.into() },
            ..Default::default()
        }
    }

    /// Create a new TCP health check configuration.
    #[allow(dead_code)]
    pub fn tcp() -> Self {
        Self {
            check_type: HealthCheckType::Tcp,
            ..Default::default()
        }
    }

    /// Get the path for HTTP health checks (for backwards compatibility).
    #[allow(dead_code)]
    pub fn path(&self) -> &str {
        match &self.check_type {
            HealthCheckType::Http { path } => path,
            HealthCheckType::Tcp => "",
        }
    }
}

/// Health check service for monitoring backend health.
pub(super) struct HealthCheck {
    config: HealthCheckConfig,
    /// HTTP client for HTTP health checks (only created when needed).
    client: Option<reqwest::Client>,
}

impl HealthCheck {
    /// Create a new health check service.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created (e.g., TLS configuration issues).
    pub(super) fn new(config: HealthCheckConfig) -> Result<Self> {
        // Only create HTTP client if we're doing HTTP health checks
        let client = match &config.check_type {
            HealthCheckType::Http { .. } => Some(
                reqwest::Client::builder()
                    .timeout(config.timeout)
                    .pool_max_idle_per_host(1)
                    .build()
                    .context("failed to create HTTP client - check TLS configuration")?,
            ),
            HealthCheckType::Tcp => None,
        };

        Ok(Self { config, client })
    }

    /// Check the health of a single backend.
    ///
    /// For HTTP backends, performs either HTTP or TCP health checks based on config.
    /// For Runtime backends, delegates to the runtime's health_check method.
    pub(super) async fn check(&self, backend: &Backend) -> bool {
        // Get HTTP client for HTTP backends
        let client = self.client.as_ref();

        match backend {
            Backend::Http(http_backend) => {
                match &self.config.check_type {
                    HealthCheckType::Http { path } => {
                        if let Some(client) = client {
                            http_backend.health_check(client, path).await
                        } else {
                            // Should not happen due to invariant in new()
                            debug!(backend = %http_backend.address(), "No HTTP client for health check");
                            false
                        }
                    },
                    HealthCheckType::Tcp => self.check_tcp_address(http_backend.address()).await,
                }
            },
            Backend::Runtime(runtime_backend) => {
                // Runtime backends use their own health check
                runtime_backend.health_check().await
            },
        }
    }

    /// Perform a TCP health check by attempting to connect to an address.
    async fn check_tcp_address(&self, address: &str) -> bool {
        // Resolve the address to a SocketAddr
        let socket_addr = match address.to_socket_addrs() {
            Ok(mut addrs) => {
                if let Some(addr) = addrs.next() {
                    addr
                } else {
                    debug!(
                        backend = %address,
                        "TCP health check failed: no addresses resolved"
                    );
                    return false;
                }
            },
            Err(e) => {
                debug!(
                    backend = %address,
                    error = %e,
                    "TCP health check failed: address resolution error"
                );
                return false;
            },
        };

        // Attempt TCP connection with timeout
        match timeout(self.config.timeout, TcpStream::connect(socket_addr)).await {
            Ok(Ok(_stream)) => {
                debug!(backend = %address, "TCP health check passed");
                true
            },
            Ok(Err(e)) => {
                debug!(
                    backend = %address,
                    error = %e,
                    "TCP health check failed: connection error"
                );
                false
            },
            Err(_) => {
                debug!(
                    backend = %address,
                    timeout = ?self.config.timeout,
                    "TCP health check failed: connection timeout"
                );
                false
            },
        }
    }
}

/// Health checker service for use with Proxy.
///
/// This struct wraps `HealthCheckConfig` and provides a `run` method that can be
/// spawned as a background task. It's designed to work with the new `Backend` enum
/// that supports both HTTP and Runtime backends.
pub struct HealthChecker {
    config: HealthCheckConfig,
}

impl HealthChecker {
    /// Create a new health checker with the given configuration.
    pub fn new(config: HealthCheckConfig) -> Self {
        Self { config }
    }

    /// Run continuous health checks for all backends.
    ///
    /// This method runs forever, performing health checks at the configured interval.
    /// It updates backend health status and metrics.
    pub async fn run(&self, backends: Arc<RwLock<Vec<Backend>>>, metrics: LbMetrics) {
        run_health_checks_internal(backends, self.config.clone(), metrics).await;
    }

    /// Get the health check configuration.
    pub fn config(&self) -> &HealthCheckConfig {
        &self.config
    }
}

/// Internal implementation of health check loop.
async fn run_health_checks_internal(
    backends: Arc<RwLock<Vec<Backend>>>,
    config: HealthCheckConfig,
    metrics: LbMetrics,
) {
    let health_check = HealthCheck::new(config.clone())
        .expect("failed to create health check service - check TLS configuration");
    let mut interval = tokio::time::interval(config.interval);

    match &config.check_type {
        HealthCheckType::Http { path } => {
            info!(
                interval = ?config.interval,
                path = %path,
                check_type = "HTTP",
                "Starting health check loop"
            );
        },
        HealthCheckType::Tcp => {
            info!(
                interval = ?config.interval,
                check_type = "TCP",
                "Starting health check loop"
            );
        },
    }

    loop {
        interval.tick().await;

        let backends_snapshot = {
            let backends = backends.read().await;
            backends.clone()
        };

        for (i, backend) in backends_snapshot.iter().enumerate() {
            let is_healthy = health_check.check(backend).await;
            let was_healthy = backend.is_healthy();

            // Apply threshold logic
            if is_healthy {
                backend.mark_healthy();
                if backend.success_count() >= u64::from(config.healthy_threshold) && !was_healthy {
                    info!(
                        backend = %backend.id(),
                        index = i,
                        "Backend recovered and marked healthy"
                    );
                }
            } else {
                backend.mark_unhealthy();
                if backend.failure_count() >= u64::from(config.unhealthy_threshold) && was_healthy {
                    warn!(
                        backend = %backend.id(),
                        index = i,
                        failures = backend.failure_count(),
                        "Backend marked unhealthy after consecutive failures"
                    );
                }
            }
        }

        // Update the shared backends with new health state
        {
            let backends_write = backends.write().await;
            for (i, backend) in backends_snapshot.iter().enumerate() {
                if i < backends_write.len() {
                    // Copy health state to shared backends
                    if backend.is_healthy() {
                        backends_write[i].mark_healthy();
                    } else {
                        backends_write[i].mark_unhealthy();
                    }
                }
            }
        }

        // Update backend health metrics after each health check cycle
        metrics.update_backend_metrics(
            backends_snapshot
                .iter()
                .map(|b| (b.id(), b.is_healthy(), b.active_requests())),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.interval, Duration::from_secs(5));
        assert_eq!(config.timeout, Duration::from_millis(2000));
        assert_eq!(
            config.check_type,
            HealthCheckType::Http {
                path: "/health".to_string()
            }
        );
        assert_eq!(config.path(), "/health");
        assert_eq!(config.unhealthy_threshold, 3);
        assert_eq!(config.healthy_threshold, 2);
    }

    #[test]
    fn test_health_check_config_http() {
        let config = HealthCheckConfig::http("/healthz");
        assert_eq!(
            config.check_type,
            HealthCheckType::Http {
                path: "/healthz".to_string()
            }
        );
        assert_eq!(config.path(), "/healthz");
    }

    #[test]
    fn test_health_check_config_tcp() {
        let config = HealthCheckConfig::tcp();
        assert_eq!(config.check_type, HealthCheckType::Tcp);
        assert_eq!(config.path(), "");
    }

    #[test]
    fn test_health_check_config_custom() {
        let config = HealthCheckConfig {
            interval: Duration::from_secs(1),
            timeout: Duration::from_millis(500),
            check_type: HealthCheckType::Http {
                path: "/healthz".to_string(),
            },
            unhealthy_threshold: 5,
            healthy_threshold: 3,
        };

        assert_eq!(config.interval, Duration::from_secs(1));
        assert_eq!(config.path(), "/healthz");
    }

    #[test]
    fn test_health_check_type_default() {
        let check_type = HealthCheckType::default();
        assert_eq!(
            check_type,
            HealthCheckType::Http {
                path: "/health".to_string()
            }
        );
    }

    #[test]
    fn test_health_check_creates_client_for_http() {
        let config = HealthCheckConfig::http("/health");
        let health_check = HealthCheck::new(config).unwrap();
        assert!(health_check.client.is_some());
    }

    #[test]
    fn test_health_check_no_client_for_tcp() {
        let config = HealthCheckConfig::tcp();
        let health_check = HealthCheck::new(config).unwrap();
        assert!(health_check.client.is_none());
    }

    #[tokio::test]
    async fn test_tcp_health_check_passes_when_port_open() {
        // Start a TCP listener on a random port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Create backend and health check
        let backend = Backend::http(addr.to_string());
        let config = HealthCheckConfig {
            timeout: Duration::from_millis(100),
            ..HealthCheckConfig::tcp()
        };
        let health_check = HealthCheck::new(config).unwrap();

        // Health check should pass
        let result = health_check.check(&backend).await;
        assert!(
            result,
            "TCP health check should pass when port is accepting connections"
        );
    }

    #[tokio::test]
    async fn test_tcp_health_check_fails_when_port_closed() {
        // Use a port that's likely not in use
        // We bind to a port and then drop it to get a "free" port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // Close the listener so the port is closed

        // Create backend and health check
        let backend = Backend::http(addr.to_string());
        let config = HealthCheckConfig {
            timeout: Duration::from_millis(100),
            ..HealthCheckConfig::tcp()
        };
        let health_check = HealthCheck::new(config).unwrap();

        // Health check should fail
        let result = health_check.check(&backend).await;
        assert!(!result, "TCP health check should fail when port is closed");
    }

    #[tokio::test]
    async fn test_tcp_health_check_fails_on_invalid_address() {
        // Use an invalid address
        let backend = Backend::http("invalid-host:12345");
        let config = HealthCheckConfig {
            timeout: Duration::from_millis(100),
            ..HealthCheckConfig::tcp()
        };
        let health_check = HealthCheck::new(config).unwrap();

        // Health check should fail
        let result = health_check.check(&backend).await;
        assert!(!result, "TCP health check should fail on invalid address");
    }

    #[tokio::test]
    async fn test_tcp_health_check_respects_timeout() {
        // Use a non-routable address to trigger timeout
        // 10.255.255.1 is typically non-routable and will cause a timeout
        let backend = Backend::http("10.255.255.1:12345");
        let config = HealthCheckConfig {
            timeout: Duration::from_millis(50), // Very short timeout
            ..HealthCheckConfig::tcp()
        };
        let health_check = HealthCheck::new(config).unwrap();

        let start = std::time::Instant::now();
        let result = health_check.check(&backend).await;
        let elapsed = start.elapsed();

        assert!(!result, "TCP health check should fail on timeout");
        // Allow some margin for the timeout check
        assert!(
            elapsed < Duration::from_millis(200),
            "TCP health check should respect timeout (elapsed: {elapsed:?})"
        );
    }
}
