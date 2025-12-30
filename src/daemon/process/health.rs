//! Health checking and log reading for mik daemon instances.
//!
//! This module provides utilities for checking process health status
//! and reading log files.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use sysinfo::{Pid, ProcessesToUpdate, System};

/// Checks if a process with the given PID is currently running.
///
/// Uses sysinfo to query the system's process table and verify the process
/// exists and is active.
///
/// # Arguments
///
/// * `pid` - Process ID to check
///
/// # Returns
///
/// `true` if the process is running, `false` otherwise.
///
/// # Errors
///
/// This function returns `Ok(false)` for non-existent processes rather than
/// erroring, making it safe to use for polling process status.
pub fn is_running(pid: u32) -> Result<bool> {
    let mut system = System::new();

    // Refresh all processes
    system.refresh_processes(ProcessesToUpdate::All, true);

    Ok(system.process(Pid::from(pid as usize)).is_some())
}

/// Reads the last N lines from a log file.
///
/// Useful for implementing `mik logs` command to show recent log entries.
///
/// # Arguments
///
/// * `log_path` - Path to the log file
/// * `lines` - Number of lines to read from the end
///
/// # Returns
///
/// A vector of log lines (most recent first).
pub fn tail_log(log_path: &Path, lines: usize) -> Result<Vec<String>> {
    let file = File::open(log_path)
        .with_context(|| format!("Failed to open log file: {}", log_path.display()))?;

    let reader = BufReader::new(file);
    let all_lines: Vec<String> = reader
        .lines()
        .collect::<std::io::Result<_>>()
        .context("Failed to read log file")?;

    // Take last N lines
    let start = all_lines.len().saturating_sub(lines);
    Ok(all_lines[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_tail_log() {
        let temp_dir = TempDir::new().unwrap();
        let log_file = temp_dir.path().join("test.log");

        let mut file = File::create(&log_file).unwrap();
        writeln!(file, "Line 1").unwrap();
        writeln!(file, "Line 2").unwrap();
        writeln!(file, "Line 3").unwrap();
        writeln!(file, "Line 4").unwrap();
        writeln!(file, "Line 5").unwrap();

        let lines = tail_log(&log_file, 3).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Line 3");
        assert_eq!(lines[1], "Line 4");
        assert_eq!(lines[2], "Line 5");
    }

    #[test]
    fn test_tail_log_more_than_available() {
        let temp_dir = TempDir::new().unwrap();
        let log_file = temp_dir.path().join("test.log");

        let mut file = File::create(&log_file).unwrap();
        writeln!(file, "Line 1").unwrap();
        writeln!(file, "Line 2").unwrap();

        let lines = tail_log(&log_file, 10).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Line 1");
        assert_eq!(lines[1], "Line 2");
    }

    #[test]
    fn test_is_running_current_process() {
        // Current process should always be running
        let current_pid = std::process::id();
        assert!(is_running(current_pid).unwrap());
    }

    #[test]
    fn test_is_running_nonexistent_process() {
        // Very high PID unlikely to exist
        let fake_pid = u32::MAX - 1;
        assert!(!is_running(fake_pid).unwrap());
    }
}
