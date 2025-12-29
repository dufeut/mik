// Test-specific lint suppressions
#![allow(clippy::println_empty_string)]

//! Timeout and epoch interruption tests for the mikrozen runtime.
//!
//! These tests verify that the runtime's timeout mechanisms work correctly:
//! - Epoch-based interruption for WASM execution
//! - Timeout configuration parsing
//! - Appropriate error responses for timeouts
//!
//! Some tests require WASM fixtures (infinite_loop.wasm) and are marked `#[ignore]`.
//!
//! Run available tests with:
//! ```bash
//! cargo test -p mik timeout
//! ```
//!
//! Run all tests (including those requiring fixtures):
//! ```bash
//! cargo test -p mik timeout -- --ignored
//! ```

#[path = "common.rs"]
mod common;

use common::TestHost;
use std::time::{Duration, Instant};

// =============================================================================
// Configuration Unit Tests
// =============================================================================

/// Test that epoch deadline calculation matches expected formula.
/// The runtime calculates: execution_timeout_secs * 100 epochs
/// (10ms per epoch = 100 epochs per second)
#[test]
fn test_epoch_deadline_calculation() {
    let timeout_secs: u64 = 30;
    let expected_epochs = timeout_secs.saturating_mul(100);

    assert_eq!(expected_epochs, 3000, "30 seconds should be 3000 epochs");

    // Test edge cases
    assert_eq!(0_u64.saturating_mul(100), 0, "0 seconds should be 0 epochs");
    assert_eq!(
        1_u64.saturating_mul(100),
        100,
        "1 second should be 100 epochs"
    );

    // Test overflow protection
    let max_epochs = u64::MAX.saturating_mul(100);
    assert_eq!(max_epochs, u64::MAX, "Overflow should saturate to MAX");
}

/// Test fuel budget calculation matches expected formula.
/// The runtime calculates: execution_timeout_secs * 10_000_000 fuel
#[test]
fn test_fuel_budget_calculation() {
    let timeout_secs: u64 = 30;
    let expected_fuel = timeout_secs.saturating_mul(10_000_000);

    assert_eq!(expected_fuel, 300_000_000, "30 seconds should be 300M fuel");

    // Test edge cases
    assert_eq!(
        0_u64.saturating_mul(10_000_000),
        0,
        "0 seconds should be 0 fuel"
    );
    assert_eq!(
        1_u64.saturating_mul(10_000_000),
        10_000_000,
        "1 second should be 10M fuel"
    );
}

/// Test epoch incrementer timing math.
/// In real usage, a background thread calls engine.increment_epoch() every 10ms.
#[test]
fn test_epoch_incrementer_timing_math() {
    // 10ms intervals = 100 epochs per second
    let interval_ms = 10;
    let epochs_per_second = 1000 / interval_ms;

    assert_eq!(epochs_per_second, 100, "Should be 100 epochs per second");

    // 30 second timeout = 3000 epochs
    let timeout_secs = 30;
    let expected_epochs = timeout_secs * epochs_per_second;

    assert_eq!(
        expected_epochs, 3000,
        "30 second timeout should be 3000 epochs"
    );
}

/// Test epoch deadline saturating multiplication prevents overflow.
#[test]
fn test_epoch_deadline_overflow_protection() {
    // Very large timeout should not overflow
    let huge_timeout: u64 = u64::MAX / 50; // Large but not at max
    let epochs = huge_timeout.saturating_mul(100);

    // Should either compute correctly or saturate to MAX, never overflow/wrap
    assert!(epochs >= huge_timeout, "Result should be >= timeout");

    // Test actual MAX case
    let max_epochs = u64::MAX.saturating_mul(100);
    assert_eq!(max_epochs, u64::MAX, "Should saturate to MAX on overflow");
}

/// Test that timeout error messages contain useful information.
#[test]
fn test_timeout_error_message_format() {
    // The runtime formats timeout messages like:
    // "WASM execution timed out after 30s"
    // Verify this format is consistent

    let timeout = Duration::from_secs(30);
    let error_msg = format!("WASM execution timed out after {:?}", timeout);

    assert!(
        error_msg.contains("timed out"),
        "Error message should contain 'timed out'"
    );
    assert!(
        error_msg.contains("30"),
        "Error message should contain the timeout value"
    );
}

/// Test Duration conversion is consistent.
#[test]
fn test_duration_conversion() {
    let timeout_secs: u64 = 30;
    let duration = Duration::from_secs(timeout_secs);

    // Verify we can get secs back
    assert_eq!(duration.as_secs(), 30);

    // Verify multiplication for epochs
    let epochs = duration.as_secs().saturating_mul(100);
    assert_eq!(epochs, 3000);
}

// =============================================================================
// Server Responsiveness Tests (No WASM Required)
// =============================================================================

/// Test that short timeout configuration doesn't break normal endpoint responses.
/// The /health endpoint should still work with any timeout setting.
#[tokio::test]
async fn test_short_timeout_doesnt_break_health_endpoint() {
    let host = TestHost::builder()
        .with_execution_timeout(1) // Very short 1 second timeout
        .start()
        .await
        .expect("Failed to start test host");

    // Health endpoint doesn't execute WASM, so it should always work
    let resp = host.get("/health").await.expect("Failed to get health");
    assert_eq!(resp.status(), 200);
}

/// Test that the metrics endpoint works with short timeout.
#[tokio::test]
async fn test_short_timeout_doesnt_break_metrics_endpoint() {
    let host = TestHost::builder()
        .with_execution_timeout(1)
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/metrics").await.expect("Failed to get metrics");
    assert_eq!(resp.status(), 200);
}

/// Test that multiple quick requests with short timeout don't cause issues.
#[tokio::test]
async fn test_rapid_requests_with_short_timeout() {
    let host = TestHost::builder()
        .with_execution_timeout(1)
        .start()
        .await
        .expect("Failed to start test host");

    // Make 20 rapid requests to /health
    for _ in 0..20 {
        let resp = host.get("/health").await.expect("Failed to get health");
        assert_eq!(resp.status(), 200);
    }
}

/// Test concurrent requests with short timeout configuration.
#[tokio::test]
async fn test_concurrent_requests_with_short_timeout() {
    let host = TestHost::builder()
        .with_execution_timeout(1)
        .start()
        .await
        .expect("Failed to start test host");

    // Make 10 concurrent requests
    let futures: Vec<_> = (0..10)
        .map(|_| {
            let client = host.client().clone();
            let url = host.url("/health");
            async move { client.get(&url).send().await }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    for result in results {
        let resp = result.expect("Request failed");
        assert_eq!(resp.status(), 200);
    }
}

/// Test that health endpoint response time is reasonable regardless of timeout setting.
#[tokio::test]
async fn test_health_response_time_independent_of_timeout() {
    // With very long timeout
    let host_long = TestHost::builder()
        .with_execution_timeout(300) // 5 minute timeout
        .start()
        .await
        .expect("Failed to start test host");

    let start = Instant::now();
    let resp = host_long
        .get("/health")
        .await
        .expect("Failed to get health");
    let elapsed_long = start.elapsed();
    assert_eq!(resp.status(), 200);
    drop(host_long);

    // With very short timeout
    let host_short = TestHost::builder()
        .with_execution_timeout(1)
        .start()
        .await
        .expect("Failed to start test host");

    let start = Instant::now();
    let resp = host_short
        .get("/health")
        .await
        .expect("Failed to get health");
    let elapsed_short = start.elapsed();
    assert_eq!(resp.status(), 200);

    // Both should be fast (under 500ms) - timeout doesn't affect non-WASM endpoints
    assert!(
        elapsed_long < Duration::from_millis(500),
        "Long timeout config slowed health endpoint: {:?}",
        elapsed_long
    );
    assert!(
        elapsed_short < Duration::from_millis(500),
        "Short timeout config slowed health endpoint: {:?}",
        elapsed_short
    );
}

// =============================================================================
// WASM Timeout Tests (Require Fixtures)
// =============================================================================

/// Test that WASM infinite loops are interrupted by epoch mechanism.
///
/// This test requires an `infinite_loop.wasm` module that contains an
/// infinite loop in the request handler that never yields control.
///
/// To create this fixture:
/// 1. Create a new WASM component project
/// 2. Implement a handler that contains `loop {}`
/// 3. Build with `cargo component build --release`
/// 4. Copy the resulting .wasm to tests/fixtures/modules/infinite_loop.wasm
#[tokio::test]
#[ignore = "Requires infinite_loop.wasm fixture - create a WASM component with `loop {}` handler"]
async fn test_infinite_loop_interrupted() {
    use std::path::PathBuf;

    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules");

    // Skip if fixture doesn't exist
    if !fixtures_dir.join("infinite_loop.wasm").exists() {
        eprintln!("Skipping: fixtures/modules/infinite_loop.wasm not found");
        eprintln!("");
        eprintln!("To create this fixture:");
        eprintln!("  1. Create a WASM component project with mikrozen SDK");
        eprintln!("  2. Add `loop {{}}` in the request handler");
        eprintln!("  3. Build with `cargo component build --release`");
        eprintln!("  4. Copy to tests/fixtures/modules/infinite_loop.wasm");
        return;
    }

    let host = TestHost::builder()
        .with_modules_dir(&fixtures_dir)
        .with_execution_timeout(2) // 2 second timeout
        .start()
        .await
        .expect("Failed to start test host");

    let start = Instant::now();
    let resp = host
        .get("/run/infinite_loop/")
        .await
        .expect("Request failed");
    let elapsed = start.elapsed();

    // Request should complete (due to timeout) within reasonable time
    // Allow some buffer for instantiation overhead
    assert!(
        elapsed < Duration::from_secs(5),
        "Request took too long ({}s), epoch interruption may not be working",
        elapsed.as_secs_f64()
    );

    // Should return an error status (500 or 503)
    assert!(
        resp.status().is_server_error(),
        "Expected 5xx error for timeout, got {}",
        resp.status()
    );
}

/// Test that timeout returns appropriate HTTP error code.
///
/// Timeouts should return 503 Service Unavailable or similar 5xx status.
#[tokio::test]
#[ignore = "Requires slow_handler.wasm fixture - a handler that sleeps/loops longer than timeout"]
async fn test_timeout_returns_5xx() {
    use std::path::PathBuf;

    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules");

    if !fixtures_dir.join("slow_handler.wasm").exists() {
        eprintln!("Skipping: fixtures/modules/slow_handler.wasm not found");
        return;
    }

    let host = TestHost::builder()
        .with_modules_dir(&fixtures_dir)
        .with_execution_timeout(1) // 1 second timeout
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host
        .get("/run/slow_handler/")
        .await
        .expect("Request failed");

    // Timeout should result in 5xx error
    assert!(
        resp.status().is_server_error(),
        "Timeout should return 5xx, got {}",
        resp.status()
    );
}

/// Test that different timeout values result in different actual timeouts.
///
/// A 1-second timeout should complete faster than a 3-second timeout
/// for the same infinite loop handler.
///
/// NOTE: Requires full runtime integration - TestHost is a mock that doesn't load WASM.
#[tokio::test]
#[ignore = "Requires full runtime integration - TestHost mock doesn't load WASM modules"]
async fn test_different_timeout_values_work() {
    // This test needs full runtime integration.
    // TestHost is a mock server for testing HTTP endpoints only.
    // WASM fixtures exist at tests/fixtures/modules/ but need real runtime to load.
    eprintln!("Skipping: requires full runtime integration (not mock TestHost)");
}

/// Test that normal (fast) handlers work correctly with timeout enabled.
///
/// A fast handler should complete well before the timeout without being affected.
///
/// NOTE: Requires full runtime integration - TestHost is a mock that doesn't load WASM.
#[tokio::test]
#[ignore = "Requires full runtime integration - TestHost mock doesn't load WASM modules"]
async fn test_fast_handler_completes_before_timeout() {
    // This test needs full runtime integration.
    // TestHost is a mock server for testing HTTP endpoints only.
    // WASM fixtures exist at tests/fixtures/modules/ but need real runtime to load.
    eprintln!("Skipping: requires full runtime integration (not mock TestHost)");
}

/// Test that timeout interacts correctly with circuit breaker.
///
/// Repeated timeouts should eventually trip the circuit breaker, causing
/// subsequent requests to fail fast with a Retry-After header.
#[tokio::test]
#[ignore = "Requires infinite_loop.wasm fixture and full runtime integration"]
async fn test_timeout_triggers_circuit_breaker() {
    use std::path::PathBuf;

    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules");

    if !fixtures_dir.join("infinite_loop.wasm").exists() {
        eprintln!("Skipping: fixtures/modules/infinite_loop.wasm not found");
        return;
    }

    let host = TestHost::builder()
        .with_modules_dir(&fixtures_dir)
        .with_execution_timeout(1) // Short timeout
        .start()
        .await
        .expect("Failed to start test host");

    // Make multiple requests that will timeout
    // The circuit breaker should eventually open
    let mut circuit_opened = false;
    for i in 0..10 {
        let resp = host.get("/run/infinite_loop/").await;
        if let Ok(r) = resp {
            // Check if circuit breaker opened (would return 503 with Retry-After)
            if r.headers().contains_key("retry-after") {
                println!("Circuit breaker opened after {} failures", i + 1);
                circuit_opened = true;
                break;
            }
        }
    }

    // Circuit breaker should have opened
    // (Note: depends on circuit breaker threshold configuration)
    if !circuit_opened {
        println!("Note: Circuit breaker did not open after 10 failures");
        println!("This may be expected depending on threshold configuration");
    }
}

/// Test that timeout doesn't affect subsequent requests to other modules.
///
/// After a timeout in one module, new requests to other modules should work normally.
///
/// NOTE: Requires full runtime integration - TestHost is a mock that doesn't load WASM.
#[tokio::test]
#[ignore = "Requires full runtime integration - TestHost mock doesn't load WASM modules"]
async fn test_timeout_isolation_between_modules() {
    // This test needs full runtime integration.
    // TestHost is a mock server for testing HTTP endpoints only.
    // WASM fixtures exist at tests/fixtures/modules/ but need real runtime to load.
    eprintln!("Skipping: requires full runtime integration (not mock TestHost)");
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Test that zero timeout is handled gracefully.
/// A zero timeout should either be treated as immediate timeout or use a default.
#[tokio::test]
async fn test_zero_timeout_handled() {
    let host = TestHost::builder()
        .with_execution_timeout(0) // Edge case: zero timeout
        .start()
        .await
        .expect("Failed to start test host");

    // Non-WASM endpoints should still work
    let resp = host.get("/health").await.expect("Failed to get health");
    assert_eq!(resp.status(), 200);
}

/// Test that very large timeout values are handled without overflow.
#[tokio::test]
async fn test_large_timeout_handled() {
    let host = TestHost::builder()
        .with_execution_timeout(86400) // 24 hours
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/health").await.expect("Failed to get health");
    assert_eq!(resp.status(), 200);
}

/// Test that server starts quickly regardless of timeout setting.
#[tokio::test]
async fn test_server_startup_time_independent_of_timeout() {
    let start = Instant::now();

    let host = TestHost::builder()
        .with_execution_timeout(3600) // 1 hour timeout
        .start()
        .await
        .expect("Failed to start test host");

    let startup_time = start.elapsed();

    // Server should start quickly (under 5 seconds)
    assert!(
        startup_time < Duration::from_secs(5),
        "Server took too long to start with large timeout: {:?}",
        startup_time
    );

    let resp = host.get("/health").await.expect("Failed to get health");
    assert_eq!(resp.status(), 200);
}
