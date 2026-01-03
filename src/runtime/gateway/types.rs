//! Response types for gateway API endpoints.
//!
//! Uses JSON:API-inspired format for standardized responses that are
//! easy to consume by any gateway implementation.

use serde::{Deserialize, Serialize};

/// Response for GET /_mik/handlers endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct HandlersResponse {
    /// List of handler resources.
    pub data: Vec<HandlerInfo>,
    /// Response metadata.
    pub meta: HandlersMetadata,
}

/// Individual handler resource (JSON:API style).
#[derive(Debug, Serialize, Deserialize)]
pub struct HandlerInfo {
    /// Handler identifier (e.g., "auth", "payments").
    pub id: String,
    /// Resource type (always "wasm").
    #[serde(rename = "type")]
    pub handler_type: String,
    /// Handler attributes.
    pub attributes: HandlerAttributes,
    /// Related links.
    pub links: HandlerLinks,
}

/// Handler attributes.
#[derive(Debug, Serialize, Deserialize)]
pub struct HandlerAttributes {
    /// Handler name (same as id).
    pub name: String,
    /// WASM module size in bytes.
    pub size_bytes: u64,
    /// Whether an OpenAPI spec exists for this handler.
    pub has_openapi: bool,
    /// Tenant ID if this is a tenant-specific handler.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

/// Handler-related links.
#[derive(Debug, Serialize, Deserialize)]
pub struct HandlerLinks {
    /// Path to invoke this handler.
    #[serde(rename = "self")]
    pub self_link: String,
    /// Path to the OpenAPI spec for this handler.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openapi: Option<String>,
}

/// Metadata for handlers response.
#[derive(Debug, Serialize, Deserialize)]
pub struct HandlersMetadata {
    /// Total number of handlers.
    pub total: usize,
    /// Number of platform handlers.
    pub platform_count: usize,
    /// Number of tenant handlers.
    pub tenant_count: usize,
    /// Timestamp when this data was generated (ISO 8601).
    pub timestamp: String,
}

/// Discovered module information.
#[derive(Debug, Clone)]
pub struct DiscoveredModule {
    /// Module name (without .wasm extension).
    pub name: String,
    /// Full path to the .wasm file.
    pub wasm_path: std::path::PathBuf,
    /// Size of the .wasm file in bytes.
    pub size_bytes: u64,
    /// Path to the .openapi.json file if it exists.
    pub openapi_path: Option<std::path::PathBuf>,
    /// Tenant ID if this is a tenant module.
    pub tenant_id: Option<String>,
}

/// Discovered tenant directory.
#[derive(Debug, Clone)]
pub struct DiscoveredTenant {
    /// Tenant ID (directory name).
    pub id: String,
    /// Full path to the tenant directory.
    pub path: std::path::PathBuf,
    /// Number of modules in this tenant directory.
    pub module_count: usize,
}

/// Standard error response format.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code (e.g., "not_found", "internal_error").
    pub error: String,
    /// Human-readable error message.
    pub message: String,
    /// Request ID for tracing (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ErrorResponse {
    /// Create a not found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            error: "not_found".to_string(),
            message: message.into(),
            request_id: None,
        }
    }

    /// Create an internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            error: "internal_error".to_string(),
            message: message.into(),
            request_id: None,
        }
    }

    /// Add request ID to the error.
    #[must_use]
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}
