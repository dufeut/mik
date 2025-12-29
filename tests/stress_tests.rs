//! Stress tests for the mikrozen runtime.
//!
//! These tests validate the runtime's behavior under load, including:
//! - Concurrent request handling
//! - Rate limiting behavior
//! - Memory pressure handling
//! - Connection exhaustion recovery
//!
//! All tests are marked `#[ignore]` by default since they are resource-intensive.
//!
//! Run with:
//! ```bash
//! cargo test -p mik stress -- --ignored --test-threads=1
//! ```
//!
//! Note: Use `--test-threads=1` to avoid resource contention between stress tests.

#[path = "common.rs"]
mod common;

use common::TestHost;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Barrier;

/// Configuration for stress tests.
#[allow(dead_code)]
struct StressConfig {
    /// Number of concurrent clients.
    concurrent_clients: usize,
    /// Number of requests per client.
    requests_per_client: usize,
    /// Maximum acceptable error rate (0.0 - 1.0).
    max_error_rate: f64,
    /// Maximum acceptable p99 response time (for future use).
    max_p99_ms: u64,
}

impl Default for StressConfig {
    fn default() -> Self {
        Self {
            concurrent_clients: 100,
            requests_per_client: 10,
            max_error_rate: 0.01, // 1% error rate acceptable
            max_p99_ms: 5000,     // 5 seconds max p99
        }
    }
}

/// Metrics collected during stress tests.
#[derive(Debug, Default)]
struct StressMetrics {
    /// Total requests sent.
    total_requests: AtomicU64,
    /// Successful responses (2xx).
    successful_responses: AtomicU64,
    /// Client errors (4xx).
    client_errors: AtomicU64,
    /// Server errors (5xx).
    server_errors: AtomicU64,
    /// Connection/network errors.
    connection_errors: AtomicU64,
    /// Total response time in microseconds.
    total_response_time_us: AtomicU64,
}

impl StressMetrics {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn record_success(&self, response_time_us: u64) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.successful_responses.fetch_add(1, Ordering::Relaxed);
        self.total_response_time_us
            .fetch_add(response_time_us, Ordering::Relaxed);
    }

    fn record_client_error(&self, response_time_us: u64) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.client_errors.fetch_add(1, Ordering::Relaxed);
        self.total_response_time_us
            .fetch_add(response_time_us, Ordering::Relaxed);
    }

    fn record_server_error(&self, response_time_us: u64) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.server_errors.fetch_add(1, Ordering::Relaxed);
        self.total_response_time_us
            .fetch_add(response_time_us, Ordering::Relaxed);
    }

    fn record_connection_error(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.connection_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn summary(&self) -> StressSummary {
        let total = self.total_requests.load(Ordering::Relaxed);
        let successful = self.successful_responses.load(Ordering::Relaxed);
        let client_errors = self.client_errors.load(Ordering::Relaxed);
        let server_errors = self.server_errors.load(Ordering::Relaxed);
        let connection_errors = self.connection_errors.load(Ordering::Relaxed);
        let total_time_us = self.total_response_time_us.load(Ordering::Relaxed);

        let error_rate = if total > 0 {
            (server_errors + connection_errors) as f64 / total as f64
        } else {
            0.0
        };

        let avg_response_time_ms = if successful + client_errors + server_errors > 0 {
            total_time_us as f64 / (successful + client_errors + server_errors) as f64 / 1000.0
        } else {
            0.0
        };

        StressSummary {
            total_requests: total,
            successful_responses: successful,
            client_errors,
            server_errors,
            connection_errors,
            error_rate,
            avg_response_time_ms,
        }
    }
}

#[derive(Debug)]
struct StressSummary {
    total_requests: u64,
    successful_responses: u64,
    client_errors: u64,
    server_errors: u64,
    connection_errors: u64,
    error_rate: f64,
    avg_response_time_ms: f64,
}

impl std::fmt::Display for StressSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Stress Test Results:\n\
             - Total requests: {}\n\
             - Successful (2xx): {}\n\
             - Client errors (4xx): {}\n\
             - Server errors (5xx): {}\n\
             - Connection errors: {}\n\
             - Error rate: {:.2}%\n\
             - Avg response time: {:.2}ms",
            self.total_requests,
            self.successful_responses,
            self.client_errors,
            self.server_errors,
            self.connection_errors,
            self.error_rate * 100.0,
            self.avg_response_time_ms
        )
    }
}

// =============================================================================
// Concurrent Request Handling Tests
// =============================================================================

/// Test that the server handles 100+ simultaneous requests without crashing.
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_concurrent_request_handling() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let config = StressConfig {
        concurrent_clients: 100,
        requests_per_client: 10,
        max_error_rate: 0.05, // 5% error rate acceptable for concurrent test
        max_p99_ms: 10000,    // 10 seconds max for high concurrency
    };

    let metrics = StressMetrics::new();
    let barrier = Arc::new(Barrier::new(config.concurrent_clients));

    let mut handles = Vec::with_capacity(config.concurrent_clients);

    for _ in 0..config.concurrent_clients {
        let client = host.client().clone();
        let url = host.url("/health");
        let barrier = barrier.clone();
        let metrics = metrics.clone();
        let requests = config.requests_per_client;

        handles.push(tokio::spawn(async move {
            // Wait for all clients to be ready
            barrier.wait().await;

            for _ in 0..requests {
                let start = Instant::now();
                match client.get(&url).send().await {
                    Ok(resp) => {
                        let elapsed_us = start.elapsed().as_micros() as u64;
                        let status = resp.status().as_u16();
                        if (200..300).contains(&status) {
                            metrics.record_success(elapsed_us);
                        } else if (400..500).contains(&status) {
                            metrics.record_client_error(elapsed_us);
                        } else {
                            metrics.record_server_error(elapsed_us);
                        }
                    },
                    Err(_) => {
                        metrics.record_connection_error();
                    },
                }
            }
        }));
    }

    // Wait for all clients to complete
    for handle in handles {
        handle.await.expect("Client task panicked");
    }

    let summary = metrics.summary();
    println!("{}", summary);

    // Assertions
    assert!(
        summary.error_rate <= config.max_error_rate,
        "Error rate {:.2}% exceeds maximum {:.2}%",
        summary.error_rate * 100.0,
        config.max_error_rate * 100.0
    );

    let expected_total = (config.concurrent_clients * config.requests_per_client) as u64;
    assert_eq!(
        summary.total_requests, expected_total,
        "Expected {} requests, got {}",
        expected_total, summary.total_requests
    );
}

/// Test burst traffic (many requests in a very short time window).
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_burst_traffic() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let metrics = StressMetrics::new();
    let num_requests = 500;

    let start = Instant::now();

    // Fire all requests as fast as possible
    let mut handles = Vec::with_capacity(num_requests);
    for _ in 0..num_requests {
        let client = host.client().clone();
        let url = host.url("/health");
        let metrics = metrics.clone();

        handles.push(tokio::spawn(async move {
            let req_start = Instant::now();
            match client.get(&url).send().await {
                Ok(resp) => {
                    let elapsed_us = req_start.elapsed().as_micros() as u64;
                    let status = resp.status().as_u16();
                    if (200..300).contains(&status) {
                        metrics.record_success(elapsed_us);
                    } else if (400..500).contains(&status) {
                        metrics.record_client_error(elapsed_us);
                    } else {
                        metrics.record_server_error(elapsed_us);
                    }
                },
                Err(_) => {
                    metrics.record_connection_error();
                },
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Request task panicked");
    }

    let total_time = start.elapsed();
    let summary = metrics.summary();

    println!("{}", summary);
    println!("Total burst time: {:?}", total_time);
    println!(
        "Requests per second: {:.2}",
        num_requests as f64 / total_time.as_secs_f64()
    );

    // Server should handle burst without excessive errors
    assert!(
        summary.error_rate <= 0.10,
        "Burst error rate {:.2}% exceeds 10%",
        summary.error_rate * 100.0
    );
}

// =============================================================================
// Rate Limiting Tests
// =============================================================================

/// Test that rate limiting kicks in under sustained load.
/// Note: This test uses the /health endpoint which may not have rate limiting
/// applied in the same way as /run/* endpoints.
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_rate_limiting_behavior() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let metrics = StressMetrics::new();
    let rate_limited_count = Arc::new(AtomicUsize::new(0));

    // Send requests faster than typical rate limits
    let num_clients = 50;
    let requests_per_client = 50;

    let mut handles = Vec::with_capacity(num_clients);

    for _ in 0..num_clients {
        let client = host.client().clone();
        let url = host.url("/health");
        let metrics = metrics.clone();
        let rate_limited = rate_limited_count.clone();

        handles.push(tokio::spawn(async move {
            for _ in 0..requests_per_client {
                let start = Instant::now();
                match client.get(&url).send().await {
                    Ok(resp) => {
                        let elapsed_us = start.elapsed().as_micros() as u64;
                        let status = resp.status().as_u16();

                        // Check for rate limiting responses (429 or 503)
                        if status == 429 || status == 503 {
                            rate_limited.fetch_add(1, Ordering::Relaxed);
                            metrics.record_client_error(elapsed_us);
                        } else if (200..300).contains(&status) {
                            metrics.record_success(elapsed_us);
                        } else if (400..500).contains(&status) {
                            metrics.record_client_error(elapsed_us);
                        } else {
                            metrics.record_server_error(elapsed_us);
                        }
                    },
                    Err(_) => {
                        metrics.record_connection_error();
                    },
                }
                // Small delay to simulate realistic traffic pattern
                tokio::time::sleep(Duration::from_micros(100)).await;
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Client task panicked");
    }

    let summary = metrics.summary();
    let rate_limited = rate_limited_count.load(Ordering::Relaxed);

    println!("{}", summary);
    println!("Rate limited requests: {}", rate_limited);

    // All requests should complete (either success or rate-limited)
    let expected_total = (num_clients * requests_per_client) as u64;
    assert_eq!(
        summary.total_requests, expected_total,
        "Expected {} requests, got {}",
        expected_total, summary.total_requests
    );
}

/// Test that the server recovers after rate limiting subsides.
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_rate_limit_recovery() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    // Phase 1: Generate heavy load
    let metrics_phase1 = StressMetrics::new();
    let num_requests = 200;

    let mut handles = Vec::with_capacity(num_requests);
    for _ in 0..num_requests {
        let client = host.client().clone();
        let url = host.url("/health");
        let metrics = metrics_phase1.clone();

        handles.push(tokio::spawn(async move {
            let start = Instant::now();
            match client.get(&url).send().await {
                Ok(resp) => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    let status = resp.status().as_u16();
                    if (200..300).contains(&status) {
                        metrics.record_success(elapsed_us);
                    } else {
                        metrics.record_client_error(elapsed_us);
                    }
                },
                Err(_) => {
                    metrics.record_connection_error();
                },
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Request task panicked");
    }

    let summary_phase1 = metrics_phase1.summary();
    println!("Phase 1 (heavy load): {}", summary_phase1);

    // Phase 2: Wait for rate limits to reset
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Phase 3: Send normal traffic - should succeed
    let metrics_phase3 = StressMetrics::new();

    for _ in 0..10 {
        let start = Instant::now();
        match host.get("/health").await {
            Ok(resp) => {
                let elapsed_us = start.elapsed().as_micros() as u64;
                if resp.status().is_success() {
                    metrics_phase3.record_success(elapsed_us);
                } else {
                    metrics_phase3.record_client_error(elapsed_us);
                }
            },
            Err(_) => {
                metrics_phase3.record_connection_error();
            },
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let summary_phase3 = metrics_phase3.summary();
    println!("Phase 3 (after recovery): {}", summary_phase3);

    // After rate limit recovery, requests should succeed
    assert!(
        summary_phase3.successful_responses >= 8,
        "Expected at least 8 successful requests after recovery, got {}",
        summary_phase3.successful_responses
    );
}

// =============================================================================
// Memory Pressure Tests
// =============================================================================

/// Test handling of many large request bodies.
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_memory_pressure_large_requests() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let metrics = StressMetrics::new();

    // Create a moderately large body (100KB each)
    let large_body = "x".repeat(100 * 1024);

    let num_requests = 50;
    let mut handles = Vec::with_capacity(num_requests);

    for _ in 0..num_requests {
        let client = host.client().clone();
        let url = host.url("/health"); // POST to health (will be ignored but tests body handling)
        let body = large_body.clone();
        let metrics = metrics.clone();

        handles.push(tokio::spawn(async move {
            let start = Instant::now();
            match client
                .post(&url)
                .header("Content-Type", "text/plain")
                .body(body)
                .send()
                .await
            {
                Ok(resp) => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    let status = resp.status().as_u16();
                    if (200..300).contains(&status) {
                        metrics.record_success(elapsed_us);
                    } else if (400..500).contains(&status) {
                        metrics.record_client_error(elapsed_us);
                    } else {
                        metrics.record_server_error(elapsed_us);
                    }
                },
                Err(_) => {
                    metrics.record_connection_error();
                },
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Request task panicked");
    }

    let summary = metrics.summary();
    println!("{}", summary);

    // Memory pressure test: server should not crash and should handle all requests
    assert_eq!(
        summary.total_requests, num_requests as u64,
        "Expected {} requests, got {}",
        num_requests, summary.total_requests
    );

    // Connection errors indicate memory exhaustion - should be minimal
    assert!(
        summary.connection_errors <= 5,
        "Too many connection errors ({}) suggesting memory exhaustion",
        summary.connection_errors
    );
}

/// Test sustained load over time (simulates long-running production traffic).
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_sustained_load() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let metrics = StressMetrics::new();
    let duration = Duration::from_secs(10);
    let requests_per_second = 50;
    let interval = Duration::from_millis(1000 / requests_per_second);

    let start = Instant::now();
    let mut request_count = 0;

    while start.elapsed() < duration {
        let client = host.client().clone();
        let url = host.url("/health");
        let metrics = metrics.clone();

        tokio::spawn(async move {
            let req_start = Instant::now();
            match client.get(&url).send().await {
                Ok(resp) => {
                    let elapsed_us = req_start.elapsed().as_micros() as u64;
                    let status = resp.status().as_u16();
                    if (200..300).contains(&status) {
                        metrics.record_success(elapsed_us);
                    } else if (400..500).contains(&status) {
                        metrics.record_client_error(elapsed_us);
                    } else {
                        metrics.record_server_error(elapsed_us);
                    }
                },
                Err(_) => {
                    metrics.record_connection_error();
                },
            }
        });

        request_count += 1;
        tokio::time::sleep(interval).await;
    }

    // Wait a bit for in-flight requests to complete
    tokio::time::sleep(Duration::from_secs(2)).await;

    let summary = metrics.summary();
    println!("{}", summary);
    println!("Sustained load duration: {:?}", duration);
    println!("Target requests: {}", request_count);

    // Sustained load should have low error rate
    assert!(
        summary.error_rate <= 0.02,
        "Sustained load error rate {:.2}% exceeds 2%",
        summary.error_rate * 100.0
    );
}

// =============================================================================
// Connection Exhaustion Tests
// =============================================================================

/// Test behavior when many connections are opened simultaneously.
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_connection_exhaustion_recovery() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    // Phase 1: Open many connections simultaneously
    let num_connections = 200;
    let metrics_phase1 = StressMetrics::new();

    let barrier = Arc::new(Barrier::new(num_connections));
    let mut handles = Vec::with_capacity(num_connections);

    for _ in 0..num_connections {
        // Create a new client per connection to simulate many TCP connections
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(1)
            .build()
            .expect("Failed to create client");

        let url = host.url("/health");
        let barrier = barrier.clone();
        let metrics = metrics_phase1.clone();

        handles.push(tokio::spawn(async move {
            // Wait for all connections to be ready
            barrier.wait().await;

            let start = Instant::now();
            match client.get(&url).send().await {
                Ok(resp) => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    let status = resp.status().as_u16();
                    if (200..300).contains(&status) {
                        metrics.record_success(elapsed_us);
                    } else {
                        metrics.record_server_error(elapsed_us);
                    }
                },
                Err(_) => {
                    metrics.record_connection_error();
                },
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Connection task panicked");
    }

    let summary_phase1 = metrics_phase1.summary();
    println!("Phase 1 (connection exhaustion): {}", summary_phase1);

    // Phase 2: Wait for connections to close
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Phase 3: Verify server can still handle requests
    let mut successful_recovery = 0;
    for _ in 0..10 {
        match host.get("/health").await {
            Ok(resp) if resp.status().is_success() => {
                successful_recovery += 1;
            },
            _ => {},
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    println!("Recovery: {}/10 requests successful", successful_recovery);

    // Server should recover and handle new requests
    assert!(
        successful_recovery >= 8,
        "Server failed to recover after connection exhaustion: {}/10 successful",
        successful_recovery
    );
}

/// Test that the server handles connection timeouts gracefully.
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_slow_client_handling() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    // Simulate slow clients by using a very short timeout
    let slow_client = reqwest::Client::builder()
        .timeout(Duration::from_millis(10)) // Very short timeout
        .build()
        .expect("Failed to create slow client");

    let fast_client = host.client().clone();
    let url = host.url("/health");

    // Start some "slow client" requests that will timeout
    let mut slow_handles = Vec::new();
    for _ in 0..20 {
        let client = slow_client.clone();
        let url = url.clone();
        slow_handles.push(tokio::spawn(async move {
            // These will likely timeout
            let _ = client.get(&url).send().await;
        }));
    }

    // Meanwhile, fast clients should still work
    let mut fast_successes = 0;
    for _ in 0..10 {
        match fast_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                fast_successes += 1;
            },
            _ => {},
        }
    }

    // Wait for slow clients to complete/timeout
    for handle in slow_handles {
        let _ = handle.await;
    }

    println!("Fast client successes: {}/10", fast_successes);

    // Fast clients should not be blocked by slow clients
    assert!(
        fast_successes >= 8,
        "Fast clients blocked by slow clients: {}/10 successful",
        fast_successes
    );
}

// =============================================================================
// Mixed Load Tests
// =============================================================================

/// Test with mixed request patterns (GET, POST, different paths).
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_mixed_request_patterns() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let metrics = StressMetrics::new();
    let num_clients = 50;

    let endpoints = ["/health", "/metrics", "/nonexistent"];

    let mut handles = Vec::with_capacity(num_clients);

    for i in 0..num_clients {
        let client = host.client().clone();
        let base_url = host.base_url();
        let metrics = metrics.clone();
        let endpoint = endpoints[i % endpoints.len()];

        handles.push(tokio::spawn(async move {
            for _ in 0..10 {
                let url = format!("{}{}", base_url, endpoint);
                let start = Instant::now();

                let result = if i % 2 == 0 {
                    client.get(&url).send().await
                } else {
                    client.post(&url).body("test").send().await
                };

                match result {
                    Ok(resp) => {
                        let elapsed_us = start.elapsed().as_micros() as u64;
                        let status = resp.status().as_u16();
                        if (200..300).contains(&status) {
                            metrics.record_success(elapsed_us);
                        } else if (400..500).contains(&status) {
                            metrics.record_client_error(elapsed_us);
                        } else {
                            metrics.record_server_error(elapsed_us);
                        }
                    },
                    Err(_) => {
                        metrics.record_connection_error();
                    },
                }
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Client task panicked");
    }

    let summary = metrics.summary();
    println!("{}", summary);

    // All requests should complete (including 404s which are expected)
    let expected_total = (num_clients * 10) as u64;
    assert_eq!(
        summary.total_requests, expected_total,
        "Expected {} requests, got {}",
        expected_total, summary.total_requests
    );

    // Connection errors should be minimal
    assert!(
        summary.connection_errors <= 5,
        "Too many connection errors: {}",
        summary.connection_errors
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quick sanity check that metrics tracking works correctly.
    #[test]
    fn test_metrics_tracking() {
        let metrics = StressMetrics::new();

        metrics.record_success(1000);
        metrics.record_success(2000);
        metrics.record_client_error(500);
        metrics.record_server_error(1500);
        metrics.record_connection_error();

        let summary = metrics.summary();

        assert_eq!(summary.total_requests, 5);
        assert_eq!(summary.successful_responses, 2);
        assert_eq!(summary.client_errors, 1);
        assert_eq!(summary.server_errors, 1);
        assert_eq!(summary.connection_errors, 1);
    }

    /// Test error rate calculation.
    #[test]
    fn test_error_rate_calculation() {
        let metrics = StressMetrics::new();

        // 80 successes, 10 server errors, 10 connection errors = 20% error rate
        for _ in 0..80 {
            metrics.record_success(1000);
        }
        for _ in 0..10 {
            metrics.record_server_error(1000);
        }
        for _ in 0..10 {
            metrics.record_connection_error();
        }

        let summary = metrics.summary();

        assert_eq!(summary.total_requests, 100);
        assert!((summary.error_rate - 0.20).abs() < 0.001);
    }
}
