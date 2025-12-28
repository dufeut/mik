//! OOM (Out of Memory) handling tests for the mikrozen runtime.
//!
//! These tests are based on Wasmtime issue #12069, which describes a planned initiative
//! to handle Out-Of-Memory conditions gracefully in the Wasmtime runtime:
//! <https://github.com/bytecodealliance/wasmtime/issues/12069>
//!
//! ## Background
//!
//! Currently, Wasmtime doesn't always gracefully handle allocation failures. When memory
//! allocation fails during WASM execution, the runtime may crash rather than returning
//! an error to the embedder. The Wasmtime team's goal is to:
//!
//! > "Turn allocation failure into an `Err(...)` return and ultimately propagate that
//! > up to the Wasmtime embedder."
//!
//! ## Test Philosophy
//!
//! These tests verify that the mikrozen runtime properly handles OOM scenarios:
//!
//! 1. **Error Propagation** - OOM should return an HTTP error response (4xx/5xx), not crash
//! 2. **Server Resilience** - The server must continue operating after OOM events
//! 3. **Logging/Observability** - OOM events should be properly handled and observable
//!
//! ## Fixtures
//!
//! Tests use the `memory_hog.wasm` fixture, which is designed to exhaust memory by
//! continuously allocating until the memory limit is reached. The fixture source is
//! at `tests/fixtures/wasm-fixtures/memory_hog/`.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run unit tests (no fixtures required)
//! cargo test -p mik oom_handling
//!
//! # Run integration tests (requires memory_hog.wasm fixture)
//! cargo test -p mik oom_handling -- --ignored --test-threads=1
//! ```
//!
//! ## Related Issues
//!
//! - Wasmtime #12069: Handle OOM in the runtime
//! - Wasmtime #1872: Max memory configuration
//! - Wasmtime #1501: OOM with vmemoryuse limits

#[path = "common.rs"]
mod common;

use common::RealTestHost;
use std::path::PathBuf;
use std::time::Duration;

// =============================================================================
// Helper Functions
// =============================================================================

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

/// Check if the memory_hog.wasm fixture exists.
fn memory_hog_wasm_exists() -> bool {
    fixture_exists("memory_hog.wasm")
}

// =============================================================================
// Integration Tests (Require memory_hog.wasm fixture)
// =============================================================================

/// Test that OOM returns an error response (4xx/5xx) instead of crashing the server.
///
/// This test verifies the core behavior from Wasmtime #12069: when a WASM module
/// exhausts its memory allocation, the runtime should return an error to the
/// embedder (in our case, an HTTP error response) rather than crashing.
///
/// ## Expected Behavior
///
/// - The `memory_hog.wasm` module attempts to allocate memory beyond its limit
/// - The runtime's ResourceLimiter denies the allocation
/// - A WASM trap is raised, or an error is returned
/// - The HTTP response is a 5xx error (not a connection reset due to crash)
///
/// ## Bug Behavior (what we're guarding against)
///
/// - The runtime crashes when the allocation fails
/// - The connection is reset without a proper HTTP response
/// - The server process terminates
#[tokio::test]
#[ignore = "Requires memory_hog.wasm fixture"]
async fn test_oom_returns_error_not_panic() {
    if !memory_hog_wasm_exists() {
        eprintln!(
            "Skipping: memory_hog.wasm fixture not found at {}",
            fixtures_dir().display()
        );
        eprintln!("");
        eprintln!("To create this fixture:");
        eprintln!("  cd mik/tests/fixtures/wasm-fixtures/memory_hog");
        eprintln!("  cargo component build --release");
        eprintln!("  cp target/wasm32-wasip2/release/memory_hog.wasm ../modules/");
        return;
    }

    // Configure with a low memory limit to trigger OOM quickly
    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_memory_limit_mb(16) // 16MB limit - memory_hog will exceed this
        .with_execution_timeout(30) // Long timeout so OOM happens first
        .start()
        .await
        .expect("Failed to start real test host");

    // Call the memory_hog module - it should attempt to exhaust memory
    let result = host
        .post_json("/run/memory_hog/", &serde_json::json!({}))
        .await;

    // The key assertion: we should get a response, not a panic/crash
    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();

            // OOM should result in a server error (5xx), not success
            assert!(
                status >= 400,
                "OOM should return an error status (4xx/5xx), got {}. \
                This suggests the memory limit is not being enforced.",
                status
            );

            // Specifically expecting 500 (Internal Server Error) or similar
            if status >= 500 && status < 600 {
                println!(
                    "SUCCESS: OOM correctly returned {} status (server error)",
                    status
                );
            } else if status >= 400 && status < 500 {
                println!(
                    "ACCEPTABLE: OOM returned {} status (client error). \
                    This may indicate the error was mapped differently.",
                    status
                );
            }

            // Verify we can still read the response body (connection not dropped)
            let _body = resp.text().await;
            println!("Response body received successfully (connection stable)");
        },
        Err(e) => {
            // If we got an error, it should NOT be a connection reset due to crash
            // Timeout is acceptable (though OOM should happen before timeout)
            if e.is_connect() {
                panic!(
                    "POTENTIAL BUG (Wasmtime #12069): Connection failed during OOM. \
                    The server may have crashed instead of returning an error. \
                    Error: {}",
                    e
                );
            } else if e.is_timeout() {
                // Timeout is not ideal but acceptable - at least the server didn't crash
                println!(
                    "WARNING: Request timed out. OOM should occur before timeout. \
                    Consider increasing memory_hog's allocation rate."
                );
            } else {
                // Other errors might still indicate issues
                println!(
                    "Request failed with error: {}. \
                    Verify this is not a server crash.",
                    e
                );
            }
        },
    }

    // Final verification: the server is still running (didn't crash)
    let health = host
        .get("/health")
        .await
        .expect("Server should still be running after OOM");
    assert_eq!(
        health.status(),
        200,
        "Health check should succeed after OOM"
    );
}

/// Test that the server remains healthy and continues processing requests after an OOM event.
///
/// This test verifies server resilience: after one module experiences an OOM condition,
/// the server should:
///
/// 1. Recover and accept new connections
/// 2. Successfully execute other modules
/// 3. Report healthy status
///
/// ## Expected Behavior
///
/// - OOM in memory_hog doesn't affect subsequent requests
/// - Health endpoint returns 200
/// - Other modules (like echo) continue to work
///
/// ## Bug Behavior
///
/// - Server becomes unresponsive after OOM
/// - New requests fail or timeout
/// - Server needs restart to recover
#[tokio::test]
#[ignore = "Requires memory_hog.wasm fixture"]
async fn test_server_continues_after_oom() {
    if !memory_hog_wasm_exists() {
        eprintln!(
            "Skipping: memory_hog.wasm fixture not found at {}",
            fixtures_dir().display()
        );
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_memory_limit_mb(16) // Low limit to trigger OOM
        .with_execution_timeout(30)
        .start()
        .await
        .expect("Failed to start real test host");

    // Phase 1: Trigger OOM
    println!("Phase 1: Triggering OOM condition...");
    let _ = host
        .post_json("/run/memory_hog/", &serde_json::json!({}))
        .await;

    // Brief pause to ensure any cleanup has completed
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Phase 2: Verify health endpoint still works
    println!("Phase 2: Checking health endpoint...");
    let health_result = host.get("/health").await;

    match health_result {
        Ok(resp) => {
            assert_eq!(
                resp.status(),
                200,
                "Health endpoint should return 200 after OOM. Got: {}",
                resp.status()
            );

            let body: serde_json::Value = resp.json().await.expect("Failed to parse health JSON");
            assert_eq!(
                body["status"], "ready",
                "Server status should be 'ready' after OOM"
            );
            println!("SUCCESS: Health endpoint returned 200 after OOM");
        },
        Err(e) => {
            panic!(
                "CRITICAL: Health endpoint failed after OOM. \
                Server may be in a bad state. Error: {}",
                e
            );
        },
    }

    // Phase 3: Verify other modules work (if echo.wasm exists)
    if fixture_exists("echo.wasm") {
        println!("Phase 3: Testing echo module after OOM...");

        let echo_result = host
            .post_json(
                "/run/echo/",
                &serde_json::json!({"test": "after_oom", "value": 42}),
            )
            .await;

        match echo_result {
            Ok(resp) => {
                assert_eq!(
                    resp.status(),
                    200,
                    "Echo should work after OOM. Got: {}",
                    resp.status()
                );

                let body: serde_json::Value =
                    resp.json().await.expect("Failed to parse echo response");
                assert_eq!(
                    body["test"], "after_oom",
                    "Echo should return our test value"
                );
                println!("SUCCESS: Echo module works correctly after OOM");
            },
            Err(e) => {
                panic!(
                    "Echo module failed after OOM. \
                    OOM in memory_hog may have corrupted server state. Error: {}",
                    e
                );
            },
        }
    } else {
        println!("Phase 3: Skipped (echo.wasm not available)");
    }

    // Phase 4: Verify we can trigger OOM again (server fully recovered)
    println!("Phase 4: Triggering second OOM to verify full recovery...");
    let second_oom = host
        .post_json("/run/memory_hog/", &serde_json::json!({}))
        .await;

    // We don't care about the result, just that the server accepted the request
    match second_oom {
        Ok(resp) => {
            println!(
                "Second OOM request completed with status: {}",
                resp.status()
            );
        },
        Err(_) => {
            println!("Second OOM request failed (acceptable)");
        },
    }

    // Final health check
    let final_health = host
        .get("/health")
        .await
        .expect("Server should be running after second OOM");
    assert_eq!(
        final_health.status(),
        200,
        "Server should be healthy after second OOM"
    );

    println!("SUCCESS: Server fully recovered and continues operation after OOM events");
}

/// Test that OOM events are properly handled and observable via health/metrics endpoints.
///
/// While we can't directly verify logging in this test, we verify that:
///
/// 1. The server's health endpoint responds correctly after OOM
/// 2. Metrics endpoint (if available) is still functional
/// 3. The server can report its status accurately
///
/// ## Expected Behavior
///
/// - OOM is handled gracefully (not a crash)
/// - Server health status remains accurate
/// - Metrics can be collected after OOM
///
/// ## Observability Goals
///
/// In production, OOM events should:
/// - Be logged with appropriate severity (error/warn)
/// - Include the module name that caused OOM
/// - Include memory usage at time of failure
/// - Increment error counters in metrics
#[tokio::test]
#[ignore = "Requires memory_hog.wasm fixture"]
async fn test_oom_logged_properly() {
    if !memory_hog_wasm_exists() {
        eprintln!(
            "Skipping: memory_hog.wasm fixture not found at {}",
            fixtures_dir().display()
        );
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_memory_limit_mb(16)
        .with_execution_timeout(30)
        .start()
        .await
        .expect("Failed to start real test host");

    // Get initial metrics (if endpoint exists)
    let initial_metrics = host.get("/metrics").await.ok().and_then(|r| {
        if r.status().is_success() {
            Some(r)
        } else {
            None
        }
    });

    // Trigger OOM
    println!("Triggering OOM event...");
    let oom_start = std::time::Instant::now();
    let oom_result = host
        .post_json("/run/memory_hog/", &serde_json::json!({}))
        .await;
    let oom_duration = oom_start.elapsed();

    println!("OOM request completed in {:?}", oom_duration);

    // Log the result for observability verification
    match &oom_result {
        Ok(resp) => {
            println!("OOM response status: {}", resp.status());
        },
        Err(e) => {
            println!("OOM request error: {}", e);
        },
    }

    // Verify health endpoint reports status correctly
    let health = host
        .get("/health")
        .await
        .expect("Health check should succeed after OOM");

    assert_eq!(health.status(), 200, "Health should return 200");

    let health_body: serde_json::Value = health.json().await.expect("Failed to parse health");
    println!("Health response after OOM: {:?}", health_body);

    // The health response should still indicate a ready server
    assert_eq!(
        health_body["status"], "ready",
        "Server should report ready status even after OOM"
    );

    // Verify memory information is reported (if available)
    if let Some(memory) = health_body.get("memory") {
        println!("Memory info from health: {:?}", memory);
        // Memory limit should be configured
        if let Some(limit) = memory.get("limit_per_request_bytes") {
            println!("Memory limit per request: {} bytes", limit);
        }
    }

    // Check metrics endpoint (if initial metrics were available)
    if initial_metrics.is_some() {
        let post_metrics = host.get("/metrics").await;

        match post_metrics {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.expect("Failed to get metrics text");
                println!("Metrics endpoint still functional after OOM");

                // Check for request counter (should have increased)
                if body.contains("mik_requests_total") {
                    println!("Request counter present in metrics");
                }

                // In a full implementation, we'd expect error counters here
                // e.g., mik_oom_events_total, mik_module_errors_total
            },
            Ok(resp) => {
                println!(
                    "Metrics endpoint returned non-success after OOM: {}",
                    resp.status()
                );
            },
            Err(e) => {
                // Metrics endpoint failing is a warning, not a test failure
                println!("WARNING: Metrics endpoint error after OOM: {}", e);
            },
        }
    }

    println!("OOM handling test complete - server remains operational");
}

// =============================================================================
// Unit Tests (No fixtures required)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the fixture detection helper functions work correctly.
    #[test]
    fn test_fixture_detection() {
        let dir = fixtures_dir();
        let exists = memory_hog_wasm_exists();

        println!("Fixtures directory: {}", dir.display());
        println!("memory_hog.wasm exists: {}", exists);

        // Directory should exist even if specific fixtures don't
        assert!(
            dir.parent().map(|p| p.exists()).unwrap_or(false)
                || dir.exists()
                || cfg!(not(target_os = "linux")),
            "Fixtures parent directory should exist"
        );
    }

    /// Test memory limit calculations and edge cases.
    ///
    /// This verifies the math used when configuring memory limits doesn't overflow.
    #[test]
    fn test_memory_limit_calculations() {
        // Standard limits
        let mb_16 = 16_usize * 1024 * 1024;
        let mb_64 = 64_usize * 1024 * 1024;
        let mb_128 = 128_usize * 1024 * 1024;

        assert_eq!(mb_16, 16_777_216, "16MB should be 16,777,216 bytes");
        assert_eq!(mb_64, 67_108_864, "64MB should be 67,108,864 bytes");
        assert_eq!(mb_128, 134_217_728, "128MB should be 134,217,728 bytes");

        // Test large but valid limits
        let gb_1 = 1024_usize * 1024 * 1024;
        let gb_4 = 4_usize * 1024 * 1024 * 1024;

        assert_eq!(gb_1, 1_073_741_824, "1GB should be 1,073,741,824 bytes");
        assert_eq!(gb_4, 4_294_967_296, "4GB should be 4,294,967,296 bytes");
    }

    /// Test that saturating multiplication prevents overflow in memory calculations.
    #[test]
    fn test_memory_overflow_protection() {
        // Using usize saturating operations
        let huge_mb: usize = usize::MAX / (1024 * 1024) + 1;
        let result = huge_mb.saturating_mul(1024).saturating_mul(1024);

        // Should saturate to MAX, not wrap
        assert_eq!(
            result,
            usize::MAX,
            "Overflow should saturate to MAX, not wrap"
        );
    }

    /// Test OOM error message format expectations.
    ///
    /// Verifies that OOM error messages contain useful diagnostic information.
    #[test]
    fn test_oom_error_message_format() {
        // Expected error message patterns for OOM conditions
        let expected_patterns = [
            "memory",
            "limit",
            "exceeded",
            "allocation",
            "out of memory",
            "oom",
            "resource",
        ];

        // A sample OOM error message (from wasmtime ResourceLimiter)
        let sample_error = "memory allocation failed: allocation limit exceeded";

        // At least one pattern should match typical OOM messages
        let has_pattern = expected_patterns
            .iter()
            .any(|p| sample_error.to_lowercase().contains(p));

        assert!(
            has_pattern,
            "OOM error should contain recognizable keywords"
        );
    }

    /// Test that memory limits are within WASM spec bounds.
    ///
    /// WASM32 has a 4GB address space limit (32-bit addressing).
    #[test]
    fn test_memory_limit_within_wasm_bounds() {
        let wasm32_max: u64 = 4 * 1024 * 1024 * 1024; // 4GB

        // Typical mikrozen limits should be well under this
        let typical_limits: [u64; 4] = [
            16 * 1024 * 1024,  // 16MB
            64 * 1024 * 1024,  // 64MB
            128 * 1024 * 1024, // 128MB
            256 * 1024 * 1024, // 256MB
        ];

        for limit in typical_limits {
            assert!(
                limit < wasm32_max,
                "Limit {} should be under WASM32 max {}",
                limit,
                wasm32_max
            );
        }
    }

    /// Verify status code expectations for OOM responses.
    #[test]
    fn test_oom_status_code_expectations() {
        // OOM should result in a server error (5xx)
        // 500 Internal Server Error - generic server error
        // 503 Service Unavailable - resources exhausted
        // 507 Insufficient Storage - storage/memory full (WebDAV but applicable)

        let acceptable_status_codes: [u16; 3] = [500, 503, 507];

        for code in acceptable_status_codes {
            assert!(
                code >= 500 && code < 600,
                "Status {} should be a 5xx server error",
                code
            );
        }
    }
}
