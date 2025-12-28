//! Runtime error types for typed error handling.
//!
//! This module provides structured errors for the WASI HTTP runtime,
//! enabling better error handling and more informative error messages.

use std::path::PathBuf;

/// Result type for runtime operations.
#[allow(dead_code)] // Type alias for gradual migration from anyhow
pub type Result<T> = std::result::Result<T, Error>;

/// Runtime errors with structured context.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
#[allow(dead_code)] // Some variants defined for future use
#[allow(clippy::enum_variant_names)] // ScriptError is clearer than Script in error context
pub enum Error {
    /// Module not found in cache or filesystem.
    #[error("module not found: {name}")]
    ModuleNotFound { name: String },

    /// Module failed to load (compile error, invalid WASM).
    #[error("failed to load module '{name}': {reason}")]
    ModuleLoadFailed { name: String, reason: String },

    /// Module execution timed out.
    #[error("module '{name}' timed out after {timeout_secs}s")]
    ExecutionTimeout { name: String, timeout_secs: u64 },

    /// Circuit breaker is open (too many failures).
    #[error("circuit breaker open for module '{name}'")]
    CircuitBreakerOpen { name: String },

    /// Rate limit exceeded.
    #[error("rate limit exceeded: {reason}")]
    RateLimitExceeded { reason: String },

    /// Path traversal attempt detected.
    #[error("path traversal blocked: {path:?}")]
    PathTraversal { path: PathBuf },

    /// Script execution error.
    #[error("script '{name}' failed: {reason}")]
    ScriptError { name: String, reason: String },

    /// Script not found.
    #[error("script not found: {name}")]
    ScriptNotFound { name: String },

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// IO error with context.
    #[error("IO error in {context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// Wasmtime error wrapper.
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),

    /// HTTP error.
    #[error("HTTP error: {0}")]
    Http(String),

    /// Invalid request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
}

#[allow(dead_code)] // Helper constructors for ergonomic error creation
impl Error {
    /// Create an IO error with context.
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    /// Create a module not found error.
    pub fn module_not_found(name: impl Into<String>) -> Self {
        Self::ModuleNotFound { name: name.into() }
    }

    /// Create a module load failed error.
    pub fn module_load_failed(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ModuleLoadFailed {
            name: name.into(),
            reason: reason.into(),
        }
    }

    /// Create a script error.
    pub fn script_error(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ScriptError {
            name: name.into(),
            reason: reason.into(),
        }
    }

    /// Create a circuit breaker open error.
    pub fn circuit_breaker_open(name: impl Into<String>) -> Self {
        Self::CircuitBreakerOpen { name: name.into() }
    }

    /// Create a rate limit exceeded error.
    pub fn rate_limit_exceeded(reason: impl Into<String>) -> Self {
        Self::RateLimitExceeded {
            reason: reason.into(),
        }
    }

    /// Create a path traversal error.
    pub fn path_traversal(path: impl Into<PathBuf>) -> Self {
        Self::PathTraversal { path: path.into() }
    }

    /// Create an execution timeout error.
    pub fn execution_timeout(name: impl Into<String>, timeout_secs: u64) -> Self {
        Self::ExecutionTimeout {
            name: name.into(),
            timeout_secs,
        }
    }
}

/// Convert runtime error to HTTP status code.
impl Error {
    /// Get the appropriate HTTP status code for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::ModuleNotFound { .. } | Self::ScriptNotFound { .. } => 404,
            Self::PathTraversal { .. } | Self::InvalidRequest(_) => 400,
            Self::CircuitBreakerOpen { .. } => 503,
            Self::RateLimitExceeded { .. } => 429,
            Self::ExecutionTimeout { .. } => 504,
            Self::ModuleLoadFailed { .. }
            | Self::ScriptError { .. }
            | Self::Config(_)
            | Self::Io { .. }
            | Self::Wasmtime(_)
            | Self::Http(_) => 500,
        }
    }
}

impl Error {
    /// Convert to anyhow::Error for gradual migration.
    pub fn into_anyhow(self) -> anyhow::Error {
        anyhow::Error::from(self)
    }
}
