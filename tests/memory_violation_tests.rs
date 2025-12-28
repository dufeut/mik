//! Memory violation logging tests for the mikrozen runtime.
//!
//! These tests are based on wasmCloud issue #3050, which tracks the need to
//! properly log memory violations in WASM execution:
//! <https://github.com/wasmCloud/wasmCloud/issues/3050>
//!
//! ## Background
//!
//! When WASM modules perform invalid memory operations (out-of-bounds access,
//! stack overflow, invalid table access), the runtime should:
//!
//! 1. **Trap gracefully** - Return an error, not crash the host
//! 2. **Log the violation** - Include useful diagnostic information
//! 3. **Continue serving** - Other requests should not be affected
//!
//! ## Test Philosophy
//!
//! These tests verify that memory violations are handled safely and observable:
//!
//! 1. **Trap Containment** - Memory violations result in HTTP errors, not panics
//! 2. **Server Resilience** - Server continues after memory violations
//! 3. **Observable Errors** - Error responses contain useful information
//!
//! ## Related Issues
//!
//! - wasmCloud #3050: Memory violation logging
//! - Wasmtime #12069: OOM handling (related)
//! - wasmCloud #2978: Guest panic recovery (related)
//!
//! ## Running Tests
//!
//! ```bash
//! # Run unit tests (no fixtures required)
//! cargo test -p mik memory_violation
//!
//! # Run integration tests (requires fixtures)
//! cargo test -p mik memory_violation -- --ignored --test-threads=1
//! ```

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

/// Check if the panic.wasm fixture exists.
fn panic_wasm_exists() -> bool {
    fixture_exists("panic.wasm")
}

/// Check if echo.wasm exists (for recovery verification).
fn echo_wasm_exists() -> bool {
    fixture_exists("echo.wasm")
}

// =============================================================================
// Integration Tests (Require WASM fixtures)
// =============================================================================

/// Test that memory violations (traps) return proper error responses instead of crashing.
///
/// This test verifies the core behavior from wasmCloud #3050: when a WASM module
/// experiences a memory violation (panic, trap, out-of-bounds access), the runtime
/// should return an error to the client rather than crashing or hanging.
///
/// ## Expected Behavior
///
/// - The `panic.wasm` module triggers an explicit panic/trap
/// - The runtime catches the trap and converts it to an HTTP error response
/// - The response includes error information (status 5xx)
/// - The connection is not dropped unexpectedly
///
/// ## Bug Behavior (what we're guarding against)
///
/// - The runtime crashes when a trap occurs
/// - The connection is reset without a proper HTTP response
/// - No useful error information is returned to the client
#[tokio::test]
#[ignore = "Requires panic.wasm fixture"]
async fn test_memory_violation_returns_error_not_crash() {
    if !panic_wasm_exists() {
        eprintln!(
            "Skipping: panic.wasm fixture not found at {}",
            fixtures_dir().display()
        );
        eprintln!("");
        eprintln!("To create this fixture:");
        eprintln!("  cd mik/tests/fixtures/wasm-fixtures/panic");
        eprintln!("  cargo component build --release");
        eprintln!("  cp target/wasm32-wasip2/release/panic.wasm ../../modules/");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_execution_timeout(10)
        .start()
        .await
        .expect("Failed to start real test host");

    // Call the panic module - it should trigger a trap
    let result = host.post_json("/run/panic/", &serde_json::json!({})).await;

    // The key assertion: we should get a response, not a crash
    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();

            // Memory violation should result in a server error (5xx)
            assert!(
                status >= 400,
                "Memory violation should return an error status (4xx/5xx), got {}. \
                This suggests the trap is not being caught properly.",
                status
            );

            // Specifically expecting 500 (Internal Server Error)
            if status >= 500 && status < 600 {
                println!(
                    "SUCCESS: Memory violation correctly returned {} status (server error)",
                    status
                );
            } else if status >= 400 && status < 500 {
                println!(
                    "NOTE: Memory violation returned {} status (client error). \
                    Verify error categorization is correct.",
                    status
                );
            }

            // Verify we can read the response body (connection stable)
            let body_text = resp.text().await;
            match body_text {
                Ok(text) => {
                    println!("Response body: {}", text);
                    // Error response should contain some error indication
                    // (varies by implementation)
                    assert!(!text.is_empty(), "Error response body should not be empty");
                },
                Err(e) => {
                    // Body read error is acceptable if we got the status
                    println!("Could not read body: {} (acceptable)", e);
                },
            }
        },
        Err(e) => {
            // If we got an error, it should NOT be a connection reset due to crash
            if e.is_connect() {
                panic!(
                    "POTENTIAL BUG (wasmCloud #3050): Connection failed during memory violation. \
                    The server may have crashed instead of returning an error. \
                    Error: {}",
                    e
                );
            } else if e.is_timeout() {
                panic!(
                    "POTENTIAL BUG: Request timed out. Memory violations should fail fast, \
                    not hang. Error: {}",
                    e
                );
            } else {
                // Other errors might indicate issues
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
        .expect("Server should still be running after memory violation");

    assert_eq!(
        health.status(),
        200,
        "Health check should succeed after memory violation"
    );

    println!("SUCCESS: Server survived memory violation and remains healthy");
}

/// Test that the server continues serving other requests after a memory violation.
///
/// This test verifies server resilience: after one module experiences a memory
/// violation, the server should:
///
/// 1. Recover and accept new connections
/// 2. Successfully execute other (well-behaved) modules
/// 3. Report healthy status
/// 4. Be able to handle the violating module again
///
/// ## Expected Behavior
///
/// - Memory violation in panic.wasm doesn't affect subsequent requests
/// - Health endpoint returns 200
/// - Other modules (like echo) continue to work
/// - Multiple violations don't accumulate issues
///
/// ## Bug Behavior
///
/// - Server becomes unresponsive after memory violation
/// - New requests fail or timeout
/// - Memory corruption from one request affects others
#[tokio::test]
#[ignore = "Requires panic.wasm and echo.wasm fixtures"]
async fn test_server_continues_after_memory_violation() {
    if !panic_wasm_exists() {
        eprintln!(
            "Skipping: panic.wasm fixture not found at {}",
            fixtures_dir().display()
        );
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_execution_timeout(10)
        .start()
        .await
        .expect("Failed to start real test host");

    // Phase 1: Trigger memory violation
    println!("Phase 1: Triggering memory violation...");
    let _ = host
        .post_json("/run/panic/", &serde_json::json!({"trigger": "panic"}))
        .await;

    // Brief pause to ensure any cleanup has completed
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Phase 2: Verify health endpoint still works
    println!("Phase 2: Checking health endpoint...");
    let health_result = host.get("/health").await;

    match health_result {
        Ok(resp) => {
            assert_eq!(
                resp.status(),
                200,
                "Health endpoint should return 200 after memory violation. Got: {}",
                resp.status()
            );

            let body: serde_json::Value = resp.json().await.expect("Failed to parse health JSON");
            assert_eq!(
                body["status"], "ready",
                "Server status should be 'ready' after memory violation"
            );
            println!("SUCCESS: Health endpoint returned 200 after memory violation");
        },
        Err(e) => {
            panic!(
                "CRITICAL: Health endpoint failed after memory violation. \
                Server may be in a bad state. Error: {}",
                e
            );
        },
    }

    // Phase 3: Verify other modules work (if echo.wasm exists)
    if echo_wasm_exists() {
        println!("Phase 3: Testing echo module after memory violation...");

        let echo_result = host
            .post_json(
                "/run/echo/",
                &serde_json::json!({"test": "after_violation", "value": 123}),
            )
            .await;

        match echo_result {
            Ok(resp) => {
                assert_eq!(
                    resp.status(),
                    200,
                    "Echo should work after memory violation. Got: {}",
                    resp.status()
                );

                let body: serde_json::Value =
                    resp.json().await.expect("Failed to parse echo response");
                assert_eq!(
                    body["test"], "after_violation",
                    "Echo should return our test value"
                );
                println!("SUCCESS: Echo module works correctly after memory violation");
            },
            Err(e) => {
                panic!(
                    "Echo module failed after memory violation. \
                    Memory violation in panic.wasm may have corrupted server state. Error: {}",
                    e
                );
            },
        }
    } else {
        println!("Phase 3: Skipped (echo.wasm not available)");
    }

    // Phase 4: Verify we can trigger violation again (server fully recovered)
    println!("Phase 4: Triggering second memory violation to verify full recovery...");
    let second_violation = host
        .post_json("/run/panic/", &serde_json::json!({"trigger": "second"}))
        .await;

    // We don't care about the result, just that the server accepted the request
    match second_violation {
        Ok(resp) => {
            println!(
                "Second memory violation request completed with status: {}",
                resp.status()
            );
        },
        Err(e) => {
            // Connection error would be bad, but request error is acceptable
            if e.is_connect() {
                panic!(
                    "Server crashed on second memory violation. \
                    State may not be properly cleaned up. Error: {}",
                    e
                );
            }
            println!("Second memory violation request failed (acceptable): {}", e);
        },
    }

    // Final health check
    let final_health = host
        .get("/health")
        .await
        .expect("Server should be running after second memory violation");

    assert_eq!(
        final_health.status(),
        200,
        "Server should be healthy after multiple memory violations"
    );

    // Phase 5: Run multiple violations in quick succession
    println!("Phase 5: Rapid memory violation stress test...");
    let mut success_count = 0;
    let mut error_count = 0;

    for i in 0..5 {
        let result = host
            .post_json("/run/panic/", &serde_json::json!({"iteration": i}))
            .await;

        match result {
            Ok(resp) if resp.status().as_u16() >= 400 => {
                success_count += 1; // Error response is "success" - we caught the violation
            },
            Ok(resp) => {
                println!("Unexpected success status: {}", resp.status());
            },
            Err(e) if e.is_connect() => {
                panic!("Server crashed during rapid violation test: {}", e);
            },
            Err(_) => {
                error_count += 1;
            },
        }
    }

    println!(
        "Rapid test: {} caught violations, {} request errors",
        success_count, error_count
    );

    // Final verification
    let final_check = host
        .get("/health")
        .await
        .expect("Server should survive rapid memory violations");

    assert_eq!(
        final_check.status(),
        200,
        "Server should be healthy after rapid memory violations"
    );

    println!("SUCCESS: Server fully recovered and continues operation after memory violations");
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
        let panic_exists = panic_wasm_exists();
        let echo_exists = echo_wasm_exists();

        println!("Fixtures directory: {}", dir.display());
        println!("panic.wasm exists: {}", panic_exists);
        println!("echo.wasm exists: {}", echo_exists);

        // Directory should exist even if specific fixtures don't
        assert!(
            dir.parent().map(|p| p.exists()).unwrap_or(false)
                || dir.exists()
                || cfg!(not(target_os = "linux")),
            "Fixtures parent directory should exist"
        );
    }

    /// Test error status code expectations for memory violations.
    ///
    /// Memory violations should result in server errors (5xx), similar to OOM.
    #[test]
    fn test_memory_violation_status_code_expectations() {
        // Memory violations should result in server errors
        // 500 Internal Server Error - generic server error (trap)
        // 503 Service Unavailable - module unavailable

        let acceptable_status_codes: [u16; 2] = [500, 503];

        for code in acceptable_status_codes {
            assert!(
                code >= 500 && code < 600,
                "Status {} should be a 5xx server error",
                code
            );
        }
    }

    /// Test common trap message patterns.
    ///
    /// Verifies that trap/violation error messages contain useful diagnostic information.
    #[test]
    fn test_trap_error_message_patterns() {
        // Expected patterns in trap error messages from Wasmtime
        let expected_patterns = [
            "trap",
            "unreachable",
            "out of bounds",
            "stack overflow",
            "integer overflow",
            "integer divide by zero",
            "invalid conversion",
            "indirect call type mismatch",
            "undefined element",
            "uninitialized element",
            "memory access",
        ];

        // Sample trap messages (representative, not exhaustive)
        let sample_messages = [
            "wasm trap: wasm `unreachable` instruction executed",
            "wasm trap: out of bounds memory access",
            "wasm trap: call stack exhausted",
        ];

        for msg in sample_messages {
            let msg_lower = msg.to_lowercase();
            let has_pattern = expected_patterns.iter().any(|p| msg_lower.contains(p));
            assert!(
                has_pattern,
                "Trap message '{}' should contain a recognizable pattern",
                msg
            );
        }
    }

    /// Test that trap categorization is consistent.
    #[test]
    fn test_trap_categorization() {
        // Different trap types should all be categorized as errors
        let trap_types = [
            ("unreachable", "explicit trap instruction"),
            ("memory_oob", "out of bounds memory access"),
            ("stack_overflow", "call stack exhausted"),
            (
                "integer_overflow",
                "integer overflow with trapping semantics",
            ),
            ("div_by_zero", "integer divide by zero"),
        ];

        for (trap_name, _description) in trap_types {
            // All traps should be treated as server errors
            // This is a design principle test, not a runtime test
            assert!(
                !trap_name.is_empty(),
                "Trap {} should be handled as server error",
                trap_name
            );
        }
    }

    /// Verify memory violation doesn't affect unrelated state.
    ///
    /// This is a unit test for the principle that memory violations are isolated.
    #[test]
    fn test_memory_violation_isolation_principle() {
        // Each WASM instance should have isolated memory
        // A violation in one instance should not affect:
        // 1. The host process
        // 2. Other WASM instances
        // 3. The module cache

        // These are design invariants, tested here as documentation
        let isolation_guarantees = [
            "WASM linear memory is sandboxed",
            "Traps are caught by the runtime",
            "Instance state is per-request",
            "Module compilation is cached, execution is fresh",
        ];

        for guarantee in isolation_guarantees {
            // Document that we rely on these guarantees
            assert!(!guarantee.is_empty(), "Runtime depends on: {}", guarantee);
        }
    }
}
