//! Daemon startup and health check utilities.
//!
//! Provides shared functionality for starting and checking the daemon:
//! - `ensure_daemon_running()` - Start daemon if not already running
//! - `is_daemon_running()` - Check if daemon is healthy

use anyhow::{Context, Result};
use std::time::Duration;

use super::paths::{get_daemon_pid_path, get_logs_dir, get_state_path};

/// Default daemon port.
pub const DAEMON_PORT: u16 = 9919;

/// Check if daemon is running by trying to connect to health endpoint.
pub async fn is_daemon_running(port: u16) -> bool {
    reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/health"))
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .is_ok()
}

/// Ensure daemon is running, starting it if necessary.
///
/// Returns `Ok(())` if daemon is already running or was successfully started.
/// Returns `Err` if daemon failed to start within timeout.
pub async fn ensure_daemon_running() -> Result<()> {
    ensure_daemon_running_with_message(true).await
}

/// Ensure daemon is running, with optional startup message.
///
/// When `print_message` is true, prints "Starting daemon..." message.
pub async fn ensure_daemon_running_with_message(print_message: bool) -> Result<()> {
    if is_daemon_running(DAEMON_PORT).await {
        return Ok(());
    }

    if print_message {
        println!("Starting daemon...");
    }

    let mik_exe = std::env::current_exe()?;
    let daemon_log = get_logs_dir()?.join("daemon.log");
    std::fs::create_dir_all(daemon_log.parent().unwrap())?;

    let log_file = std::fs::File::create(&daemon_log)?;

    let child = std::process::Command::new(&mik_exe)
        .args(["daemon", "--port", &DAEMON_PORT.to_string()])
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .spawn()
        .context("Failed to start daemon")?;

    // Save daemon PID
    let daemon_pid_path = get_daemon_pid_path()?;
    std::fs::write(&daemon_pid_path, child.id().to_string())?;

    // Wait for daemon to be ready (max 5 seconds)
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if is_daemon_running(DAEMON_PORT).await {
            if print_message {
                println!("Daemon started (PID: {})", child.id());
            }
            return Ok(());
        }
    }

    anyhow::bail!("Daemon failed to start within 5 seconds")
}

/// Ensure daemon is running for services (prints services message).
///
/// Used by `mik dev` to start daemon with services information.
pub async fn ensure_daemon_running_for_services() -> Result<()> {
    if is_daemon_running(DAEMON_PORT).await {
        return Ok(());
    }

    println!("Starting services daemon...");

    let mik_exe = std::env::current_exe()?;
    let state_path = get_state_path()?;
    let daemon_log = state_path.parent().unwrap().join("logs").join("daemon.log");
    std::fs::create_dir_all(daemon_log.parent().unwrap())?;

    let log_file = std::fs::File::create(&daemon_log)?;

    let child = std::process::Command::new(&mik_exe)
        .args(["daemon", "--port", &DAEMON_PORT.to_string()])
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .spawn()
        .context("Failed to start daemon")?;

    // Save daemon PID
    let daemon_pid_path = get_daemon_pid_path()?;
    std::fs::write(&daemon_pid_path, child.id().to_string())?;

    // Wait for ready
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if is_daemon_running(DAEMON_PORT).await {
            return Ok(());
        }
    }

    anyhow::bail!("Daemon failed to start")
}
