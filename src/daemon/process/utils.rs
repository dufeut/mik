//! Utility functions for process management.
//!
//! This module contains shared utility functions used across
//! the process management subsystem.

use anyhow::Result;
use std::path::PathBuf;

/// Gets the log directory path for mik instances.
///
/// Returns `~/.mik/logs` on all platforms.
///
/// This delegates to [`super::super::paths::get_logs_dir`].
pub fn get_log_dir() -> Result<PathBuf> {
    crate::daemon::paths::get_logs_dir()
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
