//! # Hello World Example
//!
//! This is the simplest possible example of using the mik runtime.
//! It demonstrates how to create a basic WASI HTTP runtime configuration.
//!
//! ## What This Example Shows
//!
//! - Creating a `RuntimeBuilder` with minimal configuration
//! - Setting basic runtime parameters
//! - Understanding the default configuration values
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example 01_hello_world
//! ```
//!
//! Note: This example demonstrates configuration only. To actually serve
//! WASM components, you would need to:
//! 1. Create a `modules/` directory with `.wasm` files
//! 2. Call `Server::new(runtime, addr).serve().await` within a tokio runtime
//!
//! ## Related Documentation
//!
//! - [`mik::runtime::RuntimeBuilder`] - The builder used to configure the runtime
//! - [`mik::runtime::Runtime`] - The core runtime for WASM execution
//! - [`mik::runtime::Server`] - The HTTP server that wraps the runtime
//! - [CLAUDE.md](../CLAUDE.md) - Project architecture overview

use anyhow::Result;

fn main() -> Result<()> {
    // =========================================================================
    // Step 1: Create a RuntimeBuilder
    // =========================================================================
    //
    // The RuntimeBuilder provides a fluent API for configuring the WASI HTTP
    // runtime. You can either:
    // - Use `Runtime::builder()` for programmatic configuration
    // - Use `Runtime::builder().from_manifest_file("mik.toml")` to load from a file
    //
    // Here we use the programmatic approach to show all configuration options.

    let _builder = mik::runtime::Runtime::builder()
        // Set the directory containing WASM modules.
        // The runtime will look for `.wasm` files here and expose them
        // at routes like `/run/<module_name>/*`
        .modules_dir("modules/")
        // Set the LRU cache size (number of compiled modules to keep in memory).
        // Higher values improve performance but use more memory.
        // Default is auto-detected based on available RAM.
        .cache_size(100)
        // Set the WASM execution timeout in seconds.
        // Requests taking longer than this are terminated.
        // Default is 30 seconds.
        .execution_timeout_secs(30)
        // Set the maximum request body size in bytes.
        // Larger requests are rejected with 413 Payload Too Large.
        // Default is 10 MB (10 * 1024 * 1024).
        .max_body_size(10 * 1024 * 1024)
        // Set the maximum concurrent requests globally.
        // Excess requests receive 503 Service Unavailable.
        // Default is auto-detected based on CPU cores.
        .max_concurrent_requests(1000);

    // =========================================================================
    // Step 2: Build and Serve (Optional)
    // =========================================================================
    //
    // In a real application, you would call `.build()` to create the Runtime,
    // then wrap it in a Server and call `.serve().await` to start serving requests.
    //
    // We skip this step here because:
    // 1. We don't have actual WASM modules to serve
    // 2. It would require a tokio runtime and async context
    //
    // Here's what the full code would look like:
    //
    // ```rust
    // use mik::runtime::{Runtime, Server};
    //
    // #[tokio::main]
    // async fn main() -> Result<()> {
    //     let runtime = Runtime::builder()
    //         .modules_dir("modules/")
    //         .build()?;
    //
    //     let addr: std::net::SocketAddr = "127.0.0.1:3000".parse()?;
    //     println!("Serving on http://{}", addr);
    //     Server::new(runtime, addr).serve().await?;
    //     Ok(())
    // }
    // ```

    println!("=== mik Hello World Example ===\n");
    println!("Configuration:");
    println!("  Modules: modules/");
    println!("  Cache size: 100");
    println!("  Timeout: 30s");

    println!("\nTo actually serve WASM modules:");
    println!("  1. Create a modules/ directory");
    println!("  2. Add .wasm files (built with cargo-component)");
    println!("  3. Run: mik run modules/");
    println!("\nFor more information, see: mik --help");

    // =========================================================================
    // Step 3: Alternative - Load from Manifest
    // =========================================================================
    //
    // In production, you typically load configuration from mik.toml:
    //
    // ```rust
    // let runtime = Runtime::builder()
    //     .from_manifest_file("mik.toml")?
    //     .build()?;
    // ```
    //
    // The manifest format supports additional options like tracing,
    // load balancing, and dependency management. See CLAUDE.md for details.

    println!("\n=== Example Complete ===");

    Ok(())
}
