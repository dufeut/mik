//! Gateway API endpoints for mik runtime.
//!
//! Provides endpoints for the Go gateway to discover handlers and fetch
//! aggregated OpenAPI specs:
//!
//! - `GET /_mik/handlers` - List all available handlers
//! - `GET /_mik/openapi/platform` - Aggregated platform OpenAPI spec
//! - `GET /_mik/openapi/tenant/{tenant-id}` - Aggregated tenant OpenAPI spec

pub mod discovery;
pub mod openapi;
pub mod types;

use self::types::{
    ErrorResponse, HandlerAttributes, HandlerInfo, HandlerLinks, HandlersMetadata, HandlersResponse,
};
use crate::runtime::SharedState;
use anyhow::Result;
use chrono::Utc;
use http_body_util::Full;
use hyper::Response;
use hyper::body::Bytes;
use std::sync::Arc;
use tracing::debug;

/// Route prefix for gateway API endpoints.
pub const MIK_API_PREFIX: &str = "/_mik/";

/// Handle a gateway API request.
///
/// Routes requests to the appropriate handler based on the path:
/// - `/_mik/handlers` - List all handlers
/// - `/_mik/openapi/platform` - Platform OpenAPI spec
/// - `/_mik/openapi/tenant/{id}` - Tenant OpenAPI spec
///
/// # Arguments
///
/// * `shared` - Shared runtime state
/// * `path` - Request path (must start with `/_mik/`)
///
/// # Returns
///
/// HTTP response with JSON body.
pub fn handle_gateway_request(
    shared: &Arc<SharedState>,
    path: &str,
) -> Result<Response<Full<Bytes>>> {
    debug!("Gateway API request: {}", path);

    let Some(api_path) = path.strip_prefix(MIK_API_PREFIX) else {
        return json_error(404, &ErrorResponse::not_found("Invalid gateway API path"));
    };

    match api_path {
        "handlers" => handle_handlers(shared),
        "openapi/platform" => handle_platform_openapi(shared),
        p if p.starts_with("openapi/tenant/") => {
            let tenant_id = p.strip_prefix("openapi/tenant/").unwrap_or("");
            handle_tenant_openapi(shared, tenant_id)
        },
        _ => json_error(
            404,
            &ErrorResponse::not_found(format!("Unknown gateway endpoint: {api_path}")),
        ),
    }
}

/// Handle GET /_mik/handlers endpoint.
///
/// Returns a JSON:API-style list of all available handlers (platform + tenant).
fn handle_handlers(shared: &Arc<SharedState>) -> Result<Response<Full<Bytes>>> {
    let modules_dir = &shared.modules_dir;
    let user_modules_dir = shared.user_modules_dir.as_deref();

    let (platform_modules, tenant_modules) =
        discovery::discover_all_modules(modules_dir, user_modules_dir);

    let platform_count = platform_modules.len();
    let tenant_count = tenant_modules.len();

    // Convert to HandlerInfo
    let mut handlers: Vec<HandlerInfo> = Vec::new();

    for module in platform_modules {
        let name = &module.name;
        handlers.push(HandlerInfo {
            id: module.name.clone(),
            handler_type: "wasm".to_string(),
            attributes: HandlerAttributes {
                name: module.name.clone(),
                size_bytes: module.size_bytes,
                has_openapi: module.openapi_path.is_some(),
                tenant_id: None,
            },
            links: HandlerLinks {
                self_link: format!("/run/{name}/"),
                openapi: module
                    .openapi_path
                    .as_ref()
                    .map(|_| "/_mik/openapi/platform".to_string()),
            },
        });
    }

    for module in tenant_modules {
        let tid = module.tenant_id.clone().unwrap_or_default();
        let name = &module.name;
        handlers.push(HandlerInfo {
            id: format!("{tid}/{name}"),
            handler_type: "wasm".to_string(),
            attributes: HandlerAttributes {
                name: module.name.clone(),
                size_bytes: module.size_bytes,
                has_openapi: module.openapi_path.is_some(),
                tenant_id: Some(tid.clone()),
            },
            links: HandlerLinks {
                self_link: format!("/tenant/{tid}/{name}/"),
                openapi: module
                    .openapi_path
                    .as_ref()
                    .map(|_| format!("/_mik/openapi/tenant/{tid}")),
            },
        });
    }

    let response = HandlersResponse {
        data: handlers,
        meta: HandlersMetadata {
            total: platform_count + tenant_count,
            platform_count,
            tenant_count,
            timestamp: Utc::now().to_rfc3339(),
        },
    };

    json_response(200, &response)
}

/// Handle GET /_mik/openapi/platform endpoint.
///
/// Returns the aggregated OpenAPI spec for all platform handlers.
fn handle_platform_openapi(shared: &Arc<SharedState>) -> Result<Response<Full<Bytes>>> {
    let modules_dir = &shared.modules_dir;
    let spec = openapi::aggregate_platform_spec(modules_dir);

    json_response(200, &spec)
}

/// Handle GET /_mik/openapi/tenant/{tenant-id} endpoint.
///
/// Returns the aggregated OpenAPI spec for a specific tenant's handlers.
fn handle_tenant_openapi(
    shared: &Arc<SharedState>,
    tenant_id: &str,
) -> Result<Response<Full<Bytes>>> {
    if tenant_id.is_empty() {
        return json_error(
            400,
            &ErrorResponse {
                error: "invalid_request".to_string(),
                message: "Tenant ID is required".to_string(),
                request_id: None,
            },
        );
    }

    // Validate tenant_id format (should be UUID-like or alphanumeric)
    if !tenant_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return json_error(
            400,
            &ErrorResponse {
                error: "invalid_request".to_string(),
                message: "Invalid tenant ID format".to_string(),
                request_id: None,
            },
        );
    }

    let spec = match &shared.user_modules_dir {
        Some(dir) if dir.is_dir() => openapi::aggregate_tenant_spec(dir, tenant_id),
        _ => None,
    };

    match spec {
        Some(s) => json_response(200, &s),
        None => json_error(
            404,
            &ErrorResponse::not_found(format!("Tenant not found: {tenant_id}")),
        ),
    }
}

/// Create a JSON response with the given status code and body.
fn json_response<T: serde::Serialize>(status: u16, body: &T) -> Result<Response<Full<Bytes>>> {
    let json = serde_json::to_string(body)?;
    Ok(Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(json)))?)
}

/// Create a JSON error response.
fn json_error(status: u16, error: &ErrorResponse) -> Result<Response<Full<Bytes>>> {
    json_response(status, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests with SharedState require a complete Host setup.
    // These unit tests focus on serialization and response format.
    // Integration tests are in tests/gateway_tests.rs

    #[test]
    fn test_handlers_response_serialization() {
        let response = HandlersResponse {
            data: vec![HandlerInfo {
                id: "auth".to_string(),
                handler_type: "wasm".to_string(),
                attributes: HandlerAttributes {
                    name: "auth".to_string(),
                    size_bytes: 12345,
                    has_openapi: true,
                    tenant_id: None,
                },
                links: HandlerLinks {
                    self_link: "/run/auth/".to_string(),
                    openapi: Some("/_mik/openapi/platform".to_string()),
                },
            }],
            meta: HandlersMetadata {
                total: 1,
                platform_count: 1,
                tenant_count: 0,
                timestamp: "2024-01-01T00:00:00Z".to_string(),
            },
        };

        let json = serde_json::to_string_pretty(&response).unwrap();

        // Verify JSON:API-like structure
        assert!(json.contains("\"data\""));
        assert!(json.contains("\"type\""));
        assert!(json.contains("\"attributes\""));
        assert!(json.contains("\"links\""));
        assert!(json.contains("\"meta\""));
    }
}
