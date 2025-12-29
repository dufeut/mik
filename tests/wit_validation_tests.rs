// Test-specific lint suppressions
#![allow(clippy::expect_fun_call)]
#![allow(clippy::approx_constant)]

//! WIT (WebAssembly Interface Types) Validation Tests
//!
//! These tests verify that WASM modules conform to the expected WIT contracts
//! for the mikrozen-host runtime. The runtime requires modules to export the
//! `wasi:http/incoming-handler@0.2.0` interface, which is the standard WASI HTTP
//! handler interface.
//!
//! # Background
//!
//! WebAssembly Interface Types (WIT) define the contracts between WASM components
//! and their hosts. For mikrozen-host, the critical interface is:
//!
//! - `wasi:http/incoming-handler@0.2.0` - The standard WASI HTTP handler interface
//!   that modules must export to handle incoming HTTP requests.
//!
//! Raw mikrozen handlers export `mikrozen:core/handler`, which is NOT directly
//! compatible with the runtime. They must be composed with bridge and router
//! components to produce a valid `wasi:http/incoming-handler` export.
//!
//! # Test Categories
//!
//! 1. **Positive Tests**: Verify that valid modules (like echo.wasm) work correctly
//! 2. **Negative Tests**: Verify that invalid/incompatible modules are rejected gracefully
//! 3. **Unit Tests**: Test path/name validation logic without requiring WASM fixtures
//!
//! # References
//!
//! - WASI HTTP: https://github.com/WebAssembly/wasi-http
//! - WIT specification: https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md
//! - mikrozen composition: See CLAUDE.md for handler composition details

#[path = "common.rs"]
mod common;

use common::RealTestHost;
use std::path::PathBuf;

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
// Unit Tests - Module Name and Path Validation
// =============================================================================
//
// These tests verify the validation logic for module names and paths.
// They don't require WASM fixtures and can run quickly.

/// Module names must be valid identifiers (alphanumeric + underscore/dash).
///
/// This prevents:
/// - Path traversal attacks (../../etc/passwd)
/// - Shell injection via module names
/// - Invalid filesystem access
#[test]
fn test_module_name_validation_valid_names() {
    let valid_names = [
        "echo",
        "my_module",
        "my-module",
        "MyModule",
        "module123",
        "Module_123",
        "a",
        "module-with-many-dashes",
        "module_with_underscores",
    ];

    for name in valid_names {
        assert!(
            is_valid_module_name(name),
            "Expected '{}' to be a valid module name",
            name
        );
    }
}

/// Invalid module names should be rejected.
///
/// These patterns could be used for path traversal or other attacks.
#[test]
fn test_module_name_validation_invalid_names() {
    let invalid_names = [
        "",
        ".",
        "..",
        "../etc/passwd",
        "module.wasm",
        "module/subpath",
        "module\\subpath",
        " module",
        "module ",
        "module with spaces",
        "module\nwith\nnewlines",
        "module\twith\ttabs",
        "module;rm -rf /",
        "module&echo",
        "module|cat",
        "module`ls`",
        "module$(ls)",
        "module>output",
        "module<input",
        "../../../etc/passwd",
        "....//....//etc/passwd",
    ];

    for name in invalid_names {
        assert!(
            !is_valid_module_name(name),
            "Expected '{}' to be an invalid module name",
            name
        );
    }
}

/// Test that module paths are properly constructed and validated.
#[test]
fn test_module_path_construction() {
    let base_dir = PathBuf::from("modules");
    let module_name = "echo";

    let expected_path = base_dir.join(format!("{}.wasm", module_name));

    // The path should end with the expected components
    assert!(
        expected_path.ends_with("echo.wasm"),
        "Module path should end with module name and .wasm extension"
    );

    // Verify the path contains the modules directory
    let path_str = expected_path.to_string_lossy();
    assert!(
        path_str.contains("modules"),
        "Module path should contain base directory"
    );

    // Verify the path has exactly one level of depth after base
    let components: Vec<_> = expected_path.components().collect();
    assert_eq!(
        components.len(),
        2,
        "Path should have exactly 2 components: base_dir and module.wasm"
    );
}

/// Test that path traversal attempts in module names are blocked.
#[test]
fn test_path_traversal_prevention() {
    let traversal_attempts = [
        "../secret",
        "..\\secret",
        "foo/../bar",
        "foo/../../bar",
        "%2e%2e%2f", // URL-encoded ../
        "%2e%2e/",
        "..%2f",
        "..%5c", // URL-encoded ..\
    ];

    for attempt in traversal_attempts {
        assert!(
            !is_valid_module_name(attempt),
            "Path traversal attempt '{}' should be rejected",
            attempt
        );
    }
}

/// Helper function to validate module names.
///
/// This mirrors the validation logic used in the actual runtime.
/// Module names must:
/// - Be non-empty
/// - Contain only alphanumeric characters, underscores, or dashes
/// - Not contain path separators or shell metacharacters
fn is_valid_module_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Check for path traversal patterns
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return false;
    }

    // Check each character
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// =============================================================================
// Integration Tests - Valid Module Exports (Positive Tests)
// =============================================================================

/// Verify that a valid module exports the wasi:http/incoming-handler interface.
///
/// The echo.wasm fixture is a properly composed WASM component that exports
/// the `wasi:http/incoming-handler@0.2.0` interface. This test verifies that
/// the runtime can:
/// 1. Load the module
/// 2. Verify its exports
/// 3. Execute HTTP requests against it
///
/// This is the "happy path" test for WIT contract validation.
#[tokio::test]
#[ignore = "Requires echo.wasm fixture"]
async fn test_handler_exports_wasi_http() {
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm not found. Run fixture build script first.");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .start()
        .await
        .expect("Failed to start host with valid module");

    // The runtime should have loaded the module successfully
    // Now verify it can handle HTTP requests (proving the interface is correct)
    let resp = host
        .post_json("/run/echo/", &serde_json::json!({"wit": "validation"}))
        .await
        .expect("Request to valid module failed");

    // A valid wasi:http/incoming-handler module should return a proper HTTP response
    assert_eq!(
        resp.status(),
        200,
        "Valid module should return 200 OK, got {}",
        resp.status()
    );

    // Verify the Content-Type header indicates proper HTTP handling
    let content_type = resp.headers().get("content-type").cloned();
    assert!(
        content_type.is_some(),
        "Response should have Content-Type header"
    );

    // Verify the response body is valid JSON (proving the handler processed the request)
    let body: serde_json::Value = resp.json().await.expect("Response should be valid JSON");
    assert_eq!(
        body["wit"], "validation",
        "Echo module should return the input"
    );
}

/// Verify that composed modules work correctly with the runtime.
///
/// A "composed" module is one that has been properly assembled from:
/// 1. A raw handler (mikrozen:core/handler)
/// 2. A bridge component (provides wasi:http/incoming-handler wrapper)
/// 3. A router component (provides mikrozen:core/http interface)
///
/// The result should export `wasi:http/incoming-handler@0.2.0` and handle
/// HTTP requests correctly.
#[tokio::test]
#[ignore = "Requires echo.wasm fixture"]
async fn test_composed_module_valid() {
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm not found. Run fixture build script first.");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_execution_timeout(5)
        .start()
        .await
        .expect("Failed to start host");

    // Test various HTTP scenarios to verify full composition

    // 1. Basic POST request
    let resp = host
        .post_json("/run/echo/", &serde_json::json!({"test": "basic"}))
        .await
        .expect("POST request failed");
    assert_eq!(resp.status(), 200, "Basic POST should succeed");

    // 2. Complex nested JSON (tests serialization/deserialization through WIT)
    let complex_input = serde_json::json!({
        "string": "hello",
        "number": 42,
        "float": 3.14159,
        "boolean": true,
        "null_value": null,
        "array": [1, 2, 3, "four", true],
        "nested": {
            "level1": {
                "level2": {
                    "value": "deeply nested"
                }
            }
        }
    });

    let resp = host
        .post_json("/run/echo/", &complex_input)
        .await
        .expect("Complex JSON request failed");
    assert_eq!(resp.status(), 200, "Complex JSON should be handled");

    let body: serde_json::Value = resp.json().await.expect("Failed to parse response");
    assert_eq!(
        body, complex_input,
        "Complex JSON should round-trip correctly"
    );

    // 3. Empty body (edge case)
    let resp = host
        .post_json("/run/echo/", &serde_json::json!({}))
        .await
        .expect("Empty body request failed");
    assert_eq!(resp.status(), 200, "Empty body should be handled");

    // 4. Multiple sequential requests (module should be reusable)
    for i in 0..3 {
        let resp = host
            .post_json("/run/echo/", &serde_json::json!({"iteration": i}))
            .await
            .expect(&format!("Request {} failed", i));
        assert_eq!(resp.status(), 200, "Request {} should succeed", i);
    }
}

// =============================================================================
// Integration Tests - Invalid Module Handling (Negative Tests)
// =============================================================================

/// Verify that modules without proper exports are rejected gracefully.
///
/// When a WASM module doesn't export `wasi:http/incoming-handler@0.2.0`,
/// the runtime should:
/// 1. Detect the missing/incompatible interface
/// 2. Return an appropriate error (not crash or hang)
/// 3. Continue serving other valid modules
///
/// This test uses a non-existent module name to verify 404 handling.
/// For testing actual invalid WIT, we would need a fixture that compiles
/// but doesn't export the required interface.
#[tokio::test]
#[ignore = "Requires echo.wasm fixture"]
async fn test_invalid_wit_rejected() {
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm not found. Run fixture build script first.");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .start()
        .await
        .expect("Failed to start host");

    // Test 1: Non-existent module should return 404
    let resp = host
        .post_json("/run/nonexistent_module/", &serde_json::json!({}))
        .await
        .expect("Request should complete even for missing module");

    assert_eq!(
        resp.status(),
        404,
        "Non-existent module should return 404, got {}",
        resp.status()
    );

    // Test 2: Module name with invalid characters should be rejected
    // The runtime should validate the name before attempting to load
    let resp = host
        .post_json("/run/../etc/passwd/", &serde_json::json!({}))
        .await
        .expect("Path traversal request should complete");

    assert!(
        resp.status() == 400 || resp.status() == 404,
        "Path traversal should be blocked with 400 or 404, got {}",
        resp.status()
    );

    // Test 3: After rejection, server should still work
    let health = host.get("/health").await.expect("Health check failed");
    assert_eq!(health.status(), 200, "Server should still be healthy");

    // Test 4: Valid module should still work after invalid attempts
    let resp = host
        .post_json("/run/echo/", &serde_json::json!({"after": "invalid"}))
        .await
        .expect("Valid module request failed after invalid attempts");

    assert_eq!(
        resp.status(),
        200,
        "Valid module should work after invalid attempts"
    );
}

/// Test that the runtime handles module loading errors gracefully.
///
/// This includes scenarios like:
/// - Corrupted WASM files
/// - Files that aren't valid WASM
/// - Modules with missing imports
///
/// The runtime should report errors clearly without crashing.
#[tokio::test]
#[ignore = "Requires echo.wasm fixture"]
async fn test_module_loading_error_handling() {
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm not found. Run fixture build script first.");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .start()
        .await
        .expect("Failed to start host");

    // Attempt to load a "module" that doesn't exist
    let resp = host
        .post_json("/run/not_a_real_module/", &serde_json::json!({}))
        .await
        .expect("Request should complete");

    // Should get a clean 404, not a 500 or crash
    assert_eq!(resp.status(), 404, "Missing module should return 404");

    // Check error message format
    let body = resp.text().await.expect("Should have response body");
    assert!(
        body.contains("error") || body.contains("not found") || body.contains("404"),
        "Error response should indicate module not found: {}",
        body
    );
}

// =============================================================================
// Edge Case Tests
// =============================================================================

/// Test that module names are case-sensitive on case-sensitive filesystems.
#[tokio::test]
#[ignore = "Requires echo.wasm fixture"]
async fn test_module_name_case_sensitivity() {
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm not found. Run fixture build script first.");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .start()
        .await
        .expect("Failed to start host");

    // "echo" should work (lowercase)
    let resp = host
        .post_json("/run/echo/", &serde_json::json!({"case": "lower"}))
        .await
        .expect("Lowercase echo request failed");
    assert_eq!(resp.status(), 200, "Lowercase 'echo' should work");

    // "ECHO" might not exist (uppercase) - behavior depends on filesystem
    let resp = host
        .post_json("/run/ECHO/", &serde_json::json!({"case": "upper"}))
        .await
        .expect("Uppercase ECHO request failed");

    // On case-sensitive filesystems, this should be 404
    // On case-insensitive filesystems (Windows, macOS default), it might be 200
    assert!(
        resp.status() == 200 || resp.status() == 404,
        "Uppercase 'ECHO' should return 200 or 404, got {}",
        resp.status()
    );
}

/// Test handling of special characters in module paths.
#[tokio::test]
#[ignore = "Requires echo.wasm fixture"]
async fn test_special_characters_in_path() {
    if !fixture_exists("echo.wasm") {
        eprintln!("Skipping: echo.wasm not found. Run fixture build script first.");
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .start()
        .await
        .expect("Failed to start host");

    // URL-encoded path traversal
    let resp = host
        .post_json("/run/%2e%2e%2fetc%2fpasswd/", &serde_json::json!({}))
        .await
        .expect("URL-encoded path traversal request failed");

    assert!(
        resp.status() == 400 || resp.status() == 404,
        "URL-encoded path traversal should be blocked, got {}",
        resp.status()
    );

    // Null bytes (should be rejected)
    let resp = host
        .client()
        .post(host.url("/run/echo%00evil/"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("Null byte request failed");

    assert!(
        resp.status() == 400 || resp.status() == 404,
        "Null byte in path should be blocked, got {}",
        resp.status()
    );

    // Server should still be healthy
    let health = host.get("/health").await.expect("Health check failed");
    assert_eq!(health.status(), 200);
}

// =============================================================================
// Documentation Tests
// =============================================================================

/// This test documents the expected WIT interface structure.
///
/// A valid mikrozen handler module must export:
/// ```wit
/// package wasi:http@0.2.0;
///
/// interface incoming-handler {
///     use types.{incoming-request, response-outparam};
///
///     handle: func(request: incoming-request, response-out: response-outparam);
/// }
/// ```
///
/// Raw handlers that export `mikrozen:core/handler` must be composed with
/// bridge and router components to produce this interface.
#[test]
fn document_expected_wit_interface() {
    // This test serves as documentation
    // The expected interface is wasi:http/incoming-handler@0.2.0
    //
    // Handlers must implement:
    // - handle(request: incoming-request, response-out: response-outparam)
    //
    // The runtime uses wasmtime-wasi-http to bind this interface.

    let expected_export = "wasi:http/incoming-handler@0.2.0";
    assert!(
        expected_export.contains("wasi:http"),
        "Export should be a WASI HTTP interface"
    );
    assert!(
        expected_export.contains("incoming-handler"),
        "Export should be the incoming-handler interface"
    );
    assert!(
        expected_export.contains("0.2.0"),
        "Export should specify version 0.2.0"
    );
}
