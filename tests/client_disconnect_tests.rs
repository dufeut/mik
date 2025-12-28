//! Client disconnect handling tests for the mikrozen runtime.
//!
//! These tests validate the runtime's behavior when HTTP clients disconnect
//! mid-request, based on wasmCloud bug #3920:
//! <https://github.com/wasmCloud/wasmCloud/issues/3920>
//!
//! The bug in wasmCloud causes HTTP component instances to hang indefinitely when:
//! - A client cancels an HTTP request after consuming response headers
//! - But before consuming the response body
//! - The component fails to clean up and hangs forever
//!
//! These tests ensure mikrozen-host handles client disconnects gracefully:
//! - No hanging when clients disconnect mid-request
//! - Proper resource cleanup after disconnect
//! - Server continues accepting new requests after disconnect
//!
//! # Running the tests
//!
//! Most tests are marked `#[ignore]` because they require the `slow_response.wasm`
//! fixture which doesn't exist yet. To run them once the fixture is available:
//!
//! ```bash
//! cargo test -p mik client_disconnect -- --ignored --test-threads=1
//! ```
//!
//! The non-ignored tests use the mock `TestHost` and can be run normally:
//!
//! ```bash
//! cargo test -p mik client_disconnect
//! ```

#[path = "common.rs"]
mod common;

use common::{RealTestHost, TestHost};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// =============================================================================
// Client Disconnect Tests - Mock Server
// =============================================================================

/// Test that the mock server handles client disconnects gracefully.
///
/// This test validates the basic pattern using the mock `TestHost`.
/// It starts a request and then drops the client before completion.
#[tokio::test]
async fn test_client_disconnect_mock_server() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    // Start multiple requests and drop some mid-flight
    let mut handles = Vec::new();

    for i in 0..10 {
        let client = host.client().clone();
        let url = host.url("/health");

        handles.push(tokio::spawn(async move {
            if i % 2 == 0 {
                // These requests complete normally
                let resp = client.get(&url).send().await;
                resp.is_ok()
            } else {
                // These requests are dropped before completion
                let client_with_short_timeout = reqwest::Client::builder()
                    .timeout(Duration::from_millis(1))
                    .build()
                    .unwrap();

                // This will likely timeout/disconnect
                let _ = client_with_short_timeout.get(&url).send().await;
                true // We don't care about the result
            }
        }));
    }

    // Wait for all to complete
    for handle in handles {
        let _ = handle.await;
    }

    // Server should still be responsive after disconnects
    let resp = host
        .get("/health")
        .await
        .expect("Server should still respond");
    assert_eq!(
        resp.status(),
        200,
        "Health check should succeed after disconnects"
    );
}

/// Test that the server recovers from abrupt client disconnects.
///
/// Simulates many clients connecting and immediately disconnecting.
#[tokio::test]
async fn test_abrupt_disconnect_recovery() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    // Simulate 50 abrupt disconnects
    for _ in 0..50 {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(1))
            .build()
            .unwrap();

        // Start request and let it timeout (simulating disconnect)
        let url = host.url("/health");
        let _ = client.get(&url).send().await;
    }

    // Give server a moment to process the disconnects
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Server must still be responsive
    let resp = host
        .get("/health")
        .await
        .expect("Server should recover from disconnects");
    assert_eq!(resp.status(), 200);
}

// =============================================================================
// Client Disconnect Tests - Real WASM Runtime
// =============================================================================

/// Test that client disconnect during slow WASM execution doesn't hang the server.
///
/// This test requires a `slow_response.wasm` fixture that:
/// 1. Delays response for several seconds
/// 2. Returns a large response body (to test mid-body disconnect)
///
/// The test:
/// 1. Starts a request to the slow endpoint
/// 2. Disconnects the client before the response completes
/// 3. Verifies the server doesn't hang and accepts new requests within a timeout
///
/// Based on wasmCloud bug #3920 where cancelling an HTTP request after consuming
/// headers but before the body causes the component to hang forever.
#[tokio::test]
#[ignore = "Requires slow_response.wasm fixture - see tests/fixtures/README.md"]
async fn test_client_disconnect_no_hang() {
    // Check if fixtures exist
    let fixtures_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules");

    if !fixtures_dir.join("slow_response.wasm").exists() {
        eprintln!("Skipping: slow_response.wasm fixture not found");
        eprintln!("Create a WASM module that delays response for 5+ seconds");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(&fixtures_dir)
        .with_execution_timeout(30) // Long timeout so WASM doesn't timeout
        .start()
        .await
        .expect("Failed to start real test host");

    // Create a client with a short timeout to simulate disconnect
    let short_timeout_client = reqwest::Client::builder()
        .timeout(Duration::from_millis(100)) // Very short - will disconnect before slow response
        .build()
        .expect("Failed to create client");

    let url = host.url("/run/slow_response/");

    // Start the slow request - it will timeout (disconnect) before completion
    let start = Instant::now();
    let result = short_timeout_client
        .post(&url)
        .json(&serde_json::json!({"delay_secs": 5}))
        .send()
        .await;

    // The request should have timed out (client disconnected)
    assert!(
        result.is_err() || start.elapsed() < Duration::from_secs(1),
        "Client should have disconnected quickly, not waited for slow response"
    );

    // Critical test: Server must NOT hang and must accept new requests
    // Give it a moment to clean up the cancelled request
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The server should respond to health checks quickly (not hang)
    let health_start = Instant::now();
    let timeout_result = tokio::time::timeout(Duration::from_secs(5), host.get("/health")).await;

    assert!(
        timeout_result.is_ok(),
        "Server hung after client disconnect! No response within 5 seconds"
    );

    let health_response = timeout_result
        .unwrap()
        .expect("Health check should succeed");

    assert_eq!(
        health_response.status(),
        200,
        "Health check should return 200"
    );

    let health_time = health_start.elapsed();
    assert!(
        health_time < Duration::from_secs(2),
        "Health check took too long ({:?}), server may be partially hung",
        health_time
    );

    println!(
        "Server responded to health check in {:?} after client disconnect",
        health_time
    );
}

/// Test that resources are properly cleaned up after client disconnect.
///
/// This test verifies:
/// 1. Memory is released when a client disconnects mid-request
/// 2. No resource leaks accumulate over multiple disconnect cycles
/// 3. The server can handle sustained disconnect patterns
///
/// Requires a `slow_response.wasm` fixture that uses measurable resources.
#[tokio::test]
#[ignore = "Requires slow_response.wasm fixture - see tests/fixtures/README.md"]
async fn test_client_disconnect_cleanup() {
    let fixtures_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules");

    if !fixtures_dir.join("slow_response.wasm").exists() {
        eprintln!("Skipping: slow_response.wasm fixture not found");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(&fixtures_dir)
        .with_execution_timeout(30)
        .start()
        .await
        .expect("Failed to start real test host");

    let disconnect_count = Arc::new(AtomicU64::new(0));
    let num_disconnect_cycles = 10;

    // Perform multiple disconnect cycles
    for cycle in 0..num_disconnect_cycles {
        let short_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();

        let url = host.url("/run/slow_response/");

        // Start request and let it disconnect
        let _ = short_client
            .post(&url)
            .json(&serde_json::json!({"delay_secs": 5}))
            .send()
            .await;

        disconnect_count.fetch_add(1, Ordering::Relaxed);

        // Small delay between cycles
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Every few cycles, verify server is still healthy
        if cycle % 3 == 0 {
            let health_result =
                tokio::time::timeout(Duration::from_secs(2), host.get("/health")).await;

            assert!(
                health_result.is_ok(),
                "Server became unresponsive after {} disconnect cycles",
                cycle + 1
            );
        }
    }

    // Wait for any cleanup to complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Final health check
    let final_health = tokio::time::timeout(Duration::from_secs(5), host.get("/health"))
        .await
        .expect("Server hung after disconnect cycles")
        .expect("Health check failed");

    assert_eq!(final_health.status(), 200);

    // Parse health response to check resource metrics
    let health_body: serde_json::Value = final_health.json().await.expect("Failed to parse health");

    println!(
        "After {} disconnect cycles, server health: {:?}",
        disconnect_count.load(Ordering::Relaxed),
        health_body
    );

    // Verify cache hasn't grown unboundedly (would indicate leak)
    if let Some(cache_size) = health_body.get("cache_size").and_then(|v| v.as_u64()) {
        // Cache should be bounded (not accumulating from disconnected requests)
        assert!(
            cache_size <= 10, // Reasonable bound for test
            "Cache size {} may indicate resource leak after disconnects",
            cache_size
        );
    }

    // Check total requests - should be higher than disconnect count
    // (because we also made health check requests)
    if let Some(total_requests) = health_body.get("total_requests").and_then(|v| v.as_u64()) {
        println!("Total requests processed: {}", total_requests);
    }
}

/// Test concurrent client disconnects don't cause deadlock.
///
/// Multiple clients disconnect simultaneously - server must not deadlock.
#[tokio::test]
#[ignore = "Requires slow_response.wasm fixture - see tests/fixtures/README.md"]
async fn test_concurrent_client_disconnects() {
    let fixtures_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules");

    if !fixtures_dir.join("slow_response.wasm").exists() {
        eprintln!("Skipping: slow_response.wasm fixture not found");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(&fixtures_dir)
        .with_execution_timeout(30)
        .start()
        .await
        .expect("Failed to start real test host");

    let url = host.url("/run/slow_response/");
    let num_concurrent = 20;

    // Use a barrier to synchronize all disconnects
    let barrier = Arc::new(tokio::sync::Barrier::new(num_concurrent));

    let mut handles = Vec::with_capacity(num_concurrent);

    for _ in 0..num_concurrent {
        let url = url.clone();
        let barrier = barrier.clone();

        handles.push(tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_millis(50))
                .build()
                .unwrap();

            // Wait for all tasks to be ready
            barrier.wait().await;

            // All disconnect roughly simultaneously
            let _ = client
                .post(&url)
                .json(&serde_json::json!({"delay_secs": 5}))
                .send()
                .await;
        }));
    }

    // Wait for all disconnects to complete
    for handle in handles {
        let _ = handle.await;
    }

    // Server must not be deadlocked
    let health_result = tokio::time::timeout(Duration::from_secs(10), host.get("/health")).await;

    assert!(
        health_result.is_ok(),
        "Server deadlocked after {} concurrent disconnects!",
        num_concurrent
    );

    let resp = health_result.unwrap().expect("Health check failed");
    assert_eq!(resp.status(), 200);
}

/// Test disconnect during response body streaming.
///
/// This specifically tests the wasmCloud #3920 scenario where the client
/// disconnects AFTER consuming headers but BEFORE consuming the body.
#[tokio::test]
#[ignore = "Requires slow_response.wasm fixture - see tests/fixtures/README.md"]
async fn test_disconnect_during_body_streaming() {
    let fixtures_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules");

    if !fixtures_dir.join("slow_response.wasm").exists() {
        eprintln!("Skipping: slow_response.wasm fixture not found");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(&fixtures_dir)
        .with_execution_timeout(30)
        .start()
        .await
        .expect("Failed to start real test host");

    // Use a longer timeout to receive headers, then drop before body
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let url = host.url("/run/slow_response/");

    // Request a slow, large response
    let request_future = client.post(&url).json(&serde_json::json!({
        "delay_secs": 5,
        "response_size_kb": 1024  // 1MB response body
    }));

    // Start the request
    let response_result = request_future.send().await;

    if let Ok(response) = response_result {
        // We got headers - now simulate disconnect by NOT reading the body
        // and instead dropping the response
        let status = response.status();
        println!("Got response headers with status: {}", status);

        // DROP the response without reading body - this is the wasmCloud #3920 scenario
        drop(response);

        // Give server time to notice the disconnect
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Critical: Server must not hang
    let health_result = tokio::time::timeout(Duration::from_secs(5), host.get("/health")).await;

    assert!(
        health_result.is_ok(),
        "Server hung after client dropped response mid-body (wasmCloud #3920 scenario)"
    );

    let resp = health_result.unwrap().expect("Health check failed");
    assert_eq!(resp.status(), 200);
}

// =============================================================================
// Tests using echo.wasm (available fixture)
// =============================================================================

/// Test client disconnect with the echo module.
///
/// This test uses the available `echo.wasm` fixture to verify basic
/// disconnect handling works without hanging.
#[tokio::test]
async fn test_client_disconnect_echo_module() {
    let fixtures_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules");

    // Skip if echo.wasm doesn't exist
    if !fixtures_dir.join("echo.wasm").exists() {
        eprintln!("Skipping: echo.wasm fixture not found");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(&fixtures_dir)
        .start()
        .await
        .expect("Failed to start real test host");

    // Create multiple very-short-timeout clients to simulate disconnects
    for _ in 0..5 {
        let short_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(1)) // Extremely short
            .build()
            .unwrap();

        let url = host.url("/run/echo/");

        // This will almost certainly timeout (disconnect)
        let _ = short_client
            .post(&url)
            .json(&serde_json::json!({"test": "disconnect"}))
            .send()
            .await;
    }

    // Wait briefly for cleanup
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Server should still work
    let resp = host
        .get("/health")
        .await
        .expect("Server should respond after disconnects");
    assert_eq!(resp.status(), 200);

    // And the echo module should still work
    let echo_resp = host
        .post_json("/run/echo/", &serde_json::json!({"after": "disconnects"}))
        .await
        .expect("Echo module should work after disconnects");

    assert_eq!(echo_resp.status(), 200);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify test utilities work correctly.
    #[test]
    fn test_atomic_counter() {
        let counter = Arc::new(AtomicU64::new(0));
        counter.fetch_add(5, Ordering::Relaxed);
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }
}
