//! Utility functions for process management.
//!
//! This module contains shared utility functions used across
//! the process management subsystem.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Gets the log directory path for mik instances.
///
/// Returns `~/.mik/logs` on all platforms.
pub fn get_log_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;

    Ok(home.join(".mik").join("logs"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_log_dir() {
        let log_dir = get_log_dir().expect("Failed to get log dir");
        assert!(log_dir.ends_with(".mik/logs") || log_dir.ends_with(".mik\\logs"));
    }
}
