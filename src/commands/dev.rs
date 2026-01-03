//! Development server command.
//!
//! `mik dev` provides an optimized development experience:
//! - Watch mode: Auto-rebuilds on source changes
//! - Embedded services: KV, SQL, Storage, Cron via daemon
//! - Foreground: Interactive with nice output

use anyhow::Result;
use std::path::PathBuf;

use crate::daemon::paths::{get_daemon_pid, get_state_path};
use crate::daemon::process::{self, SpawnConfig};
use crate::daemon::startup::ensure_daemon_running_for_services;
use crate::daemon::state::{Instance, StateStore, Status};
use crate::manifest::Manifest;

/// Execute the dev command.
///
/// Starts a development server with watch mode and optional services.
pub async fn execute(port: u16, no_services: bool) -> Result<()> {
    println!("Starting development server...\n");

    // Start daemon for services (unless disabled)
    if !no_services {
        ensure_daemon_running_for_services().await?;
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

    // Load watch_debounce_ms from manifest, falling back to default (300ms)
    let debounce_ms = Manifest::load_server_config_from(&config_path)
        .map(|c| c.watch_debounce_ms)
        .unwrap_or(300);

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

    // Start watch loop with configurable debounce
    let pid_clone = current_pid.clone();
    let spawn_config_clone = spawn_config.clone();
    let name_clone = name.clone();
    let mut callback = move |event| {
        handle_watch_event(event, &pid_clone, &name_clone, &spawn_config_clone);
    };

    crate::daemon::watch::watch_loop_with_debounce(
        &modules_dir,
        &config_path,
        debounce_ms,
        &mut callback,
    )
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

/// Save instance state.
fn save_instance_state(
    store: &StateStore,
    name: &str,
    port: u16,
    pid: u32,
    config: &std::path::Path,
) -> Result<()> {
    let instance = Instance::new(name, port, pid, config.to_path_buf());
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
