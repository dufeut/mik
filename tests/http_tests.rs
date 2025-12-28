//! HTTP endpoint integration tests.
//!
//! Tests for the HTTP endpoints exposed by the mikrozen runtime:
//! - `/health` - Health check endpoint
//! - `/metrics` - Prometheus metrics endpoint
//! - `/static/*` - Static file serving
//! - `/run/*` - WASM module execution
//! - `/script/*` - JS script orchestration

// Import TestHost from the common module (sibling file)
#[path = "common.rs"]
mod common;

use common::TestHost;
use std::path::PathBuf;
use tempfile::TempDir;

/// Get the path to the test fixtures directory.
#[allow(dead_code)]
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

// =============================================================================
// Health Endpoint Tests
// =============================================================================

#[tokio::test]
async fn test_health_endpoint_returns_200() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/health").await.expect("Failed to get health");
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_health_endpoint_returns_json() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/health").await.expect("Failed to get health");

    let content_type = resp
        .headers()
        .get("content-type")
        .expect("No content-type header")
        .to_str()
        .expect("Invalid content-type");

    assert!(
        content_type.contains("application/json"),
        "Expected JSON content type, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_health_endpoint_body_structure() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/health").await.expect("Failed to get health");
    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");

    // Verify required fields
    assert!(body.get("status").is_some(), "Missing 'status' field");
    assert!(body.get("timestamp").is_some(), "Missing 'timestamp' field");
    assert!(
        body.get("cache_size").is_some(),
        "Missing 'cache_size' field"
    );

    // Verify status is "ready"
    assert_eq!(body["status"], "ready");
}

// =============================================================================
// Metrics Endpoint Tests
// =============================================================================

#[tokio::test]
async fn test_metrics_endpoint_returns_200() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/metrics").await.expect("Failed to get metrics");
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_metrics_endpoint_returns_prometheus_format() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/metrics").await.expect("Failed to get metrics");

    let content_type = resp
        .headers()
        .get("content-type")
        .expect("No content-type header")
        .to_str()
        .expect("Invalid content-type");

    assert!(
        content_type.contains("text/plain"),
        "Expected text/plain content type, got: {}",
        content_type
    );

    let body = resp.text().await.expect("Failed to get body");
    assert!(
        body.contains("mik_requests_total"),
        "Missing mik_requests_total metric"
    );
}

// =============================================================================
// Not Found Tests
// =============================================================================

#[tokio::test]
async fn test_nonexistent_path_returns_404() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host
        .get("/nonexistent/path")
        .await
        .expect("Failed to get response");

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_root_path_returns_404() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/").await.expect("Failed to get response");
    assert_eq!(resp.status(), 404);
}

// =============================================================================
// Static File Tests
// =============================================================================

#[tokio::test]
async fn test_static_without_config_returns_404() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host
        .get("/static/test.txt")
        .await
        .expect("Failed to get response");

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_static_file_serving() {
    // Create a temp directory with a test file
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "Hello, World!").expect("Failed to write test file");

    let host = TestHost::builder()
        .with_static_dir(temp_dir.path())
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host
        .get("/static/test.txt")
        .await
        .expect("Failed to get response");

    assert_eq!(resp.status(), 200);

    let body = resp.text().await.expect("Failed to get body");
    assert_eq!(body, "Hello, World!");
}

#[tokio::test]
async fn test_static_file_content_type() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let html_file = temp_dir.path().join("index.html");
    std::fs::write(&html_file, "<html></html>").expect("Failed to write HTML file");

    let host = TestHost::builder()
        .with_static_dir(temp_dir.path())
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host
        .get("/static/index.html")
        .await
        .expect("Failed to get response");

    assert_eq!(resp.status(), 200);

    let content_type = resp
        .headers()
        .get("content-type")
        .expect("No content-type header")
        .to_str()
        .expect("Invalid content-type");

    assert!(
        content_type.contains("text/html"),
        "Expected text/html, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_static_file_not_found() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let host = TestHost::builder()
        .with_static_dir(temp_dir.path())
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host
        .get("/static/nonexistent.txt")
        .await
        .expect("Failed to get response");

    assert_eq!(resp.status(), 404);
}

// =============================================================================
// Module Execution Tests (require actual WASM modules)
// =============================================================================

#[tokio::test]
#[ignore = "Requires WASM modules to be present"]
async fn test_run_module_returns_response() {
    // This test would require actual WASM modules
    // For now, it's ignored until modules are available
    let _fixtures = fixtures_dir();

    let host = TestHost::builder()
        .with_modules_dir(fixtures_dir().join("modules"))
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host
        .get("/run/echo/hello")
        .await
        .expect("Failed to get response");

    // Should not be 404 or 503 if module exists
    assert_ne!(resp.status(), 404);
    assert_ne!(resp.status(), 503);
}

#[tokio::test]
async fn test_run_without_modules_dir() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host
        .get("/run/somemodule/path")
        .await
        .expect("Failed to get response");

    // Should return an error status (503 or similar)
    assert!(resp.status().is_server_error() || resp.status() == 501);
}

// =============================================================================
// Concurrent Request Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_health_requests() {
    let host = TestHost::builder()
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

// =============================================================================
// Edge Case Tests
// =============================================================================

#[tokio::test]
async fn test_long_path() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    // Create a very long path (but not exceeding typical limits)
    let long_segment = "a".repeat(100);
    let long_path = format!("/{}/{}/{}", long_segment, long_segment, long_segment);

    let resp = host.get(&long_path).await.expect("Failed to get response");

    // Should return 404, not crash
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_special_characters_in_path() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    // Test with URL-encoded special characters
    let resp = host
        .get("/static/test%20file.txt")
        .await
        .expect("Failed to get response");

    // Should handle gracefully (404 is fine since file doesn't exist)
    assert!(resp.status() == 404 || resp.status().is_success());
}

#[tokio::test]
async fn test_path_traversal_blocked() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "secret").expect("Failed to write test file");

    let host = TestHost::builder()
        .with_static_dir(temp_dir.path())
        .start()
        .await
        .expect("Failed to start test host");

    // Attempt path traversal
    let resp = host
        .get("/static/../../../etc/passwd")
        .await
        .expect("Failed to get response");

    // Should not expose files outside static dir
    assert!(resp.status() == 400 || resp.status() == 404);
}
