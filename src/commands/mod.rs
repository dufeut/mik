//! CLI command implementations for mik.
//!
//! This module contains all the CLI command handlers that power the mik CLI.
//! Each submodule implements a specific command:
//!
//! - [`dev`] - Development server with watch mode and services
//! - [`new`] - Project scaffolding and template generation
//! - [`build`] - WASM component compilation and composition
//! - [`run`] - Production-like server (foreground or detached)
//! - [`daemon`] - Instance management (stop/ps/logs)
//! - [`add`] - Dependency management (OCI/git/path)
//! - [`publish`] - Push components to OCI registries
//! - [`pull`] - Pull components from registries
//! - [`cache`] - AOT cache management
//! - [`strip`] - WASM binary size reduction
//! - [`static_cmd`] - Static file serving configuration

#[cfg(feature = "registry")]
pub mod add;
pub mod build;
pub mod cache;
pub mod daemon;
pub mod dev;
pub mod new;
#[cfg(feature = "registry")]
pub mod publish;
#[cfg(feature = "registry")]
pub mod pull;
pub mod run;
pub mod static_cmd;
pub mod strip;

use anyhow::{Context, Result};
use std::process::Command;

/// Check if a command-line tool is available
pub fn check_tool(name: &str) -> Result<()> {
    let output = Command::new(name).arg("--version").output();

    match output {
        Ok(output) if output.status.success() => Ok(()),
        _ => anyhow::bail!("Required tool '{name}' not found. Please install it to continue."),
    }
}

/// Require a tool to be available, printing install instructions if not found.
///
/// This is a higher-level helper that prints formatted error messages with
/// install instructions before returning an error.
///
/// # Example
///
/// ```ignore
/// require_tool("cargo-component", "cargo install cargo-component")?;
/// require_tool_with_info("wac", "cargo install wac-cli", Some("https://github.com/bytecodealliance/wac"))?;
/// ```
pub fn require_tool(name: &str, install_cmd: &str) -> Result<()> {
    require_tool_with_info(name, install_cmd, None)
}

/// Require a tool with optional additional info URL.
pub fn require_tool_with_info(name: &str, install_cmd: &str, info_url: Option<&str>) -> Result<()> {
    if check_tool(name).is_ok() {
        return Ok(());
    }

    eprintln!("\nError: {name} not found\n");
    eprintln!("{name} is required for this operation.");
    eprintln!("\nInstall with:");
    eprintln!("  {install_cmd}");

    if let Some(url) = info_url {
        eprintln!("\nFor more information, visit:");
        eprintln!("  {url}");
    }

    anyhow::bail!("Missing required tool: {name}")
}

/// Run a command and capture output
#[allow(dead_code)]
pub fn run_command(program: &str, args: &[&str]) -> Result<()> {
    println!("Running: {} {}", program, args.join(" "));

    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("Failed to execute '{program}'"))?;

    if !status.success() {
        anyhow::bail!("Command failed with status: {status}");
    }

    Ok(())
}

/// Get the absolute path to a directory
#[allow(dead_code)]
pub fn get_absolute_path(path: &str) -> Result<String> {
    let path = std::path::Path::new(path);
    let absolute = if path.is_relative() {
        std::env::current_dir()?.join(path)
    } else {
        path.to_path_buf()
    };

    Ok(absolute
        .to_str()
        .context("Path contains invalid UTF-8")?
        .to_string())
}
