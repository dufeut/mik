//! Path utilities for mik infrastructure.
//!
//! Provides centralized path resolution for all mik-related files:
//!
//! # Base Directories
//! - [`get_mik_dir`] - `~/.mik/` (base directory for all mik data)
//! - [`get_tools_dir`] - `~/.mik/tools/` (downloaded tools like bridge)
//!
//! # Daemon Files
//! - [`get_state_path`] - `~/.mik/state.redb` (instance state database)
//! - [`get_daemon_pid_path`] - `~/.mik/daemon.pid` (daemon process ID)
//! - [`get_logs_dir`] - `~/.mik/logs/` (instance logs)
//! - [`get_log_path`] - `~/.mik/logs/<name>.log` (specific instance log)
//!
//! # Configuration
//! - [`get_daemon_config_path`] - `~/.mik/daemon.toml` (daemon settings)

use anyhow::{Context, Result};
use std::path::PathBuf;

// =============================================================================
// Base Directories
// =============================================================================

/// Get the mik base directory.
///
/// Resolution order:
/// 1. `MIK_HOME` environment variable (if set)
/// 2. `~/.mik/` (default)
///
/// This is the root directory for all mik data, configuration, and tools.
/// CI/CD systems can override the location by setting `MIK_HOME`.
pub fn get_mik_dir() -> Result<PathBuf> {
    // Check for MIK_HOME environment variable first
    if let Ok(mik_home) = std::env::var("MIK_HOME")
        && !mik_home.is_empty()
    {
        return Ok(PathBuf::from(mik_home));
    }

    // Fall back to ~/.mik/
    let home = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home.join(".mik"))
}

/// Get the tools directory: `~/.mik/tools/`
///
/// Used for downloaded tools like the bridge component.
pub fn get_tools_dir() -> Result<PathBuf> {
    Ok(get_mik_dir()?.join("tools"))
}

// =============================================================================
// Daemon Files
// =============================================================================

/// Get the state database path: `~/.mik/state.redb`
pub fn get_state_path() -> Result<PathBuf> {
    Ok(get_mik_dir()?.join("state.redb"))
}

/// Get the daemon PID file path: `~/.mik/daemon.pid`
pub fn get_daemon_pid_path() -> Result<PathBuf> {
    Ok(get_mik_dir()?.join("daemon.pid"))
}

/// Get the daemon log directory path: `~/.mik/logs/`
pub fn get_logs_dir() -> Result<PathBuf> {
    Ok(get_mik_dir()?.join("logs"))
}

/// Get log path for a specific instance: `~/.mik/logs/<name>.log`
pub fn get_log_path(name: &str) -> Result<PathBuf> {
    Ok(get_logs_dir()?.join(format!("{name}.log")))
}

// =============================================================================
// Configuration Files
// =============================================================================

/// Get the daemon config path: `~/.mik/daemon.toml`
pub fn get_daemon_config_path() -> Result<PathBuf> {
    Ok(get_mik_dir()?.join("daemon.toml"))
}

/// Get the cache directory: `~/.mik/cache/`
pub fn get_cache_dir() -> Result<PathBuf> {
    Ok(get_mik_dir()?.join("cache"))
}

/// Get the bin directory: `~/.mik/bin/`
///
/// Used for locally installed tools.
pub fn get_bin_dir() -> Result<PathBuf> {
    Ok(get_mik_dir()?.join("bin"))
}

/// Get daemon PID if running.
///
/// Returns `None` if the PID file doesn't exist or can't be parsed.
pub fn get_daemon_pid() -> Option<u32> {
    get_daemon_pid_path()
        .ok()?
        .exists()
        .then(|| {
            std::fs::read_to_string(get_daemon_pid_path().ok()?)
                .ok()?
                .trim()
                .parse()
                .ok()
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Tests for MIK_HOME environment variable support are not included
    // because Rust 2024 requires unsafe blocks for std::env::set_var/remove_var,
    // and this crate uses #![deny(unsafe_code)].
    //
    // The MIK_HOME functionality can be tested manually or via integration tests
    // that set environment variables before spawning the test process.

    #[test]
    fn test_derived_paths_structure() {
        // Test that derived paths are correctly structured relative to base
        // (without modifying environment variables)
        let home = dirs::home_dir().expect("home directory should exist");
        let expected_base = home.join(".mik");

        // When MIK_HOME is not set, all paths should be under ~/.mik/
        // We test the path structure without modifying env vars
        if std::env::var("MIK_HOME").is_err() {
            let mik_dir = get_mik_dir().unwrap();
            assert_eq!(mik_dir, expected_base);

            // Verify all derived paths are children of mik_dir
            assert!(get_tools_dir().unwrap().starts_with(&mik_dir));
            assert!(get_state_path().unwrap().starts_with(&mik_dir));
            assert!(get_daemon_pid_path().unwrap().starts_with(&mik_dir));
            assert!(get_logs_dir().unwrap().starts_with(&mik_dir));
            assert!(get_log_path("test").unwrap().starts_with(&mik_dir));
            assert!(get_daemon_config_path().unwrap().starts_with(&mik_dir));
            assert!(get_cache_dir().unwrap().starts_with(&mik_dir));
            assert!(get_bin_dir().unwrap().starts_with(&mik_dir));
        }
    }

    #[test]
    fn test_log_path_format() {
        // Test that log paths are correctly formatted
        let log_path = get_log_path("my-service").unwrap();
        assert!(log_path.to_string_lossy().ends_with("my-service.log"));
    }

    #[test]
    fn test_path_extensions() {
        // Verify expected file extensions
        let state_path = get_state_path().unwrap();
        assert_eq!(
            state_path.extension().and_then(|e| e.to_str()),
            Some("redb")
        );

        let pid_path = get_daemon_pid_path().unwrap();
        assert_eq!(pid_path.extension().and_then(|e| e.to_str()), Some("pid"));

        let config_path = get_daemon_config_path().unwrap();
        assert_eq!(
            config_path.extension().and_then(|e| e.to_str()),
            Some("toml")
        );
    }
}
