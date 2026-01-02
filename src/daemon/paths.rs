//! Path utilities for daemon infrastructure.
//!
//! Provides centralized path resolution for daemon-related files:
//! - State database (`~/.mik/state.redb`)
//! - Daemon PID file (`~/.mik/daemon.pid`)
//! - Daemon logs (`~/.mik/logs/`)

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Get the state database path: `~/.mik/state.redb`
pub fn get_state_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home.join(".mik").join("state.redb"))
}

/// Get the daemon PID file path: `~/.mik/daemon.pid`
pub fn get_daemon_pid_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home.join(".mik").join("daemon.pid"))
}

/// Get the daemon log directory path: `~/.mik/logs/`
pub fn get_logs_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home.join(".mik").join("logs"))
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
