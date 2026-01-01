//! Development server command.
//!
//! `mik dev` provides an optimized development experience:
//! - Watch mode: Auto-rebuilds on source changes
//! - Embedded services: KV, SQL, Storage, Cron via daemon
//! - Foreground: Interactive with nice output

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

use crate::daemon::process::{self, SpawnConfig};
use crate::daemon::state::{Instance, StateStore, Status};

/// Execute the dev command.
///
/// Starts a development server with watch mode and optional services.
pub async fn execute(port: u16, no_services: bool) -> Result<()> {
    println!("Starting development server...\n");

    // Start daemon for services (unless disabled)
    if !no_services {
        ensure_daemon_running().await?;
        println!("Services available at http://127.0.0.1:9919");
        println!("  KV:      /kv/:key");
        println!("  SQL:     /sql/query, /sql/execute");
        println!("  Storage: /storage/*path");
        println!("  Cron:    /cron\n");
    }

    // Find mik.toml
    let config_path = std::env::current_dir()?.join("mik.toml");
    if !config_path.exists() {
        anyhow::bail!("No mik.toml found. Run 'mik new' to create a project.");
    }

    let working_dir = std::env::current_dir()?;
    let modules_dir = working_dir.join("modules");

    // Ensure modules directory exists
    if !modules_dir.exists() {
        std::fs::create_dir_all(&modules_dir)?;
    }

    // Get project name from mik.toml
    let name = get_project_name(&config_path).unwrap_or_else(|| "dev".to_string());

    println!("Watching for changes...");
    println!("Server: http://127.0.0.1:{port}");
    println!("Press Ctrl+C to stop\n");

    // Spawn initial instance
    let spawn_config = SpawnConfig {
        name: name.clone(),
        port,
        config_path: config_path.clone(),
        working_dir: working_dir.clone(),
        hot_reload: false,
    };

    let info = process::spawn_instance(&spawn_config)?;
    let current_pid = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(info.pid));

    // Save initial state
    let state_path = get_state_path()?;
    let store = StateStore::open(&state_path)?;
    save_instance_state(&store, &name, port, info.pid, &config_path)?;

    // Start watch loop
    let pid_clone = current_pid.clone();
    let spawn_config_clone = spawn_config.clone();
    let name_clone = name.clone();

    crate::daemon::watch::watch_loop(&modules_dir, &config_path, move |event| {
        handle_watch_event(event, &pid_clone, &name_clone, &spawn_config_clone);
    })
    .await?;

    // Cleanup on exit
    let final_pid = current_pid.load(std::sync::atomic::Ordering::Relaxed);
    if process::is_running(final_pid)? {
        println!("\nStopping server...");
        process::kill_instance(final_pid)?;
    }

    // Update state
    if let Ok(Some(mut instance)) = store.get_instance(&name) {
        instance.status = Status::Stopped;
        let _ = store.save_instance(&instance);
    }

    // Stop daemon if no other instances
    if !no_services {
        let _ = check_and_stop_daemon(&store);
    }

    println!("Development server stopped.");

    Ok(())
}

/// Get project name from mik.toml.
fn get_project_name(config_path: &PathBuf) -> Option<String> {
    let content = std::fs::read_to_string(config_path).ok()?;
    for line in content.lines() {
        if line.trim().starts_with("name") {
            return line
                .split('=')
                .nth(1)
                .map(|s| s.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// Get state database path.
fn get_state_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home.join(".mik").join("state.redb"))
}

/// Save instance state.
fn save_instance_state(
    store: &StateStore,
    name: &str,
    port: u16,
    pid: u32,
    config: &std::path::Path,
) -> Result<()> {
    let instance = Instance {
        name: name.to_string(),
        port,
        pid,
        status: Status::Running,
        config: config.to_path_buf(),
        started_at: chrono::Utc::now(),
        modules: vec![],
        auto_restart: false,
        restart_count: 0,
        last_restart_at: None,
    };
    store.save_instance(&instance)?;
    Ok(())
}

/// Handle watch events by restarting the instance.
fn handle_watch_event(
    event: crate::daemon::watch::WatchEvent,
    pid: &std::sync::Arc<std::sync::atomic::AtomicU32>,
    name: &str,
    spawn_config: &SpawnConfig,
) {
    use crate::daemon::watch::WatchEvent;

    match event {
        WatchEvent::ModuleChanged { path } => {
            println!("[dev] Change detected: {path}");
            restart_instance(pid, name, spawn_config);
        },
        WatchEvent::ConfigChanged => {
            println!("[dev] Config changed, reloading...");
            restart_instance(pid, name, spawn_config);
        },
        WatchEvent::ModuleRemoved { path } => {
            println!("[dev] Module removed: {path}");
        },
        WatchEvent::Error { message } => {
            eprintln!("[dev] Watch error: {message}");
        },
    }
}

/// Restart the instance with new code.
fn restart_instance(
    pid: &std::sync::Arc<std::sync::atomic::AtomicU32>,
    name: &str,
    spawn_config: &SpawnConfig,
) {
    let old_pid = pid.load(std::sync::atomic::Ordering::Relaxed);

    // Kill old process
    if process::is_running(old_pid).unwrap_or(false)
        && let Err(e) = process::kill_instance(old_pid)
    {
        eprintln!("[dev] Failed to stop: {e}");
        return;
    }

    // Spawn new process
    let start = std::time::Instant::now();
    match process::spawn_instance(spawn_config) {
        Ok(info) => {
            pid.store(info.pid, std::sync::atomic::Ordering::Relaxed);
            println!(
                "[dev] Reloaded {} ({}ms)",
                name,
                start.elapsed().as_millis()
            );

            // Update state
            if let Ok(state_path) = get_state_path()
                && let Ok(store) = StateStore::open(&state_path)
            {
                let _ = save_instance_state(
                    &store,
                    name,
                    spawn_config.port,
                    info.pid,
                    &spawn_config.config_path,
                );
            }
        },
        Err(e) => {
            eprintln!("[dev] Failed to restart: {e}");
        },
    }
}

/// Ensure daemon is running.
async fn ensure_daemon_running() -> Result<()> {
    const DAEMON_PORT: u16 = 9919;

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
    let daemon_pid_path = state_path.parent().unwrap().join("daemon.pid");
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

/// Check if daemon is running.
async fn is_daemon_running(port: u16) -> bool {
    reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/health"))
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .is_ok()
}

/// Stop daemon if no instances are running.
fn check_and_stop_daemon(store: &StateStore) -> Result<()> {
    let instances = store.list_instances()?;
    let running = instances
        .iter()
        .filter(|i| i.status == Status::Running && process::is_running(i.pid).unwrap_or(false))
        .count();

    if running == 0
        && let Some(pid) = get_daemon_pid()
    {
        println!("Stopping services daemon...");
        process::kill_instance(pid)?;
    }

    Ok(())
}

/// Get daemon PID.
fn get_daemon_pid() -> Option<u32> {
    let state_path = get_state_path().ok()?;
    let pid_path = state_path.parent()?.join("daemon.pid");
    std::fs::read_to_string(pid_path).ok()?.trim().parse().ok()
}
