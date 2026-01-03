//! OpenAPI spec aggregation for gateway API.
//!
//! Merges multiple per-handler OpenAPI specs into a single aggregated spec
//! for the gateway to use for validation.

use super::discovery::{discover_modules, discover_tenants};
use serde_json::{Map, Value};
use std::path::Path;
use tracing::{debug, warn};

/// Default OpenAPI version.
const OPENAPI_VERSION: &str = "3.0.3";

/// Aggregate OpenAPI specs for platform handlers.
///
/// Reads all `.openapi.json` files from the modules directory and merges them
/// into a single OpenAPI 3.0.3 document.
///
/// # Arguments
///
/// * `modules_dir` - Directory containing platform modules
///
/// # Returns
///
/// A JSON value representing the aggregated OpenAPI spec.
pub fn aggregate_platform_spec(modules_dir: &Path) -> Value {
    let modules = discover_modules(modules_dir, None);

    let specs: Vec<(String, Value)> = modules
        .iter()
        .filter_map(|m| {
            m.openapi_path
                .as_ref()
                .and_then(|path| read_openapi_spec(path).map(|spec| (m.name.clone(), spec)))
        })
        .collect();

    aggregate_specs("Platform API", specs, "/run")
}

/// Aggregate OpenAPI specs for a specific tenant.
///
/// Reads all `.openapi.json` files from the tenant's directory and merges them
/// into a single OpenAPI 3.0.3 document.
///
/// # Arguments
///
/// * `user_modules_dir` - Base directory for user modules
/// * `tenant_id` - Tenant UUID
///
/// # Returns
///
/// A JSON value representing the aggregated OpenAPI spec, or None if tenant not found.
pub fn aggregate_tenant_spec(user_modules_dir: &Path, tenant_id: &str) -> Option<Value> {
    let tenant_dir = user_modules_dir.join(tenant_id);

    if !tenant_dir.is_dir() {
        debug!("Tenant directory not found: {}", tenant_dir.display());
        return None;
    }

    let modules = discover_modules(&tenant_dir, Some(tenant_id));

    let specs: Vec<(String, Value)> = modules
        .iter()
        .filter_map(|m| {
            m.openapi_path
                .as_ref()
                .and_then(|path| read_openapi_spec(path).map(|spec| (m.name.clone(), spec)))
        })
        .collect();

    // Use empty prefix for tenant specs - gateway provides friendly URLs
    // and rewrites to /tenant/{tenant_id}/... when proxying to mik
    Some(aggregate_specs(
        &format!("Tenant {tenant_id} API"),
        specs,
        "",
    ))
}

/// Read and parse an OpenAPI spec file.
fn read_openapi_spec(path: &Path) -> Option<Value> {
    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(spec) => Some(spec),
            Err(e) => {
                warn!("Failed to parse OpenAPI spec {}: {}", path.display(), e);
                None
            },
        },
        Err(e) => {
            warn!("Failed to read OpenAPI spec {}: {}", path.display(), e);
            None
        },
    }
}

/// Aggregate multiple OpenAPI specs into a single document.
///
/// Merges paths and components from each handler spec, prefixing paths
/// with the appropriate route prefix to avoid collisions.
///
/// # Arguments
///
/// * `title` - Title for the aggregated API
/// * `specs` - List of (handler_name, spec) tuples
/// * `route_prefix` - Route prefix, e.g., "/run" for platform or "/tenant/{tenant-id}" for tenant
fn aggregate_specs(title: &str, specs: Vec<(String, Value)>, route_prefix: &str) -> Value {
    let mut paths = Map::new();
    let mut schemas = Map::new();

    for (handler_name, spec) in specs {
        // Merge paths with handler prefix
        if let Some(spec_paths) = spec.get("paths").and_then(|p| p.as_object()) {
            merge_paths(&mut paths, spec_paths, &handler_name, route_prefix);
        }

        // Merge component schemas with handler prefix to avoid collisions
        if let Some(spec_schemas) = spec
            .get("components")
            .and_then(|c| c.get("schemas"))
            .and_then(|s| s.as_object())
        {
            merge_schemas(&mut schemas, spec_schemas, &handler_name);
        }
    }

    serde_json::json!({
        "openapi": OPENAPI_VERSION,
        "info": {
            "title": title,
            "version": "1.0.0"
        },
        "paths": paths,
        "components": {
            "schemas": schemas
        }
    })
}

/// Merge paths from a handler spec into the aggregated paths.
///
/// Prefixes each path with `{route_prefix}/{handler}` to match the mik routing convention.
fn merge_paths(
    aggregated: &mut Map<String, Value>,
    handler_paths: &Map<String, Value>,
    handler_name: &str,
    route_prefix: &str,
) {
    for (path, operations) in handler_paths {
        // Normalize path: ensure it starts with /
        let normalized_path = if path.starts_with('/') {
            path.clone()
        } else {
            format!("/{path}")
        };

        // Create the full path: {route_prefix}/{handler}{path}
        let full_path = format!("{route_prefix}/{handler_name}{normalized_path}");

        // Clone and update $ref paths in the operations
        let updated_operations = update_schema_refs(operations, handler_name);

        aggregated.insert(full_path, updated_operations);
    }
}

/// Merge component schemas from a handler spec.
///
/// Prefixes schema names with the handler name to avoid collisions
/// (e.g., `User` becomes `auth_User`).
fn merge_schemas(
    aggregated: &mut Map<String, Value>,
    handler_schemas: &Map<String, Value>,
    handler_name: &str,
) {
    for (schema_name, schema) in handler_schemas {
        let prefixed_name = format!("{handler_name}_{schema_name}");

        // Clone and update internal $ref paths
        let updated_schema = update_schema_refs(schema, handler_name);

        aggregated.insert(prefixed_name, updated_schema);
    }
}

/// Update $ref paths in a JSON value to use prefixed schema names.
fn update_schema_refs(value: &Value, handler_name: &str) -> Value {
    match value {
        Value::Object(obj) => {
            let mut new_obj = Map::new();
            for (key, val) in obj {
                if key == "$ref"
                    && let Some(ref_str) = val.as_str()
                    && let Some(schema_name) = ref_str.strip_prefix("#/components/schemas/")
                {
                    // Transform "#/components/schemas/Foo" to "#/components/schemas/handler_Foo"
                    new_obj.insert(
                        key.clone(),
                        Value::String(format!("#/components/schemas/{handler_name}_{schema_name}")),
                    );
                    continue;
                }
                new_obj.insert(key.clone(), update_schema_refs(val, handler_name));
            }
            Value::Object(new_obj)
        },
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| update_schema_refs(v, handler_name))
                .collect(),
        ),
        _ => value.clone(),
    }
}

/// Get the list of available tenants that have OpenAPI specs.
pub fn list_tenants_with_specs(user_modules_dir: &Path) -> Vec<String> {
    discover_tenants(user_modules_dir)
        .into_iter()
        .filter(|t| t.module_count > 0)
        .map(|t| t.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_openapi_spec(dir: &Path, name: &str, paths: &[(&str, &str)]) {
        let mut paths_obj = serde_json::Map::new();
        for (path, method) in paths {
            let mut method_obj = serde_json::Map::new();
            method_obj.insert(
                (*method).to_string(),
                serde_json::json!({
                    "summary": format!("{} {}", method.to_uppercase(), path),
                    "responses": {
                        "200": {
                            "description": "Success"
                        }
                    }
                }),
            );
            paths_obj.insert((*path).to_string(), Value::Object(method_obj));
        }

        let spec = serde_json::json!({
            "openapi": "3.0.0",
            "info": {
                "title": name,
                "version": "1.0.0"
            },
            "paths": paths_obj,
            "components": {
                "schemas": {
                    "Response": {
                        "type": "object",
                        "properties": {
                            "message": { "type": "string" }
                        }
                    }
                }
            }
        });

        let spec_path = dir.join(format!("{name}.openapi.json"));
        fs::write(spec_path, serde_json::to_string_pretty(&spec).unwrap()).unwrap();

        // Also create a dummy wasm file
        let wasm_path = dir.join(format!("{name}.wasm"));
        fs::write(wasm_path, [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]).unwrap();
    }

    #[test]
    fn test_aggregate_platform_spec_empty() {
        let temp_dir = TempDir::new().unwrap();
        let spec = aggregate_platform_spec(temp_dir.path());

        assert_eq!(spec["openapi"], "3.0.3");
        assert_eq!(spec["info"]["title"], "Platform API");
        assert!(spec["paths"].as_object().unwrap().is_empty());
    }

    #[test]
    fn test_aggregate_platform_spec_with_handlers() {
        let temp_dir = TempDir::new().unwrap();

        create_openapi_spec(
            temp_dir.path(),
            "auth",
            &[("/login", "post"), ("/logout", "post")],
        );
        create_openapi_spec(temp_dir.path(), "users", &[("/", "get"), ("/{id}", "get")]);

        let spec = aggregate_platform_spec(temp_dir.path());

        let paths = spec["paths"].as_object().unwrap();

        // Check that paths are prefixed correctly
        assert!(paths.contains_key("/run/auth/login"));
        assert!(paths.contains_key("/run/auth/logout"));
        assert!(paths.contains_key("/run/users/"));
        assert!(paths.contains_key("/run/users/{id}"));

        // Check that schemas are prefixed
        let schemas = spec["components"]["schemas"].as_object().unwrap();
        assert!(schemas.contains_key("auth_Response"));
        assert!(schemas.contains_key("users_Response"));
    }

    #[test]
    fn test_aggregate_tenant_spec() {
        let temp_dir = TempDir::new().unwrap();

        let tenant_dir = temp_dir.path().join("tenant-abc");
        fs::create_dir_all(&tenant_dir).unwrap();

        create_openapi_spec(&tenant_dir, "orders", &[("/", "get"), ("/", "post")]);

        let spec = aggregate_tenant_spec(temp_dir.path(), "tenant-abc").unwrap();

        assert_eq!(spec["info"]["title"], "Tenant tenant-abc API");

        let paths = spec["paths"].as_object().unwrap();
        // Tenant routes use friendly URLs (no prefix) - gateway rewrites to /tenant/... when proxying
        assert!(paths.contains_key("/orders/"));
    }

    #[test]
    fn test_aggregate_tenant_spec_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let spec = aggregate_tenant_spec(temp_dir.path(), "nonexistent");
        assert!(spec.is_none());
    }

    #[test]
    fn test_update_schema_refs() {
        let value = serde_json::json!({
            "content": {
                "application/json": {
                    "schema": {
                        "$ref": "#/components/schemas/User"
                    }
                }
            }
        });

        let updated = update_schema_refs(&value, "auth");

        assert_eq!(
            updated["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/auth_User"
        );
    }

    #[test]
    fn test_list_tenants_with_specs() {
        let temp_dir = TempDir::new().unwrap();

        // Create tenant with modules
        let tenant_with_modules = temp_dir.path().join("tenant-1");
        fs::create_dir_all(&tenant_with_modules).unwrap();
        create_openapi_spec(&tenant_with_modules, "orders", &[("/", "get")]);

        // Create empty tenant
        let empty_tenant = temp_dir.path().join("tenant-2");
        fs::create_dir_all(&empty_tenant).unwrap();

        let tenants = list_tenants_with_specs(temp_dir.path());

        assert_eq!(tenants.len(), 1);
        assert_eq!(tenants[0], "tenant-1");
    }
}
