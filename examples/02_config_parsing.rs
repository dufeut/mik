//! # Configuration Parsing Example
//!
//! This example demonstrates how to parse and validate mik configuration files.
//! It covers both the legacy `mikrozen.toml` format (`Config`) and the modern
//! `mik.toml` manifest format (`Manifest`).
//!
//! ## What This Example Shows
//!
//! - Parsing TOML configuration into strongly-typed structs
//! - Validating configuration with detailed error messages
//! - Understanding the difference between Config and Manifest
//! - Handling validation warnings vs errors
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example 02_config_parsing
//! ```
//!
//! ## Related Documentation
//!
//! - [`mik::config::Config`] - Legacy configuration format
//! - [`mik::manifest::Manifest`] - Modern manifest format
//! - [CLAUDE.md](../CLAUDE.md) - Configuration reference

use anyhow::Result;

fn main() -> Result<()> {
    println!("=== mik Configuration Parsing Example ===\n");

    // =========================================================================
    // Part 1: Parsing the Legacy Config Format
    // =========================================================================
    //
    // The `Config` type represents the legacy `mikrozen.toml` format.
    // It's simpler and focused on basic package and route definitions.

    println!("--- Part 1: Legacy Config Format (mikrozen.toml) ---\n");

    // Example TOML content (would normally come from a file)
    let config_toml = r#"
[package]
name = "my-handler"
version = "0.1.0"
description = "An example HTTP handler"

[[routes]]
name = "get_users"
path = "/users"
method = "GET"
description = "List all users"

[[routes]]
name = "create_user"
path = "/users"
method = "POST"
description = "Create a new user"

[server]
port = 8080
cache_size = 50
"#;

    // Parse the TOML string into a Config struct.
    // In practice, you would use Config::load() or Config::load_from(path).
    let config: mik::config::Config = toml::from_str(config_toml)?;

    // Display parsed configuration
    println!("Parsed Config:");
    println!(
        "  Package: {} v{}",
        config.package.name, config.package.version
    );
    if let Some(desc) = &config.package.description {
        println!("  Description: {}", desc);
    }
    println!("  Routes: {} defined", config.routes.len());
    for route in &config.routes {
        println!("    - {} {} ({})", route.method, route.path, route.name);
    }
    if let Some(server) = &config.server {
        println!("  Server port: {}", server.port);
        println!("  Cache size: {}", server.cache_size);
    }

    // =========================================================================
    // Part 2: Validating Configuration
    // =========================================================================
    //
    // The validate() method performs comprehensive checks and returns
    // detailed, actionable error messages.

    println!("\n--- Part 2: Configuration Validation ---\n");

    // Validate the parsed config
    match config.validate() {
        Ok(result) => {
            println!("Validation passed!");
            // Check for non-fatal warnings
            if result.has_warnings() {
                println!("Warnings:");
                for warning in &result.warnings {
                    println!("  - {}", warning);
                }
            }
        },
        Err(e) => {
            println!("Validation failed:\n{}", e);
        },
    }

    // =========================================================================
    // Part 3: Demonstrating Validation Errors
    // =========================================================================
    //
    // Let's see what happens with invalid configuration.

    println!("\n--- Part 3: Validation Error Examples ---\n");

    // Example 1: Invalid route path (missing leading /)
    let invalid_path_toml = r#"
[package]
name = "test"
version = "0.1.0"

[[routes]]
name = "bad_route"
path = "users"
method = "GET"
"#;

    let invalid_config: mik::config::Config = toml::from_str(invalid_path_toml)?;
    match invalid_config.validate() {
        Ok(_) => println!("Unexpectedly passed validation"),
        Err(e) => println!("Expected error (invalid path):\n  {}\n", e),
    }

    // Example 2: Invalid HTTP method
    let invalid_method_toml = r#"
[package]
name = "test"
version = "0.1.0"

[[routes]]
name = "bad_method"
path = "/users"
method = "INVALID"
"#;

    let invalid_config: mik::config::Config = toml::from_str(invalid_method_toml)?;
    match invalid_config.validate() {
        Ok(_) => println!("Unexpectedly passed validation"),
        Err(e) => println!("Expected error (invalid method):\n  {}\n", e),
    }

    // Example 3: Invalid port (zero)
    let invalid_port_toml = r#"
[package]
name = "test"
version = "0.1.0"

[server]
port = 0
"#;

    let invalid_config: mik::config::Config = toml::from_str(invalid_port_toml)?;
    match invalid_config.validate() {
        Ok(_) => println!("Unexpectedly passed validation"),
        Err(e) => println!("Expected error (invalid port):\n  {}\n", e),
    }

    // =========================================================================
    // Part 4: Modern Manifest Format (mik.toml)
    // =========================================================================
    //
    // The `Manifest` type represents the modern `mik.toml` format with
    // additional features like dependencies, tracing, and load balancing.

    println!("--- Part 4: Modern Manifest Format (mik.toml) ---\n");

    let manifest_toml = r#"
[project]
name = "my-service"
version = "1.0.0"
description = "A production-ready HTTP service"

[server]
port = 3000
modules = "modules/"
cache_size = 100
max_cache_mb = 256
execution_timeout_secs = 30
max_concurrent_requests = 1000
http_allowed = ["api.example.com", "*.supabase.co"]

[tracing]
enabled = true
otlp_endpoint = "http://localhost:4317"
service_name = "my-service"

[dependencies]
router = "1.0"
utils = { path = "../utils" }
"#;

    // Parse the manifest
    let manifest: mik::manifest::Manifest = toml::from_str(manifest_toml)?;

    println!("Parsed Manifest:");
    println!(
        "  Project: {} v{}",
        manifest.project.name, manifest.project.version
    );
    if let Some(desc) = &manifest.project.description {
        println!("  Description: {}", desc);
    }
    println!("  Server:");
    println!("    Port: {}", manifest.server.port);
    println!("    Modules: {}", manifest.server.modules);
    println!("    Cache size: {}", manifest.server.cache_size);
    println!("    Timeout: {}s", manifest.server.execution_timeout_secs);
    println!("    HTTP allowed: {:?}", manifest.server.http_allowed);
    println!("  Tracing:");
    println!("    Enabled: {}", manifest.tracing.enabled);
    if let Some(endpoint) = &manifest.tracing.otlp_endpoint {
        println!("    OTLP endpoint: {}", endpoint);
    }
    println!("    Service name: {}", manifest.tracing.service_name);
    println!("  Dependencies: {} defined", manifest.dependencies.len());
    for name in manifest.dependencies.keys() {
        println!("    - {}", name);
    }

    // =========================================================================
    // Part 5: Manifest Validation
    // =========================================================================
    //
    // Manifest validation is more comprehensive, checking project names,
    // versions (semver), and dependency specifications.

    println!("\n--- Part 5: Manifest Validation ---\n");

    // Validate using a dummy path (validation checks path dependencies exist)
    // In practice, you would pass the actual manifest file path.
    use std::path::Path;
    let manifest_path = Path::new("example.toml");

    match manifest.validate(manifest_path) {
        Ok(()) => println!("Manifest validation passed!"),
        Err(e) => println!("Manifest validation failed:\n{}", e),
    }

    // Note: The validation will fail for path dependencies that don't exist.
    // In a real application, you would validate against an actual file.

    // =========================================================================
    // Part 6: Programmatic Configuration
    // =========================================================================
    //
    // You can also create configuration structs programmatically.

    println!("\n--- Part 6: Programmatic Configuration ---\n");

    let manual_config = mik::config::Config {
        package: mik::config::Package {
            name: "programmatic-service".to_string(),
            version: "2.0.0".to_string(),
            description: Some("Created programmatically".to_string()),
        },
        routes: vec![mik::config::RouteConfig {
            name: "health".to_string(),
            path: "/health".to_string(),
            method: "GET".to_string(),
            description: Some("Health check endpoint".to_string()),
        }],
        server: Some(mik::config::ServerConfig {
            port: 8080,
            modules: Some("modules/".to_string()),
            cache_size: 50,
        }),
    };

    println!("Programmatic config:");
    println!(
        "  Package: {} v{}",
        manual_config.package.name, manual_config.package.version
    );
    println!("  Validation: {:?}", manual_config.validate().is_ok());

    println!("\n=== Example Complete ===");

    Ok(())
}
