//! Configuration types for the mik runtime.
//!
//! This module provides configuration structs for loading and validating
//! mik project settings from TOML files. It includes:
//!
//! - [`Config`] - Root configuration struct
//! - [`ServerConfig`] - HTTP server settings
//! - [`Package`] - Project metadata
//! - [`RouteConfig`] - URL routing rules
//!
//! All configuration types support serde deserialization and provide
//! sensible defaults suitable for development use.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::constants;

/// Result of configuration validation.
#[derive(Debug, Default)]
#[allow(dead_code)] // Public API - fields may not be used internally
pub struct ValidationResult {
    /// Non-fatal warnings that should be logged but don't prevent operation.
    pub warnings: Vec<String>,
}

#[allow(dead_code)] // Public API - methods may not be used internally
impl ValidationResult {
    /// Returns true if there are any warnings.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

/// mikrozen.toml configuration structure (legacy format).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Config {
    pub package: Package,
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
    #[serde(default)]
    pub server: Option<ServerConfig>,
}

/// Optional server configuration (for validation purposes).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub modules: Option<String>,
    #[serde(default = "default_cache_size")]
    pub cache_size: usize,
}

fn default_port() -> u16 {
    constants::DEFAULT_PORT
}

fn default_cache_size() -> usize {
    constants::DEFAULT_CACHE_SIZE
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Package {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RouteConfig {
    pub name: String,
    pub path: String,
    pub method: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[allow(dead_code)]
impl Config {
    /// Load configuration from mikrozen.toml in the current directory.
    ///
    /// # Errors
    ///
    /// Returns an error if mikrozen.toml cannot be read or contains invalid TOML.
    pub fn load() -> Result<Self> {
        Self::load_from("mikrozen.toml")
    }

    /// Load configuration from the specified path.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read (IO error)
    /// - The file contains invalid TOML syntax
    /// - Required fields are missing or have invalid types
    pub fn load_from<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    /// Validate configuration with comprehensive checks.
    ///
    /// Returns a `ValidationResult` containing any non-fatal warnings.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails with one or more errors:
    /// - Empty package name or version
    /// - Empty or invalid route paths
    /// - Invalid HTTP methods in routes
    pub fn validate(&self) -> Result<ValidationResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // 1. Validate package metadata
        if self.package.name.is_empty() {
            errors.push("package.name cannot be empty".to_string());
        }

        if self.package.version.is_empty() {
            errors.push("package.version cannot be empty".to_string());
        }

        // 2. Validate routes
        for route in &self.routes {
            if route.name.is_empty() {
                errors.push("route name cannot be empty".to_string());
            }
            if route.path.is_empty() {
                errors.push(format!("route path cannot be empty for '{}'", route.name));
            } else if !route.path.starts_with('/') {
                errors.push(format!(
                    "route path must start with '/' for '{}' (got: '{}')",
                    route.name, route.path
                ));
            }

            let valid_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
            let method = route.method.to_uppercase();
            if !valid_methods.contains(&method.as_str()) {
                errors.push(format!(
                    "Invalid HTTP method '{}' for route '{}'. Valid methods: {}",
                    route.method,
                    route.name,
                    valid_methods.join(", ")
                ));
            }
        }

        // 3. Validate server configuration (if present)
        if let Some(server) = &self.server {
            // Port range validation (1-65535, not 0)
            if server.port == 0 {
                errors.push(
                    "Server port cannot be 0. Use a valid port number (1-65535)\n  \
                     Common ports: 3000 (default), 8080, 8000"
                        .to_string(),
                );
            }

            // Warn on system ports (< 1024)
            if server.port < 1024 && server.port > 0 {
                warnings.push(format!(
                    "Server port {} is a system/privileged port (< 1024)\n  \
                     Recommendation: Use ports >= 1024 (e.g., 3000, 8080, 8000) to avoid permission issues",
                    server.port
                ));
            }

            // Warn on unusual ports
            if server.port > 49151 {
                warnings.push(format!(
                    "Server port {} is in the dynamic/private port range (49152-65535)\n  \
                     Recommendation: Use well-known ports like 3000, 8080, or 8000",
                    server.port
                ));
            }

            // Cache size validation
            if server.cache_size == 0 {
                errors.push(
                    "Server cache_size cannot be 0. Set a positive number (default: 10)\n  \
                     Recommended: 10-100 depending on available memory"
                        .to_string(),
                );
            }

            // Warn on very high cache sizes
            if server.cache_size > 1000 {
                warnings.push(format!(
                    "Server cache_size {} is very high (> 1000)\n  \
                     Recommendation: Use cache_size between 10-100 for typical usage\n  \
                     High cache sizes may consume excessive memory",
                    server.cache_size
                ));
            }

            // Path existence check for modules directory
            if let Some(modules_path) = &server.modules
                && !modules_path.is_empty()
            {
                let path = Path::new(modules_path);
                if !path.exists() {
                    warnings.push(format!(
                        "Modules directory does not exist: {modules_path}\n  \
                         Create it with: mkdir -p {modules_path}"
                    ));
                } else if !path.is_dir() {
                    errors.push(format!(
                        "Modules path is not a directory: {modules_path}\n  \
                         Expected a directory containing WASM modules"
                    ));
                }
            }
        }

        // Return errors if any
        if !errors.is_empty() {
            anyhow::bail!(
                "Configuration validation failed:\n  - {}",
                errors.join("\n  - ")
            );
        }

        // Return warnings (caller decides how to handle/display them)
        Ok(ValidationResult { warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let toml_str = r#"
[package]
name = "my-handler"
version = "0.1.0"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.package.name, "my-handler");
        assert_eq!(config.package.version, "0.1.0");
        assert_eq!(config.routes.len(), 0);
    }

    #[test]
    fn test_parse_with_routes() {
        let toml_str = r#"
[package]
name = "my-handler"
version = "0.1.0"
description = "My handler service"

[[routes]]
name = "get_user"
path = "/users/:id"
method = "GET"
description = "Get user by ID"

[[routes]]
name = "create_user"
path = "/users"
method = "POST"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.routes.len(), 2);
        assert_eq!(config.routes[0].name, "get_user");
        assert_eq!(config.routes[0].path, "/users/:id");
        assert_eq!(config.routes[0].method, "GET");
    }

    #[test]
    fn test_validate_valid_config() {
        let config = Config {
            package: Package {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                description: None,
            },
            routes: vec![RouteConfig {
                name: "test".to_string(),
                path: "/test".to_string(),
                method: "GET".to_string(),
                description: None,
            }],
            server: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_path() {
        let config = Config {
            package: Package {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                description: None,
            },
            routes: vec![RouteConfig {
                name: "test".to_string(),
                path: "test".to_string(), // Missing leading /
                method: "GET".to_string(),
                description: None,
            }],
            server: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_port_zero() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[server]
port = 0
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("port cannot be 0"));
    }

    #[test]
    fn test_validate_port_valid_range() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[server]
port = 8080
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_cache_size_zero() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[server]
cache_size = 0
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cache_size cannot be 0"));
    }

    #[test]
    fn test_validate_cache_size_valid() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[server]
cache_size = 50
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_modules_path_not_directory() {
        use std::fs::File;
        use std::io::Write;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("not-a-dir");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"test").unwrap();

        let config = Config {
            package: Package {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                description: None,
            },
            routes: vec![],
            server: Some(ServerConfig {
                port: 3000,
                modules: Some(file_path.to_string_lossy().to_string()),
                cache_size: 10,
            }),
        };

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not a directory"));
    }

    #[test]
    fn test_validate_modules_path_exists() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let modules_dir = dir.path().join("modules");
        std::fs::create_dir(&modules_dir).unwrap();

        let config = Config {
            package: Package {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                description: None,
            },
            routes: vec![],
            server: Some(ServerConfig {
                port: 3000,
                modules: Some(modules_dir.to_string_lossy().to_string()),
                cache_size: 10,
            }),
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_http_method() {
        let config = Config {
            package: Package {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                description: None,
            },
            routes: vec![RouteConfig {
                name: "test".to_string(),
                path: "/test".to_string(),
                method: "INVALID".to_string(),
                description: None,
            }],
            server: None,
        };

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid HTTP method"));
        assert!(err.contains("INVALID"));
    }

    #[test]
    fn test_validate_empty_package_name() {
        let config = Config {
            package: Package {
                name: String::new(),
                version: "0.1.0".to_string(),
                description: None,
            },
            routes: vec![],
            server: None,
        };

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("package.name cannot be empty"));
    }

    #[test]
    fn test_validate_empty_package_version() {
        let config = Config {
            package: Package {
                name: "test".to_string(),
                version: String::new(),
                description: None,
            },
            routes: vec![],
            server: None,
        };

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("package.version cannot be empty"));
    }

    #[test]
    fn test_validate_multiple_errors() {
        let config = Config {
            package: Package {
                name: String::new(),
                version: String::new(),
                description: None,
            },
            routes: vec![RouteConfig {
                name: String::new(),
                path: String::new(),
                method: "INVALID".to_string(),
                description: None,
            }],
            server: Some(ServerConfig {
                port: 0,
                modules: None,
                cache_size: 0,
            }),
        };

        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // Should contain multiple errors
        assert!(err.contains("package.name"));
        assert!(err.contains("package.version"));
        assert!(err.contains("port"));
        assert!(err.contains("cache_size"));
    }

    #[test]
    fn test_server_config_defaults() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[server]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.server.is_some());
        let server = config.server.unwrap();
        assert_eq!(server.port, constants::DEFAULT_PORT);
        assert_eq!(server.cache_size, constants::DEFAULT_CACHE_SIZE);
    }

    #[test]
    fn test_config_without_server() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.server.is_none());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_complete_valid_config() {
        let toml_str = r#"
[package]
name = "my-service"
version = "1.0.0"
description = "A test service"

[[routes]]
name = "health"
path = "/health"
method = "GET"
description = "Health check"

[[routes]]
name = "users"
path = "/users/:id"
method = "GET"

[server]
port = 8080
cache_size = 20
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(config.package.name, "my-service");
        assert_eq!(config.package.version, "1.0.0");
        assert_eq!(config.routes.len(), 2);
        assert!(config.server.is_some());
        let server = config.server.unwrap();
        assert_eq!(server.port, 8080);
        assert_eq!(server.cache_size, 20);
    }
}
