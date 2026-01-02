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

/// Get the mik base directory: `~/.mik/`
///
/// This is the root directory for all mik data, configuration, and tools.
pub fn get_mik_dir() -> Result<PathBuf> {
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
