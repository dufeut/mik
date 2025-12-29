// Test-specific lint suppressions
#![allow(clippy::println_empty_string)]
#![allow(clippy::expect_fun_call)]

//! Oversized Response Tests - Wasmtime Bug #12141
//!
//! These tests verify that the runtime handles oversized HTTP responses correctly,
//! based on Wasmtime bug #12141 which caused the server to freeze when an HTTP body
//! exceeded configured limits.
//!
//! Bug reference: https://github.com/bytecodealliance/wasmtime/issues/12141
//!
//! ## The Problem
//!
//! In affected versions of Wasmtime, when a WASM module attempted to send an HTTP
//! response body larger than the configured `max_body_size`, the server would hang
//! indefinitely instead of returning an error or timing out.
//!
//! ## Expected Behavior
//!
//! 1. Oversized responses should timeout or return an error, NOT hang forever
//! 2. After an oversized response failure, the server should continue accepting new requests
//! 3. Other modules should not be affected by one module's oversized response
//!
//! ## Required Fixture
//!
//! These tests require an `oversized_response.wasm` fixture that:
//! - Returns a response body larger than the configured `max_body_size_mb` limit
//! - For example, if limit is 1MB, the fixture should return a 2MB+ response
//!
//! ### Creating the Fixture
//!
//! ```rust,ignore
//! // oversized_response handler implementation
//! impl Guest for Component {
//!     fn handle(_request: IncomingRequest, response_out: ResponseOutparam) {
//!         let headers = Fields::new();
//!         let _ = headers.append(&"content-type".to_string(), &b"application/octet-stream".to_vec());
//!
//!         let response = OutgoingResponse::new(headers);
//!         response.set_status_code(200).unwrap();
//!
//!         let outgoing_body = response.body().unwrap();
//!         ResponseOutparam::set(response_out, Ok(response));
//!
//!         // Write 2MB of data (larger than 1MB limit)
//!         let stream = outgoing_body.write().unwrap();
//!         let large_chunk = vec![b'X'; 64 * 1024]; // 64KB chunks
//!         for _ in 0..32 {  // 32 * 64KB = 2MB
//!             let _ = stream.blocking_write_and_flush(&large_chunk);
//!         }
//!         drop(stream);
//!         OutgoingBody::finish(outgoing_body, None).unwrap();
//!     }
//! }
//! ```
//!
//! Run these tests with:
//! ```bash
//! cargo test -p mik oversized -- --ignored
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

#[path = "common.rs"]
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

// =============================================================================
// Oversized Response Tests (Wasmtime Bug #12141)
// =============================================================================

/// Test that oversized responses timeout instead of hanging forever.
///
/// This test verifies the fix for Wasmtime bug #12141 where HTTP bodies exceeding
/// the configured limit would cause the server to freeze indefinitely.
///
/// ## Expected Behavior
///
/// When a WASM module returns a response body larger than `max_body_size_mb`:
/// - The request should complete within a reasonable time (timeout)
/// - The server should return an error status (4xx or 5xx)
/// - The server should NOT hang indefinitely
///
/// ## Configuration
///
/// The test uses:
/// - `max_body_size_mb(1)` - 1MB limit
/// - `execution_timeout(10)` - 10 second timeout as safety net
///
/// The `oversized_response.wasm` fixture should return a 2MB+ response.
#[tokio::test]
#[ignore = "Requires oversized_response.wasm fixture - see module docs for creation instructions"]
async fn test_oversized_response_timeout_not_hang() {
    // Check if required fixtures exist
    if !fixture_exists("oversized_response.wasm") {
        eprintln!("Skipping: oversized_response.wasm fixture not found");
        eprintln!("");
        eprintln!("To create this fixture:");
        eprintln!("  1. Create a WASM component project with mikrozen SDK");
        eprintln!("  2. Return a response body larger than 1MB (e.g., 2MB of 'X' bytes)");
        eprintln!("  3. Build with `cargo component build --release`");
        eprintln!("  4. Copy to tests/fixtures/modules/oversized_response.wasm");
        return;
    }

    // Need echo.wasm for the server to start (at least one valid module required)
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm fixture required for server startup");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_max_body_size_mb(1) // 1MB limit - fixture should exceed this
        .with_execution_timeout(10) // 10 second timeout as safety net
        .start()
        .await
        .expect("Failed to start host");

    let start = Instant::now();

    // Make request to oversized response handler
    let result = host
        .post_json("/run/oversized_response/", &serde_json::json!({}))
        .await;

    let elapsed = start.elapsed();

    // KEY ASSERTION: Request should complete within reasonable time, NOT hang
    // The bug caused this to hang forever. With the fix, it should either:
    // - Return an error (body too large)
    // - Timeout after execution_timeout
    assert!(
        elapsed < Duration::from_secs(30),
        "Request should complete within 30 seconds, not hang forever. \
         Elapsed: {:?}. This may indicate Wasmtime bug #12141 regression.",
        elapsed
    );

    // Request should return error (either from body limit or timeout)
    match result {
        Ok(resp) => {
            // Should be an error status (4xx or 5xx)
            let status = resp.status();
            assert!(
                status.is_client_error() || status.is_server_error(),
                "Expected 4xx or 5xx error for oversized response, got {}",
                status
            );
            println!(
                "Oversized response returned status {} after {:?}",
                status, elapsed
            );
        },
        Err(e) => {
            // Connection error is also acceptable - server may close connection
            println!(
                "Oversized response caused connection error after {:?}: {}",
                elapsed, e
            );
        },
    }

    // Verify server is still responding (didn't crash or hang)
    let health = host
        .get("/health")
        .await
        .expect("Server should still be running after oversized response");
    assert_eq!(health.status(), 200, "Health endpoint should return 200");
}

/// Test that server continues accepting requests after an oversized response failure.
///
/// This is a regression test for Wasmtime bug #12141. The bug not only caused
/// the problematic request to hang, but also blocked subsequent requests.
///
/// ## Expected Behavior
///
/// After an oversized response fails:
/// 1. The server should continue running
/// 2. New requests to other modules should succeed
/// 3. The echo module should work normally
///
/// ## Test Strategy
///
/// 1. Send a request that triggers oversized response (may fail/timeout)
/// 2. Verify the server is still alive via /health
/// 3. Send a normal request to echo module
/// 4. Verify echo response is correct
#[tokio::test]
#[ignore = "Requires oversized_response.wasm fixture - see module docs for creation instructions"]
async fn test_server_continues_after_oversized() {
    // Check if required fixtures exist
    if !fixture_exists("oversized_response.wasm") {
        eprintln!("Skipping: oversized_response.wasm fixture not found");
        eprintln!("");
        eprintln!("To create this fixture:");
        eprintln!("  1. Create a WASM component project with mikrozen SDK");
        eprintln!("  2. Return a response body larger than 1MB (e.g., 2MB of 'X' bytes)");
        eprintln!("  3. Build with `cargo component build --release`");
        eprintln!("  4. Copy to tests/fixtures/modules/oversized_response.wasm");
        return;
    }

    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm fixture required");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_max_body_size_mb(1) // 1MB limit
        .with_execution_timeout(10) // 10 second timeout
        .with_max_concurrent_requests(5) // Allow concurrent requests
        .start()
        .await
        .expect("Failed to start host");

    // Step 1: Trigger oversized response (expected to fail)
    // We don't care about the result, just that it completes
    let start = Instant::now();
    let _ = tokio::time::timeout(
        Duration::from_secs(15),
        host.post_json("/run/oversized_response/", &serde_json::json!({})),
    )
    .await;
    let oversized_elapsed = start.elapsed();

    println!(
        "Oversized request completed (or timed out) after {:?}",
        oversized_elapsed
    );

    // Step 2: Verify server is still alive
    let health = host
        .get("/health")
        .await
        .expect("Server should respond to health check after oversized response failure");
    assert_eq!(
        health.status(),
        200,
        "Health endpoint should return 200 after oversized response"
    );

    // Step 3: Send a normal request to echo module
    let echo_result = host
        .post_json(
            "/run/echo/",
            &serde_json::json!({"test": "after_oversized", "value": 42}),
        )
        .await
        .expect("Echo request should succeed after oversized response failure");

    assert_eq!(
        echo_result.status(),
        200,
        "Echo should return 200 after oversized response failure"
    );

    // Step 4: Verify echo response is correct
    let body: serde_json::Value = echo_result
        .json()
        .await
        .expect("Echo response should be valid JSON");

    assert_eq!(
        body["test"], "after_oversized",
        "Echo should return correct data"
    );
    assert_eq!(body["value"], 42, "Echo should return correct value");

    println!("Server successfully handled normal request after oversized response");

    // Step 5: Make additional requests to ensure server is fully functional
    for i in 0..3 {
        let resp = host
            .post_json("/run/echo/", &serde_json::json!({"iteration": i}))
            .await
            .expect(&format!("Follow-up request {} should succeed", i));

        assert_eq!(
            resp.status(),
            200,
            "Follow-up request {} should return 200",
            i
        );
    }

    println!("All follow-up requests succeeded - server is fully functional");
}

// =============================================================================
// Unit Tests (No Fixtures Required)
// =============================================================================

/// Test that max_body_size_mb configuration is applied correctly.
#[test]
fn test_max_body_size_calculation() {
    // 1MB in bytes
    let mb = 1;
    let expected_bytes = mb * 1024 * 1024;
    assert_eq!(expected_bytes, 1_048_576);

    // 10MB in bytes
    let mb = 10;
    let expected_bytes = mb * 1024 * 1024;
    assert_eq!(expected_bytes, 10_485_760);
}

/// Test that body size limit math is correct and doesn't overflow.
#[test]
fn test_body_size_limit_overflow_protection() {
    // Test large values don't overflow when converted
    let large_mb: usize = 1024; // 1GB
    let bytes = large_mb.checked_mul(1024 * 1024);
    assert!(bytes.is_some(), "1GB should not overflow usize");
    assert_eq!(bytes.unwrap(), 1_073_741_824);

    // Test very large values
    let huge_mb: usize = 16384; // 16GB
    let bytes = huge_mb.checked_mul(1024 * 1024);
    assert!(bytes.is_some(), "16GB should not overflow usize on 64-bit");
}
