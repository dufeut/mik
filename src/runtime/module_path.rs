//! Module path abstraction for platform and tenant modules.
//!
//! Provides a unified way to handle module paths for both platform modules
//! (in `modules/`) and tenant modules (in `user-modules/{tenant-id}/`).

use std::path::{Path, PathBuf};

/// Represents a module path, either platform or tenant-scoped.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ModulePath {
    /// Platform module: `modules/{name}.wasm`
    Platform { name: String },
    /// Tenant module: `user-modules/{tenant_id}/{name}.wasm`
    Tenant { tenant_id: String, name: String },
}

impl ModulePath {
    /// Parse a module path from a URL segment.
    ///
    /// - `"hello"` → `Platform { name: "hello" }`
    /// - `"tenant-abc/orders"` → `Tenant { tenant_id: "tenant-abc", name: "orders" }`
    ///
    /// # Arguments
    ///
    /// * `segment` - The URL path segment after `/run/`
    #[must_use]
    pub fn from_url_segment(segment: &str) -> Self {
        // Check if segment contains exactly one slash (tenant/module pattern)
        if let Some((first, rest)) = segment.split_once('/') {
            // Check if rest contains another slash (would be handler path, not module name)
            if !rest.contains('/') && !first.is_empty() && !rest.is_empty() {
                return Self::Tenant {
                    tenant_id: first.to_string(),
                    name: rest.to_string(),
                };
            }
        }
        Self::Platform {
            name: segment.to_string(),
        }
    }

    /// Get the cache key for this module.
    ///
    /// Platform modules use just the name, tenant modules use `tenant:{id}/{name}`.
    #[must_use]
    pub fn cache_key(&self) -> String {
        match self {
            Self::Platform { name } => name.clone(),
            Self::Tenant { tenant_id, name } => format!("tenant:{tenant_id}/{name}"),
        }
    }

    /// Get the module name (without tenant prefix).
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Platform { name } | Self::Tenant { name, .. } => name,
        }
    }

    /// Get the tenant ID if this is a tenant module.
    #[must_use]
    pub fn tenant_id(&self) -> Option<&str> {
        match self {
            Self::Platform { .. } => None,
            Self::Tenant { tenant_id, .. } => Some(tenant_id),
        }
    }

    /// Check if this is a tenant module.
    #[must_use]
    pub fn is_tenant(&self) -> bool {
        matches!(self, Self::Tenant { .. })
    }

    /// Resolve to the WASM file path on the filesystem.
    ///
    /// # Arguments
    ///
    /// * `modules_dir` - Directory for platform modules
    /// * `user_modules_dir` - Directory for tenant modules (optional)
    ///
    /// # Returns
    ///
    /// The path to the WASM file, or None if this is a tenant module
    /// and user_modules_dir is not configured.
    #[must_use]
    pub fn wasm_path(
        &self,
        modules_dir: &Path,
        user_modules_dir: Option<&Path>,
    ) -> Option<PathBuf> {
        match self {
            Self::Platform { name } => Some(modules_dir.join(format!("{name}.wasm"))),
            Self::Tenant { tenant_id, name } => {
                user_modules_dir.map(|dir| dir.join(tenant_id).join(format!("{name}.wasm")))
            },
        }
    }

    /// Resolve to the OpenAPI spec file path on the filesystem.
    ///
    /// # Arguments
    ///
    /// * `modules_dir` - Directory for platform modules
    /// * `user_modules_dir` - Directory for tenant modules (optional)
    ///
    /// # Returns
    ///
    /// The path to the OpenAPI spec file, or None if this is a tenant module
    /// and user_modules_dir is not configured.
    #[must_use]
    pub fn openapi_path(
        &self,
        modules_dir: &Path,
        user_modules_dir: Option<&Path>,
    ) -> Option<PathBuf> {
        match self {
            Self::Platform { name } => Some(modules_dir.join(format!("{name}.openapi.json"))),
            Self::Tenant { tenant_id, name } => {
                user_modules_dir.map(|dir| dir.join(tenant_id).join(format!("{name}.openapi.json")))
            },
        }
    }

    /// Get the handler path for response headers.
    ///
    /// Returns the full path identifier used in `X-Mik-Handler` header.
    #[must_use]
    pub fn handler_name(&self) -> String {
        match self {
            Self::Platform { name } => name.clone(),
            Self::Tenant { tenant_id, name } => format!("{tenant_id}/{name}"),
        }
    }
}

impl std::fmt::Display for ModulePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Platform { name } => write!(f, "{name}"),
            Self::Tenant { tenant_id, name } => write!(f, "{tenant_id}/{name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_platform_module() {
        let path = ModulePath::from_url_segment("hello");
        assert_eq!(
            path,
            ModulePath::Platform {
                name: "hello".to_string()
            }
        );
        assert_eq!(path.cache_key(), "hello");
        assert_eq!(path.name(), "hello");
        assert!(!path.is_tenant());
        assert!(path.tenant_id().is_none());
    }

    #[test]
    fn test_parse_tenant_module() {
        let path = ModulePath::from_url_segment("tenant-abc/orders");
        assert_eq!(
            path,
            ModulePath::Tenant {
                tenant_id: "tenant-abc".to_string(),
                name: "orders".to_string()
            }
        );
        assert_eq!(path.cache_key(), "tenant:tenant-abc/orders");
        assert_eq!(path.name(), "orders");
        assert!(path.is_tenant());
        assert_eq!(path.tenant_id(), Some("tenant-abc"));
    }

    #[test]
    fn test_parse_uuid_tenant() {
        let path = ModulePath::from_url_segment("550e8400-e29b-41d4-a716-446655440000/orders");
        assert_eq!(
            path,
            ModulePath::Tenant {
                tenant_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                name: "orders".to_string()
            }
        );
    }

    #[test]
    fn test_wasm_path_platform() {
        let path = ModulePath::Platform {
            name: "auth".to_string(),
        };
        let modules_dir = Path::new("/app/modules");
        let user_modules_dir = Path::new("/app/user-modules");

        let wasm = path.wasm_path(modules_dir, Some(user_modules_dir));
        assert_eq!(wasm, Some(PathBuf::from("/app/modules/auth.wasm")));
    }

    #[test]
    fn test_wasm_path_tenant() {
        let path = ModulePath::Tenant {
            tenant_id: "tenant-abc".to_string(),
            name: "orders".to_string(),
        };
        let modules_dir = Path::new("/app/modules");
        let user_modules_dir = Path::new("/app/user-modules");

        let wasm = path.wasm_path(modules_dir, Some(user_modules_dir));
        assert_eq!(
            wasm,
            Some(PathBuf::from("/app/user-modules/tenant-abc/orders.wasm"))
        );
    }

    #[test]
    fn test_wasm_path_tenant_no_user_modules() {
        let path = ModulePath::Tenant {
            tenant_id: "tenant-abc".to_string(),
            name: "orders".to_string(),
        };
        let modules_dir = Path::new("/app/modules");

        let wasm = path.wasm_path(modules_dir, None);
        assert!(wasm.is_none());
    }

    #[test]
    fn test_handler_name() {
        let platform = ModulePath::Platform {
            name: "auth".to_string(),
        };
        assert_eq!(platform.handler_name(), "auth");

        let tenant = ModulePath::Tenant {
            tenant_id: "tenant-abc".to_string(),
            name: "orders".to_string(),
        };
        assert_eq!(tenant.handler_name(), "tenant-abc/orders");
    }

    #[test]
    fn test_display() {
        let platform = ModulePath::Platform {
            name: "auth".to_string(),
        };
        assert_eq!(format!("{platform}"), "auth");

        let tenant = ModulePath::Tenant {
            tenant_id: "tenant-abc".to_string(),
            name: "orders".to_string(),
        };
        assert_eq!(format!("{tenant}"), "tenant-abc/orders");
    }
}
