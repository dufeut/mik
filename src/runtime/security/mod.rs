//! Security utilities for input sanitization and path traversal prevention.
//!
//! This module provides functions for validating and sanitizing user-provided paths and names
//! to prevent directory traversal attacks, null byte injection, and Windows-specific
//! path exploits.
//!
//! # Overview
//!
//! The security module is organized into three submodules:
//!
//! - [`error`] - Error types for security validation failures
//! - [`path`] - Path sanitization and validation functions
//! - [`module`] - Module name sanitization functions
//!
//! # Key Functions
//!
//! - [`sanitize_file_path`] - Validates file paths, blocks traversal via `..`
//! - [`sanitize_module_name`] - Validates module names, blocks path separators
//! - [`validate_windows_path`] - Blocks reserved device names, UNC paths, and ADS
//! - [`validate_path_within_base`] - Prevents symlink-based traversal (TOCTOU)
//!
//! # Examples
//!
//! ```
//! use mik::security::{sanitize_file_path, sanitize_module_name};
//!
//! // Validate a file path
//! let path = sanitize_file_path("assets/style.css").unwrap();
//!
//! // Validate a module name
//! let name = sanitize_module_name("my-module").unwrap();
//! ```

mod error;
mod module;
mod path;

// Re-export all public items
// Note: Some re-exports may appear unused but are part of the public API
#[allow(unused_imports)]
pub use error::{ModuleNameError, PathTraversalError};
pub use module::sanitize_module_name;
#[allow(unused_imports)]
pub use path::{
    WINDOWS_RESERVED_NAMES, sanitize_file_path, validate_path_within_base, validate_windows_path,
};
