//! Module discovery for gateway API.
//!
//! Scans directories for .wasm and .openapi.json files to provide
//! handler metadata to the gateway.

use super::types::{DiscoveredModule, DiscoveredTenant};
use std::path::Path;
use tracing::debug;

/// Extension for WASM module files.
const WASM_EXT: &str = "wasm";

/// Extension for OpenAPI spec files.
const OPENAPI_EXT: &str = "openapi.json";

/// Discover all WASM modules in a directory.
///
/// Scans the given directory for `.wasm` files and their corresponding
/// `.openapi.json` files.
///
/// # Arguments
///
/// * `dir` - Directory to scan for modules
/// * `tenant_id` - Optional tenant ID if scanning a tenant directory
///
/// # Returns
///
/// A list of discovered modules with their metadata.
pub fn discover_modules(dir: &Path, tenant_id: Option<&str>) -> Vec<DiscoveredModule> {
    let mut modules = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            debug!("Failed to read directory {}: {}", dir.display(), e);
            return modules;
        },
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip non-files
        if !path.is_file() {
            continue;
        }

        // Check for .wasm extension
        let extension = path.extension().and_then(|e| e.to_str());
        if extension != Some(WASM_EXT) {
            continue;
        }

        // Extract module name (file stem without extension)
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        // Get file size
        let size_bytes = match std::fs::metadata(&path) {
            Ok(meta) => meta.len(),
            Err(_) => 0,
        };

        // Check for corresponding .openapi.json file
        let openapi_filename = format!("{name}.{OPENAPI_EXT}");
        let openapi_path = dir.join(&openapi_filename);
        let has_openapi = openapi_path.is_file();

        modules.push(DiscoveredModule {
            name,
            wasm_path: path,
            size_bytes,
            openapi_path: if has_openapi {
                Some(openapi_path)
            } else {
                None
            },
            tenant_id: tenant_id.map(String::from),
        });
    }

    // Sort by name for consistent ordering
    modules.sort_by(|a, b| a.name.cmp(&b.name));

    modules
}

/// Discover all tenant directories.
///
/// Scans the user-modules directory for tenant subdirectories.
/// Each subdirectory is expected to be named with the tenant UUID.
///
/// # Arguments
///
/// * `user_modules_dir` - Base directory containing tenant directories
///
/// # Returns
///
/// A list of discovered tenants with their metadata.
pub fn discover_tenants(user_modules_dir: &Path) -> Vec<DiscoveredTenant> {
    let mut tenants = Vec::new();

    if !user_modules_dir.exists() {
        debug!(
            "User modules directory does not exist: {}",
            user_modules_dir.display()
        );
        return tenants;
    }

    let entries = match std::fs::read_dir(user_modules_dir) {
        Ok(entries) => entries,
        Err(e) => {
            debug!(
                "Failed to read user modules directory {}: {}",
                user_modules_dir.display(),
                e
            );
            return tenants;
        },
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip non-directories
        if !path.is_dir() {
            continue;
        }

        // Extract tenant ID (directory name)
        let id = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        // Count modules in this tenant directory
        let module_count = discover_modules(&path, Some(&id)).len();

        tenants.push(DiscoveredTenant {
            id,
            path,
            module_count,
        });
    }

    // Sort by ID for consistent ordering
    tenants.sort_by(|a, b| a.id.cmp(&b.id));

    tenants
}

/// Discover all modules (platform + tenant) for the handlers endpoint.
///
/// # Arguments
///
/// * `modules_dir` - Platform modules directory
/// * `user_modules_dir` - Optional user modules directory (for tenant handlers)
///
/// # Returns
///
/// A tuple of (platform_modules, tenant_modules).
pub fn discover_all_modules(
    modules_dir: &Path,
    user_modules_dir: Option<&Path>,
) -> (Vec<DiscoveredModule>, Vec<DiscoveredModule>) {
    let platform_modules = discover_modules(modules_dir, None);

    let tenant_modules = user_modules_dir
        .map(|dir| {
            discover_tenants(dir)
                .into_iter()
                .flat_map(|tenant| discover_modules(&tenant.path, Some(&tenant.id)))
                .collect()
        })
        .unwrap_or_default();

    (platform_modules, tenant_modules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_module(dir: &Path, name: &str, with_openapi: bool) {
        // Create a minimal WASM file (magic + version)
        let wasm_path = dir.join(format!("{name}.wasm"));
        fs::write(&wasm_path, [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]).unwrap();

        if with_openapi {
            let openapi_path = dir.join(format!("{name}.openapi.json"));
            fs::write(&openapi_path, r#"{"openapi":"3.0.0"}"#).unwrap();
        }
    }

    #[test]
    fn test_discover_modules_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let modules = discover_modules(temp_dir.path(), None);
        assert!(modules.is_empty());
    }

    #[test]
    fn test_discover_modules_with_wasm() {
        let temp_dir = TempDir::new().unwrap();
        create_test_module(temp_dir.path(), "auth", true);
        create_test_module(temp_dir.path(), "payments", false);

        let modules = discover_modules(temp_dir.path(), None);

        assert_eq!(modules.len(), 2);

        let auth = modules.iter().find(|m| m.name == "auth").unwrap();
        assert!(auth.openapi_path.is_some());
        assert!(auth.tenant_id.is_none());

        let payments = modules.iter().find(|m| m.name == "payments").unwrap();
        assert!(payments.openapi_path.is_none());
    }

    #[test]
    fn test_discover_modules_with_tenant_id() {
        let temp_dir = TempDir::new().unwrap();
        create_test_module(temp_dir.path(), "orders", true);

        let modules = discover_modules(temp_dir.path(), Some("tenant-123"));

        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].tenant_id.as_deref(), Some("tenant-123"));
    }

    #[test]
    fn test_discover_tenants() {
        let temp_dir = TempDir::new().unwrap();

        // Create tenant directories
        let tenant1_dir = temp_dir.path().join("tenant-abc");
        let tenant2_dir = temp_dir.path().join("tenant-xyz");
        fs::create_dir_all(&tenant1_dir).unwrap();
        fs::create_dir_all(&tenant2_dir).unwrap();

        // Add modules to tenant1
        create_test_module(&tenant1_dir, "orders", true);
        create_test_module(&tenant1_dir, "inventory", false);

        // tenant2 is empty

        let discovered = discover_tenants(temp_dir.path());

        assert_eq!(discovered.len(), 2);

        let abc = discovered.iter().find(|t| t.id == "tenant-abc").unwrap();
        assert_eq!(abc.module_count, 2);

        let xyz = discovered.iter().find(|t| t.id == "tenant-xyz").unwrap();
        assert_eq!(xyz.module_count, 0);
    }

    #[test]
    fn test_discover_all_modules() {
        let temp_dir = TempDir::new().unwrap();

        // Create platform modules directory
        let platform_dir = temp_dir.path().join("modules");
        fs::create_dir_all(&platform_dir).unwrap();
        create_test_module(&platform_dir, "auth", true);

        // Create user modules directory with tenant
        let user_dir = temp_dir.path().join("user-modules");
        let tenant_dir = user_dir.join("tenant-123");
        fs::create_dir_all(&tenant_dir).unwrap();
        create_test_module(&tenant_dir, "orders", true);

        let (platform, tenant) = discover_all_modules(&platform_dir, Some(&user_dir));

        assert_eq!(platform.len(), 1);
        assert_eq!(platform[0].name, "auth");
        assert!(platform[0].tenant_id.is_none());

        assert_eq!(tenant.len(), 1);
        assert_eq!(tenant[0].name, "orders");
        assert_eq!(tenant[0].tenant_id.as_deref(), Some("tenant-123"));
    }
}
