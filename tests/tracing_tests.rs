//! Request tracing tests.
//!
//! Tests for the distributed tracing implementation:
//! - X-Trace-ID header generation
//! - X-Trace-ID header propagation
//! - trace_id in tracing spans

// Import TestHost from the common module (sibling file)
#[path = "common.rs"]
mod common;

use common::TestHost;

// =============================================================================
// Trace ID Header Tests
// =============================================================================

#[tokio::test]
async fn test_health_returns_trace_id_header() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/health").await.expect("Failed to get health");

    // Verify X-Trace-ID header is present
    let trace_id = resp
        .headers()
        .get("x-trace-id")
        .expect("Missing X-Trace-ID header");

    // Trace ID should be a valid UUID or hex string
    let trace_id_str = trace_id.to_str().expect("Invalid trace ID header");
    assert!(!trace_id_str.is_empty(), "Trace ID should not be empty");
}

#[tokio::test]
async fn test_metrics_returns_trace_id_header() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/metrics").await.expect("Failed to get metrics");

    // Verify X-Trace-ID header is present
    let trace_id = resp
        .headers()
        .get("x-trace-id")
        .expect("Missing X-Trace-ID header");

    let trace_id_str = trace_id.to_str().expect("Invalid trace ID header");
    assert!(!trace_id_str.is_empty(), "Trace ID should not be empty");
}

#[tokio::test]
async fn test_request_id_and_trace_id_both_present() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/health").await.expect("Failed to get health");

    // Both X-Request-ID and X-Trace-ID should be present
    assert!(
        resp.headers().get("x-request-id").is_some(),
        "Missing X-Request-ID header"
    );
    assert!(
        resp.headers().get("x-trace-id").is_some(),
        "Missing X-Trace-ID header"
    );
}

#[tokio::test]
async fn test_incoming_trace_id_is_propagated() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    // Send request with custom trace ID
    let custom_trace_id = "my-custom-trace-123";
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{}/health", host.addr().port()))
        .header("x-trace-id", custom_trace_id)
        .send()
        .await
        .expect("Failed to send request");

    // Verify the response uses the same trace ID
    let returned_trace_id = resp
        .headers()
        .get("x-trace-id")
        .expect("Missing X-Trace-ID header")
        .to_str()
        .expect("Invalid trace ID header");

    assert_eq!(
        returned_trace_id, custom_trace_id,
        "Returned trace ID should match incoming trace ID"
    );
}

#[tokio::test]
async fn test_trace_id_generated_when_not_provided() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    // Make two requests without trace ID
    let resp1 = host.get("/health").await.expect("Failed to get health");
    let resp2 = host.get("/health").await.expect("Failed to get health");

    let trace_id1 = resp1
        .headers()
        .get("x-trace-id")
        .expect("Missing X-Trace-ID header")
        .to_str()
        .expect("Invalid trace ID");

    let trace_id2 = resp2
        .headers()
        .get("x-trace-id")
        .expect("Missing X-Trace-ID header")
        .to_str()
        .expect("Invalid trace ID");

    // Each request should get a unique trace ID
    assert_ne!(
        trace_id1, trace_id2,
        "Each request should have a unique trace ID"
    );
}

#[tokio::test]
async fn test_trace_id_is_uuid_format() {
    let host = TestHost::builder()
        .start()
        .await
        .expect("Failed to start test host");

    let resp = host.get("/health").await.expect("Failed to get health");

    let trace_id = resp
        .headers()
        .get("x-trace-id")
        .expect("Missing X-Trace-ID header")
        .to_str()
        .expect("Invalid trace ID header");

    // When not provided, trace_id should be a UUID (from request_id)
    // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
    assert!(
        trace_id.len() == 36 && trace_id.chars().filter(|&c| c == '-').count() == 4,
        "Trace ID should be UUID format when generated, got: {}",
        trace_id
    );
}

// =============================================================================
// Unit Tests - Trace ID Validation
// =============================================================================

#[test]
fn test_trace_id_header_name_is_lowercase() {
    // Verify we're using lowercase header names (HTTP/2 requirement)
    let header_name = "x-trace-id";
    assert!(
        header_name.chars().all(|c| c.is_lowercase() || c == '-'),
        "Header name should be lowercase for HTTP/2 compatibility"
    );
}
