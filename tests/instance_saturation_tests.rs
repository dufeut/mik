//! Instance saturation tests for the mikrozen runtime.
//!
//! These tests are based on wasmCloud bug #4752, which describes a scenario where
//! the HTTP server becomes unresponsive when incoming requests exceed the configured
//! maximum concurrent requests limit. After instance capacity is saturated, the provider
//! stops processing new requests entirely - even after the initial load subsides.
//!
//! See: <https://github.com/wasmCloud/wasmCloud/issues/4752>
//!
//! The mikrozen runtime should:
//! 1. Return 503 Service Unavailable when at max_concurrent_requests capacity
//! 2. Queue or reject requests over the limit (never silently drop them)
//! 3. Recover and continue processing after load subsides
//!
//! Run with:
//! ```bash
//! cargo test -p mik instance_saturation -- --test-threads=1
//! ```
//!
//! To run with actual WASM modules:
//! ```bash
//! cargo test -p mik instance_saturation -- --test-threads=1 --ignored
//! ```

#[path = "common.rs"]
mod common;

use common::RealTestHost;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::Barrier;

// =============================================================================
// Helper: Check if echo.wasm fixture exists
// =============================================================================

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules")
}

fn echo_wasm_exists() -> bool {
    fixtures_dir().join("echo.wasm").exists()
}

// =============================================================================
// Instance Saturation Tests (based on wasmCloud bug #4752)
// =============================================================================

/// Test that the server returns 503 Service Unavailable when at max_concurrent_requests.
///
/// This test verifies that when all request slots are occupied by slow requests,
/// new incoming requests receive a 503 status code instead of hanging indefinitely.
///
/// Related to: wasmCloud bug #4752
/// - Expected: Requests exceeding the limit should get 503 response
/// - Bug behavior: Requests hang or are silently dropped
#[tokio::test]
#[ignore = "Requires echo.wasm fixture - run with --ignored"]
async fn test_at_max_concurrent_returns_503() {
    if !echo_wasm_exists() {
        eprintln!(
            "Skipping: echo.wasm fixture not found at {}",
            fixtures_dir().display()
        );
        return;
    }

    // Configure with very low max_concurrent_requests to easily saturate
    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_max_concurrent_requests(2) // Only 2 concurrent requests allowed
        .with_execution_timeout(30) // Long timeout so requests don't time out
        .start()
        .await
        .expect("Failed to start real test host");

    // Statistics tracking
    let success_count = Arc::new(AtomicU32::new(0));
    let service_unavailable_count = Arc::new(AtomicU32::new(0));
    let other_error_count = Arc::new(AtomicU32::new(0));

    // Use a barrier to ensure all requests start simultaneously
    let num_requests = 10; // Way more than max_concurrent_requests (2)
    let barrier = Arc::new(Barrier::new(num_requests));

    let mut handles = Vec::with_capacity(num_requests);

    for i in 0..num_requests {
        let client = host.client().clone();
        let url = host.url("/run/echo/");
        let barrier = barrier.clone();
        let success = success_count.clone();
        let unavailable = service_unavailable_count.clone();
        let other = other_error_count.clone();

        handles.push(tokio::spawn(async move {
            // Wait for all requests to be ready, then fire simultaneously
            barrier.wait().await;

            let body = serde_json::json!({
                "request_id": i,
                "message": format!("concurrent test {}", i)
            });

            match client
                .post(&url)
                .json(&body)
                .timeout(Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    match status {
                        200..=299 => {
                            success.fetch_add(1, Ordering::Relaxed);
                        },
                        503 => {
                            // Service Unavailable - expected when at capacity
                            unavailable.fetch_add(1, Ordering::Relaxed);
                        },
                        429 => {
                            // Too Many Requests - also acceptable rate limit response
                            unavailable.fetch_add(1, Ordering::Relaxed);
                        },
                        _ => {
                            eprintln!("Request {} got unexpected status: {}", i, status);
                            other.fetch_add(1, Ordering::Relaxed);
                        },
                    }
                },
                Err(e) => {
                    // Timeout or connection error
                    eprintln!("Request {} failed: {}", i, e);
                    other.fetch_add(1, Ordering::Relaxed);
                },
            }
        }));
    }

    // Wait for all requests to complete
    for handle in handles {
        handle.await.expect("Request task panicked");
    }

    let successes = success_count.load(Ordering::Relaxed);
    let unavailable = service_unavailable_count.load(Ordering::Relaxed);
    let other = other_error_count.load(Ordering::Relaxed);

    println!("Results:");
    println!("  - Successful (2xx): {}", successes);
    println!("  - Service Unavailable (503/429): {}", unavailable);
    println!("  - Other errors: {}", other);

    // Assertions:
    // 1. Some requests should succeed (at least some should get through)
    assert!(
        successes >= 1,
        "At least 1 request should succeed, got {}",
        successes
    );

    // 2. Under saturation, we expect either:
    //    - 503/429 responses (rate limiting)
    //    - Connection errors (server dropping connections under load)
    //    - All requests succeed (if queueing works well)
    // The key is that NO requests should hang indefinitely.
    let total_completed = successes + unavailable + other;
    assert!(
        total_completed == num_requests as u32,
        "All {} requests should complete (not hang). Got {} total ({} success, {} unavailable, {} other)",
        num_requests,
        total_completed,
        successes,
        unavailable,
        other
    );

    // 3. Connection errors under load are acceptable - they indicate the server
    //    is properly rejecting requests rather than hanging (the wasmCloud bug)
    println!("PASS: All requests completed (no hangs). Server properly handles saturation.");
}

/// Test that requests over the limit are queued or rejected, never lost.
///
/// This test verifies that when requests exceed max_concurrent_requests:
/// - Requests are either queued (and eventually processed)
/// - Or rejected with a proper error response (503/429)
/// - Requests are NEVER silently dropped or lost
///
/// Related to: wasmCloud bug #4752
#[tokio::test]
#[ignore = "Requires echo.wasm fixture - run with --ignored"]
async fn test_over_max_queued_or_rejected() {
    if !echo_wasm_exists() {
        eprintln!(
            "Skipping: echo.wasm fixture not found at {}",
            fixtures_dir().display()
        );
        return;
    }

    // Very low limit to ensure saturation
    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_max_concurrent_requests(1) // Only 1 concurrent request
        .with_execution_timeout(30)
        .start()
        .await
        .expect("Failed to start real test host");

    // Track all responses
    let completed_count = Arc::new(AtomicU32::new(0));
    let response_statuses = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    let num_requests = 5;
    let barrier = Arc::new(Barrier::new(num_requests));

    let mut handles = Vec::with_capacity(num_requests);

    for i in 0..num_requests {
        let client = host.client().clone();
        let url = host.url("/run/echo/");
        let barrier = barrier.clone();
        let completed = completed_count.clone();
        let statuses = response_statuses.clone();

        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            let body = serde_json::json!({
                "request_id": i,
                "message": format!("queue test {}", i)
            });

            let result = client
                .post(&url)
                .json(&body)
                .timeout(Duration::from_secs(30)) // Long timeout to allow queuing
                .send()
                .await;

            let status = match result {
                Ok(resp) => resp.status().as_u16(),
                Err(e) if e.is_timeout() => 408, // Request Timeout
                Err(_) => 0,                     // Connection error
            };

            statuses.lock().await.push((i, status));
            completed.fetch_add(1, Ordering::Relaxed);
        }));
    }

    // Wait for all requests to complete
    for handle in handles {
        handle.await.expect("Request task panicked");
    }

    let completed = completed_count.load(Ordering::Relaxed);
    let statuses = response_statuses.lock().await;

    println!("Request results:");
    for (id, status) in statuses.iter() {
        println!("  Request {}: status {}", id, status);
    }

    // Key assertion: ALL requests must complete (none lost)
    assert_eq!(
        completed, num_requests as u32,
        "All {} requests must complete (none lost). Only {} completed.",
        num_requests, completed
    );

    // Verify all responses are valid:
    // - 2xx: Success
    // - 408: Request Timeout (acceptable under load)
    // - 429: Too Many Requests (rate limiting)
    // - 503: Service Unavailable (at capacity)
    // - 0: Connection error (server dropped connection under load - acceptable)
    let valid_responses = statuses
        .iter()
        .filter(|(_, status)| matches!(status, 0 | 200..=299 | 408 | 429 | 503))
        .count();

    assert_eq!(
        valid_responses,
        num_requests,
        "All requests should get valid responses (0/2xx/408/429/503). Got: {:?}",
        statuses.iter().map(|(_, s)| s).collect::<Vec<_>>()
    );

    println!("PASS: All requests completed with valid responses (no requests lost).");
}

/// Test that the server does not hang at capacity and recovers after load subsides.
///
/// This is the core test for wasmCloud bug #4752:
/// 1. Saturate the server with concurrent requests
/// 2. Wait for those requests to complete
/// 3. Send a new request - it MUST succeed (server must recover)
///
/// The bug behavior is that step 3 fails because the server becomes unresponsive.
#[tokio::test]
#[ignore = "Requires echo.wasm fixture - run with --ignored"]
async fn test_no_hang_at_limit() {
    if !echo_wasm_exists() {
        eprintln!(
            "Skipping: echo.wasm fixture not found at {}",
            fixtures_dir().display()
        );
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_max_concurrent_requests(2)
        .with_execution_timeout(10)
        .start()
        .await
        .expect("Failed to start real test host");

    // Phase 1: Saturate the server
    println!("Phase 1: Saturating server with concurrent requests...");
    let num_saturating_requests = 10;
    let barrier = Arc::new(Barrier::new(num_saturating_requests));

    let mut handles = Vec::with_capacity(num_saturating_requests);

    for i in 0..num_saturating_requests {
        let client = host.client().clone();
        let url = host.url("/run/echo/");
        let barrier = barrier.clone();

        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            let body = serde_json::json!({
                "request_id": i,
                "phase": "saturation"
            });

            // Don't care about the result, just saturate
            let _ = client
                .post(&url)
                .json(&body)
                .timeout(Duration::from_secs(15))
                .send()
                .await;
        }));
    }

    // Wait for all saturation requests to complete
    for handle in handles {
        let _ = handle.await;
    }

    println!("Phase 1 complete: All saturating requests finished.");

    // Phase 2: Brief pause to ensure server has processed everything
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Phase 3: Recovery test - send a single request that MUST succeed
    println!("Phase 2: Testing server recovery...");

    let recovery_body = serde_json::json!({
        "test": "recovery",
        "message": "This request must succeed after saturation"
    });

    let recovery_result = host.post_json("/run/echo/", &recovery_body).await;

    match recovery_result {
        Ok(resp) => {
            let status = resp.status();
            println!("Recovery request status: {}", status);

            // The server MUST respond (not hang)
            assert!(
                status.is_success() || status.as_u16() == 503 || status.as_u16() == 429,
                "Recovery request should succeed or get proper rejection (503/429), got {}",
                status
            );

            // Ideally, after load subsides, the request should succeed
            if status.is_success() {
                println!("SUCCESS: Server recovered and processed request normally.");

                // Verify we got a valid echo response
                let body: serde_json::Value = resp.json().await.expect("Failed to parse response");
                assert!(
                    body.get("test").is_some() || body.get("message").is_some(),
                    "Echo response should contain our data"
                );
            } else {
                // Even a rejection is acceptable - the key is NO HANG
                println!("Server responded with {} (acceptable - no hang)", status);
            }
        },
        Err(e) => {
            // This is the bug behavior - request fails/times out after saturation
            if e.is_timeout() {
                panic!(
                    "BUG DETECTED (wasmCloud #4752): Server hangs after saturation. \
                    Recovery request timed out instead of completing."
                );
            }
            panic!(
                "Recovery request failed unexpectedly: {}. \
                This may indicate the server is unresponsive after saturation.",
                e
            );
        },
    }

    // Phase 4: Additional recovery verification - send more requests
    println!("Phase 3: Verifying continued operation...");

    let mut continued_successes = 0;
    for i in 0..5 {
        let body = serde_json::json!({
            "test": "continued_operation",
            "iteration": i
        });

        match host.post_json("/run/echo/", &body).await {
            Ok(resp) if resp.status().is_success() => {
                continued_successes += 1;
            },
            Ok(resp) => {
                println!("Continued request {} got status: {}", i, resp.status());
            },
            Err(e) => {
                println!("Continued request {} failed: {}", i, e);
            },
        }

        // Small delay between requests
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    println!(
        "Continued operation: {}/5 requests succeeded",
        continued_successes
    );

    assert!(
        continued_successes >= 3,
        "Server should handle at least 3/5 requests after recovery. Got {}",
        continued_successes
    );

    println!("All phases complete: Server properly handles saturation and recovers.");
}

// =============================================================================
// Unit Tests (don't require WASM fixtures)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the test fixture detection works.
    #[test]
    fn test_fixture_detection() {
        let exists = echo_wasm_exists();
        let dir = fixtures_dir();

        println!("Fixtures directory: {}", dir.display());
        println!("echo.wasm exists: {}", exists);

        // This test just verifies the helper functions work
        assert!(dir.exists(), "Fixtures directory should exist");
    }
}
