//! L7 Load Balancer for mik runtime.
//!
//! This module provides a high-performance HTTP load balancer that distributes
//! requests across multiple backend workers. It supports:
//!
//! - Round-robin load balancing
//! - Health checks with automatic failover
//! - Connection pooling via reqwest
//! - Graceful shutdown with request draining
//!
//! # Architecture
//!
//! ```text
//! [Client] -> [L7 LB :3000] -> [Worker :3001]
//!                           -> [Worker :3002]
//!                           -> [Worker :3003]
//! ```
//!
//! # Example
//!
//! ```ignore
//! use mik::runtime::lb::{LoadBalancer, Backend};
//!
//! let backends = vec![
//!     Backend::new("127.0.0.1:3001"),
//!     Backend::new("127.0.0.1:3002"),
//! ];
//!
//! let lb = LoadBalancer::new(backends);
//! lb.serve("0.0.0.0:3000").await?;
//! ```

mod backend;
mod health;
mod proxy;
mod selection;

pub use backend::{Backend, BackendState};
pub use health::{HealthCheck, HealthCheckConfig};
pub use proxy::ProxyService;
pub use selection::{RoundRobin, Selection};

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::info;

/// Configuration for the load balancer.
#[derive(Debug, Clone)]
pub struct LoadBalancerConfig {
    /// Address to listen on.
    pub listen_addr: SocketAddr,
    /// Backend addresses.
    pub backends: Vec<String>,
    /// Health check configuration.
    pub health_check: HealthCheckConfig,
    /// Request timeout.
    pub request_timeout: Duration,
    /// Maximum concurrent requests per backend.
    pub max_connections_per_backend: usize,
}

impl Default for LoadBalancerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:3000".parse().unwrap(),
            backends: vec![],
            health_check: HealthCheckConfig::default(),
            request_timeout: Duration::from_secs(30),
            max_connections_per_backend: 100,
        }
    }
}

/// L7 Load Balancer.
///
/// Distributes HTTP requests across multiple backend workers using
/// round-robin selection with health-check-based failover.
pub struct LoadBalancer {
    config: LoadBalancerConfig,
    backends: Arc<RwLock<Vec<Backend>>>,
    selection: Arc<RwLock<RoundRobin>>,
    client: reqwest::Client,
}

impl LoadBalancer {
    /// Create a new load balancer with the given configuration.
    pub fn new(config: LoadBalancerConfig) -> Self {
        let backends: Vec<Backend> = config
            .backends
            .iter()
            .map(|addr| Backend::new(addr.clone()))
            .collect();

        let selection = RoundRobin::new(backends.len());

        // Create HTTP client with connection pooling
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .pool_max_idle_per_host(config.max_connections_per_backend)
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            backends: Arc::new(RwLock::new(backends)),
            selection: Arc::new(RwLock::new(selection)),
            client,
        }
    }

    /// Create a load balancer from a list of backend addresses.
    pub fn from_backends(listen_addr: SocketAddr, backends: Vec<String>) -> Self {
        let config = LoadBalancerConfig {
            listen_addr,
            backends,
            ..Default::default()
        };
        Self::new(config)
    }

    /// Start the load balancer.
    ///
    /// This will:
    /// 1. Start background health checks
    /// 2. Listen for incoming HTTP requests
    /// 3. Proxy requests to healthy backends
    pub async fn serve(self) -> Result<()> {
        let addr = self.config.listen_addr;
        let backends = self.backends.clone();
        let health_config = self.config.health_check.clone();

        // Start health check background task
        let health_backends = backends.clone();
        tokio::spawn(async move {
            health::run_health_checks(health_backends, health_config).await;
        });

        info!("L7 Load Balancer listening on http://{}", addr);

        // Log backends
        {
            let backends = backends.read().await;
            for (i, backend) in backends.iter().enumerate() {
                info!("  Backend {}: {}", i + 1, backend.address());
            }
        }

        // Create and run the proxy service
        let proxy = ProxyService::new(
            backends,
            self.selection,
            self.client,
            self.config.request_timeout,
        );

        proxy.serve(addr).await
    }

    /// Get the number of healthy backends.
    pub async fn healthy_count(&self) -> usize {
        let backends = self.backends.read().await;
        backends.iter().filter(|b| b.is_healthy()).count()
    }

    /// Get the total number of backends.
    pub async fn total_count(&self) -> usize {
        self.backends.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_balancer_config_default() {
        let config = LoadBalancerConfig::default();
        assert_eq!(config.listen_addr, "0.0.0.0:3000".parse().unwrap());
        assert!(config.backends.is_empty());
        assert_eq!(config.request_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_load_balancer_from_backends() {
        let lb = LoadBalancer::from_backends(
            "127.0.0.1:8080".parse().unwrap(),
            vec!["127.0.0.1:3001".to_string(), "127.0.0.1:3002".to_string()],
        );
        assert_eq!(lb.config.listen_addr, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(lb.config.backends.len(), 2);
    }
}
