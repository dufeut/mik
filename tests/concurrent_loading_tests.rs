// Test-specific lint suppressions
#![allow(clippy::manual_range_contains)]

//! Concurrent module loading tests based on CVE-2024-47813.
//!
//! CVE-2024-47813 was a race condition in Wasmtime's Engine type registry
//! when multiple threads accessed it concurrently. These tests stress-test
//! concurrent module loading to verify the runtime handles parallel access
//! safely without panics, hangs, or data corruption.
//!
//! Tests include:
//! - Multiple threads loading the same module simultaneously
//! - Multiple threads loading different modules simultaneously
//! - Rapid concurrent requests to stress the module cache and type registry
//!
//! All tests are marked `#[ignore]` by default since they are resource-intensive.
//!
//! Run with:
//! ```bash
//! cargo test -p mik concurrent_loading -- --ignored --test-threads=1
//! ```
//!
//! Note: Use `--test-threads=1` to avoid resource contention between stress tests.
//!
//! ## CVE-2024-47813 Background
//!
//! The vulnerability was in the type registry of Wasmtime's Engine, which could
//! experience data races when multiple threads attempted to register or look up
//! types concurrently. This was fixed in Wasmtime 24.0.1, 25.0.2, and 26.0.0.
//!
//! These tests help ensure our runtime configuration and usage patterns don't
//! trigger similar race conditions, and that updates to Wasmtime remain safe.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Barrier;

mod common;
use common::RealTestHost;

/// Get the path to the test fixtures directory.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules")
}

/// Check if a specific fixture exists.
fn fixture_exists(name: &str) -> bool {
    fixtures_dir().join(name).exists()
}

/// Configuration for concurrent loading tests.
struct ConcurrentConfig {
    /// Number of concurrent tasks to spawn.
    num_tasks: usize,
    /// Number of iterations per test run (for repeatability).
    iterations: usize,
    /// Maximum acceptable error rate (0.0 - 1.0).
    max_error_rate: f64,
    /// Timeout for the entire test.
    test_timeout_secs: u64,
}

impl Default for ConcurrentConfig {
    fn default() -> Self {
        Self {
            num_tasks: 50,
            iterations: 3,
            max_error_rate: 0.05, // 5% error rate acceptable
            test_timeout_secs: 120,
        }
    }
}

/// Metrics collected during concurrent loading tests.
#[derive(Debug, Default)]
struct ConcurrentMetrics {
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
    /// Number of panics detected (tasks that didn't complete).
    panics: AtomicUsize,
}

impl ConcurrentMetrics {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn record_success(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.successful_responses.fetch_add(1, Ordering::Relaxed);
    }

    fn record_client_error(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.client_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn record_server_error(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.server_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn record_connection_error(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.connection_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn record_panic(&self) {
        self.panics.fetch_add(1, Ordering::Relaxed);
    }

    fn summary(&self) -> ConcurrentSummary {
        let total = self.total_requests.load(Ordering::Relaxed);
        let successful = self.successful_responses.load(Ordering::Relaxed);
        let client_errors = self.client_errors.load(Ordering::Relaxed);
        let server_errors = self.server_errors.load(Ordering::Relaxed);
        let connection_errors = self.connection_errors.load(Ordering::Relaxed);
        let panics = self.panics.load(Ordering::Relaxed);

        let error_rate = if total > 0 {
            (server_errors + connection_errors) as f64 / total as f64
        } else {
            0.0
        };

        ConcurrentSummary {
            total_requests: total,
            successful_responses: successful,
            client_errors,
            server_errors,
            connection_errors,
            panics,
            error_rate,
        }
    }
}

#[derive(Debug)]
struct ConcurrentSummary {
    total_requests: u64,
    successful_responses: u64,
    client_errors: u64,
    server_errors: u64,
    connection_errors: u64,
    panics: usize,
    error_rate: f64,
}

impl std::fmt::Display for ConcurrentSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Concurrent Loading Test Results:\n\
             - Total requests: {}\n\
             - Successful (2xx): {}\n\
             - Client errors (4xx): {}\n\
             - Server errors (5xx): {}\n\
             - Connection errors: {}\n\
             - Task panics: {}\n\
             - Error rate: {:.2}%",
            self.total_requests,
            self.successful_responses,
            self.client_errors,
            self.server_errors,
            self.connection_errors,
            self.panics,
            self.error_rate * 100.0
        )
    }
}

// =============================================================================
// CVE-2024-47813 Regression Tests: Concurrent Module Loading
// =============================================================================

/// Test that multiple threads can safely load and execute the same WASM module
/// concurrently without race conditions, panics, or hangs.
///
/// This test specifically targets the scenario from CVE-2024-47813 where
/// concurrent access to Wasmtime's Engine type registry could cause data races.
///
/// The test:
/// 1. Spawns many concurrent tasks (50-100)
/// 2. All tasks simultaneously request the same module (echo.wasm)
/// 3. Uses a barrier to ensure synchronized start for maximum contention
/// 4. Runs multiple iterations to increase the chance of catching race conditions
/// 5. Verifies no panics, hangs, and acceptable error rates
#[tokio::test]
#[ignore = "Resource-intensive stress test for CVE-2024-47813 regression"]
async fn test_parallel_load_same_module() {
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm not found. Run build script first.");
        return;
    }

    let config = ConcurrentConfig {
        num_tasks: 75,
        iterations: 5,
        max_error_rate: 0.10, // 10% error rate acceptable for high contention
        test_timeout_secs: 180,
    };

    println!(
        "CVE-2024-47813 Regression Test: Parallel Same Module Loading\n\
         Tasks: {}, Iterations: {}",
        config.num_tasks, config.iterations
    );

    let test_start = Instant::now();

    for iteration in 1..=config.iterations {
        println!("\n--- Iteration {}/{} ---", iteration, config.iterations);

        // Start a fresh host for each iteration to test cold-start concurrency
        let host = RealTestHost::builder()
            .with_modules_dir(fixtures_dir())
            .with_execution_timeout(30)
            .with_max_concurrent_requests(config.num_tasks)
            .with_cache_size(10) // Small cache to stress module loading
            .start()
            .await
            .expect("Failed to start host");

        let metrics = ConcurrentMetrics::new();
        let barrier = Arc::new(Barrier::new(config.num_tasks));

        let mut handles = Vec::with_capacity(config.num_tasks);

        for task_id in 0..config.num_tasks {
            let url = host.url("/run/echo/");
            let client = host.client().clone();
            let barrier = barrier.clone();
            let metrics = metrics.clone();

            handles.push(tokio::spawn(async move {
                // Wait for all tasks to be ready - ensures maximum contention
                barrier.wait().await;

                // Each task makes a request with unique payload
                let body = serde_json::json!({
                    "task_id": task_id,
                    "iteration": iteration,
                    "timestamp": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                });

                match client.post(&url).json(&body).send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        if status >= 200 && status < 300 {
                            metrics.record_success();
                        } else if status >= 400 && status < 500 {
                            metrics.record_client_error();
                        } else {
                            metrics.record_server_error();
                        }
                    },
                    Err(_) => {
                        metrics.record_connection_error();
                    },
                }
            }));
        }

        // Wait for all tasks with timeout detection
        let wait_timeout = Duration::from_secs(60);
        let wait_result =
            tokio::time::timeout(wait_timeout, futures::future::join_all(handles)).await;

        match wait_result {
            Ok(results) => {
                // Check for panicked tasks
                for result in results {
                    if result.is_err() {
                        metrics.record_panic();
                    }
                }
            },
            Err(_) => {
                panic!(
                    "Test timed out after {:?}. Possible hang in concurrent module loading.",
                    wait_timeout
                );
            },
        }

        let summary = metrics.summary();
        println!("{}", summary);

        // Assertions for this iteration
        assert_eq!(
            summary.panics, 0,
            "CVE-2024-47813: No tasks should panic during concurrent module loading"
        );

        assert!(
            summary.error_rate <= config.max_error_rate,
            "CVE-2024-47813: Error rate {:.2}% exceeds maximum {:.2}%",
            summary.error_rate * 100.0,
            config.max_error_rate * 100.0
        );

        let expected_total = config.num_tasks as u64;
        assert_eq!(
            summary.total_requests, expected_total,
            "All {} tasks should complete, got {}",
            expected_total, summary.total_requests
        );

        // Verify host is still healthy after concurrent load
        let health = host.get("/health").await.expect("Health check failed");
        assert_eq!(
            health.status(),
            200,
            "Server should remain healthy after concurrent load"
        );

        // Small delay between iterations
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let total_elapsed = test_start.elapsed();
    println!(
        "\nCVE-2024-47813 test completed successfully in {:?}",
        total_elapsed
    );
    assert!(
        total_elapsed.as_secs() < config.test_timeout_secs,
        "Test took too long ({:?}), possible performance regression",
        total_elapsed
    );
}

/// Test that multiple threads can safely load and execute different WASM modules
/// concurrently without race conditions, panics, or hangs.
///
/// This test complements `test_parallel_load_same_module` by stressing the
/// type registry with different module types being loaded simultaneously.
/// CVE-2024-47813 could also manifest when different modules with different
/// types are registered concurrently.
///
/// The test:
/// 1. Spawns many concurrent tasks (50-100)
/// 2. Tasks request the echo module with different paths to stress routing
/// 3. Uses a barrier to ensure synchronized start for maximum contention
/// 4. Runs multiple iterations to increase the chance of catching race conditions
/// 5. Verifies no panics, hangs, and acceptable error rates
///
/// Note: Since we primarily have echo.wasm as a reliable fixture, this test
/// uses it with varying paths/payloads. For true multi-module testing,
/// additional fixtures would be needed.
#[tokio::test]
#[ignore = "Resource-intensive stress test for CVE-2024-47813 regression"]
async fn test_parallel_load_different_modules() {
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm not found. Run build script first.");
        return;
    }

    let config = ConcurrentConfig {
        num_tasks: 100,
        iterations: 3,
        max_error_rate: 0.10,
        test_timeout_secs: 180,
    };

    // Define different request patterns to stress different code paths
    let request_patterns = [
        ("/run/echo/", serde_json::json!({"type": "simple"})),
        (
            "/run/echo/path1",
            serde_json::json!({"type": "with_path", "path": 1}),
        ),
        (
            "/run/echo/path2",
            serde_json::json!({"type": "with_path", "path": 2}),
        ),
        (
            "/run/echo/nested/path",
            serde_json::json!({"type": "nested", "depth": 2}),
        ),
        (
            "/run/echo/",
            serde_json::json!({
                "type": "complex",
                "array": [1, 2, 3, 4, 5],
                "nested": {"a": {"b": {"c": "deep"}}}
            }),
        ),
    ];

    println!(
        "CVE-2024-47813 Regression Test: Parallel Different Modules/Paths\n\
         Tasks: {}, Iterations: {}, Patterns: {}",
        config.num_tasks,
        config.iterations,
        request_patterns.len()
    );

    let test_start = Instant::now();

    for iteration in 1..=config.iterations {
        println!("\n--- Iteration {}/{} ---", iteration, config.iterations);

        let host = RealTestHost::builder()
            .with_modules_dir(fixtures_dir())
            .with_execution_timeout(30)
            .with_max_concurrent_requests(config.num_tasks + 10) // Extra headroom
            .with_cache_size(5) // Very small cache to force reloading
            .start()
            .await
            .expect("Failed to start host");

        let metrics = ConcurrentMetrics::new();
        let barrier = Arc::new(Barrier::new(config.num_tasks));

        let mut handles = Vec::with_capacity(config.num_tasks);

        for task_id in 0..config.num_tasks {
            let base_url = host.url("");
            let client = host.client().clone();
            let barrier = barrier.clone();
            let metrics = metrics.clone();

            // Each task uses a different pattern based on its ID
            let pattern_idx = task_id % request_patterns.len();
            let (path, base_body) = request_patterns[pattern_idx].clone();

            handles.push(tokio::spawn(async move {
                // Wait for all tasks to be ready
                barrier.wait().await;

                // Augment the body with task-specific info
                let mut body = base_body;
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("task_id".to_string(), serde_json::json!(task_id));
                    obj.insert("iteration".to_string(), serde_json::json!(iteration));
                }

                let url = format!("{}{}", base_url.trim_end_matches('/'), path);

                match client.post(&url).json(&body).send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        if status >= 200 && status < 300 {
                            metrics.record_success();
                        } else if status >= 400 && status < 500 {
                            metrics.record_client_error();
                        } else {
                            metrics.record_server_error();
                        }
                    },
                    Err(_) => {
                        metrics.record_connection_error();
                    },
                }
            }));
        }

        // Wait for all tasks with timeout detection
        let wait_timeout = Duration::from_secs(60);
        let wait_result =
            tokio::time::timeout(wait_timeout, futures::future::join_all(handles)).await;

        match wait_result {
            Ok(results) => {
                for result in results {
                    if result.is_err() {
                        metrics.record_panic();
                    }
                }
            },
            Err(_) => {
                panic!(
                    "Test timed out after {:?}. Possible hang in concurrent module loading.",
                    wait_timeout
                );
            },
        }

        let summary = metrics.summary();
        println!("{}", summary);

        // Assertions
        assert_eq!(
            summary.panics, 0,
            "CVE-2024-47813: No tasks should panic during concurrent module loading"
        );

        assert!(
            summary.error_rate <= config.max_error_rate,
            "CVE-2024-47813: Error rate {:.2}% exceeds maximum {:.2}%",
            summary.error_rate * 100.0,
            config.max_error_rate * 100.0
        );

        let expected_total = config.num_tasks as u64;
        assert_eq!(
            summary.total_requests, expected_total,
            "All {} tasks should complete, got {}",
            expected_total, summary.total_requests
        );

        // Verify host is still healthy
        let health = host.get("/health").await.expect("Health check failed");
        assert_eq!(health.status(), 200);

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let total_elapsed = test_start.elapsed();
    println!(
        "\nCVE-2024-47813 different modules test completed in {:?}",
        total_elapsed
    );
    assert!(
        total_elapsed.as_secs() < config.test_timeout_secs,
        "Test took too long ({:?})",
        total_elapsed
    );
}

// =============================================================================
// Additional Concurrent Loading Stress Tests
// =============================================================================

/// Rapid fire concurrent requests to stress module loading under sustained load.
///
/// Unlike the barrier-synchronized tests above, this test fires requests
/// continuously without synchronization to simulate real-world traffic patterns
/// where requests arrive asynchronously.
#[tokio::test]
#[ignore = "Resource-intensive stress test"]
async fn test_rapid_concurrent_module_access() {
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm not found. Run build script first.");
        return;
    }

    println!("Rapid Concurrent Module Access Test");

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_execution_timeout(30)
        .with_max_concurrent_requests(200)
        .start()
        .await
        .expect("Failed to start host");

    let metrics = ConcurrentMetrics::new();
    let num_requests = 500;
    let max_concurrent = 50;

    // Use a semaphore to limit concurrent requests
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));

    let start = Instant::now();
    let mut handles = Vec::with_capacity(num_requests);

    for i in 0..num_requests {
        let url = host.url("/run/echo/");
        let client = host.client().clone();
        let metrics = metrics.clone();
        let semaphore = semaphore.clone();

        handles.push(tokio::spawn(async move {
            let _permit = semaphore.acquire().await.expect("Semaphore closed");

            let body = serde_json::json!({"request_id": i});

            match client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if status >= 200 && status < 300 {
                        metrics.record_success();
                    } else if status >= 400 && status < 500 {
                        metrics.record_client_error();
                    } else {
                        metrics.record_server_error();
                    }
                },
                Err(_) => {
                    metrics.record_connection_error();
                },
            }
        }));
    }

    // Wait for all requests
    let wait_result =
        tokio::time::timeout(Duration::from_secs(120), futures::future::join_all(handles)).await;

    let elapsed = start.elapsed();

    match wait_result {
        Ok(results) => {
            for result in results {
                if result.is_err() {
                    metrics.record_panic();
                }
            }
        },
        Err(_) => {
            panic!("Rapid fire test timed out");
        },
    }

    let summary = metrics.summary();
    println!("{}", summary);
    println!("Total time: {:?}", elapsed);
    println!(
        "Requests per second: {:.2}",
        num_requests as f64 / elapsed.as_secs_f64()
    );

    assert_eq!(summary.panics, 0, "No tasks should panic");
    assert!(
        summary.error_rate <= 0.10,
        "Error rate {:.2}% too high",
        summary.error_rate * 100.0
    );
    assert_eq!(
        summary.total_requests, num_requests as u64,
        "All requests should complete"
    );

    // Verify host is healthy
    let health = host.get("/health").await.expect("Health check failed");
    assert_eq!(health.status(), 200);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quick sanity check that metrics tracking works correctly.
    #[test]
    fn test_concurrent_metrics_tracking() {
        let metrics = ConcurrentMetrics::new();

        metrics.record_success();
        metrics.record_success();
        metrics.record_client_error();
        metrics.record_server_error();
        metrics.record_connection_error();
        metrics.record_panic();

        let summary = metrics.summary();

        assert_eq!(summary.total_requests, 5);
        assert_eq!(summary.successful_responses, 2);
        assert_eq!(summary.client_errors, 1);
        assert_eq!(summary.server_errors, 1);
        assert_eq!(summary.connection_errors, 1);
        assert_eq!(summary.panics, 1);
    }

    /// Test error rate calculation.
    #[test]
    fn test_concurrent_error_rate_calculation() {
        let metrics = ConcurrentMetrics::new();

        // 90 successes, 5 server errors, 5 connection errors = 10% error rate
        for _ in 0..90 {
            metrics.record_success();
        }
        for _ in 0..5 {
            metrics.record_server_error();
        }
        for _ in 0..5 {
            metrics.record_connection_error();
        }

        let summary = metrics.summary();

        assert_eq!(summary.total_requests, 100);
        assert!((summary.error_rate - 0.10).abs() < 0.001);
    }
}
