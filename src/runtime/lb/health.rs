//! Health check implementation for backend servers.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::Backend;

/// Configuration for health checks.
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Interval between health checks.
    pub interval: Duration,
    /// Timeout for each health check request.
    pub timeout: Duration,
    /// Path to check for HTTP health checks.
    pub path: String,
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
            path: "/health".to_string(),
            unhealthy_threshold: 3,
            healthy_threshold: 2,
        }
    }
}

/// Health check service for monitoring backend health.
pub struct HealthCheck {
    config: HealthCheckConfig,
    client: reqwest::Client,
}

impl HealthCheck {
    /// Create a new health check service.
    pub fn new(config: HealthCheckConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .pool_max_idle_per_host(1)
            .build()
            .expect("Failed to create health check client");

        Self { config, client }
    }

    /// Check the health of a single backend.
    pub async fn check(&self, backend: &Backend) -> bool {
        let url = backend.url(&self.config.path);

        match self.client.get(&url).send().await {
            Ok(response) => {
                let healthy = response.status().is_success();
                if healthy {
                    debug!(backend = %backend.address(), "Health check passed");
                } else {
                    debug!(
                        backend = %backend.address(),
                        status = %response.status(),
                        "Health check failed with status"
                    );
                }
                healthy
            },
            Err(e) => {
                debug!(
                    backend = %backend.address(),
                    error = %e,
                    "Health check failed"
                );
                false
            },
        }
    }
}

/// Run continuous health checks for all backends.
pub async fn run_health_checks(backends: Arc<RwLock<Vec<Backend>>>, config: HealthCheckConfig) {
    let health_check = HealthCheck::new(config.clone());
    let mut interval = tokio::time::interval(config.interval);

    info!(
        interval = ?config.interval,
        path = %config.path,
        "Starting health check loop"
    );

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
                        backend = %backend.address(),
                        index = i,
                        "Backend recovered and marked healthy"
                    );
                }
            } else {
                backend.mark_unhealthy();
                if backend.failure_count() >= u64::from(config.unhealthy_threshold) && was_healthy {
                    warn!(
                        backend = %backend.address(),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.interval, Duration::from_secs(5));
        assert_eq!(config.timeout, Duration::from_millis(2000));
        assert_eq!(config.path, "/health");
        assert_eq!(config.unhealthy_threshold, 3);
        assert_eq!(config.healthy_threshold, 2);
    }

    #[test]
    fn test_health_check_config_custom() {
        let config = HealthCheckConfig {
            interval: Duration::from_secs(1),
            timeout: Duration::from_millis(500),
            path: "/healthz".to_string(),
            unhealthy_threshold: 5,
            healthy_threshold: 3,
        };

        assert_eq!(config.interval, Duration::from_secs(1));
        assert_eq!(config.path, "/healthz");
    }
}
