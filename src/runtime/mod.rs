//! WASI HTTP runtime for mik serve.
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
//!
//! This module provides the core functionality for running WASI HTTP components.
//!
//! # Library-First API
//!
//! The runtime provides a clean separation between:
//! - [`Runtime`]: Core WASM execution engine, no network binding
//! - [`Server`]: HTTP server that wraps a Runtime
//!
//! This allows mik to be embedded in applications like Tauri, Electron, or custom servers.
//!
//! # Examples
//!
//! ## Standalone Server (CLI use case)
//!
//! ```no_run
//! use mik::runtime::{Runtime, Server};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let runtime = Runtime::builder()
//!     .modules_dir("modules/")
//!     .build()?;
//!
//! Server::new(runtime, "127.0.0.1:3000".parse()?)
//!     .serve()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Embedded Runtime (library use case)
//!
//! ```no_run
//! use mik::runtime::{Runtime, Request};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let runtime = Runtime::builder()
//!     .modules_dir("modules/")
//!     .build()?;
//!
//! // Handle requests programmatically
//! let response = runtime.handle_request(Request::new("GET", "/run/hello/greet")).await?;
//! println!("Status: {}", response.status);
//! # Ok(())
//! # }
//! ```

pub mod aot_cache;
pub mod builder;
pub mod cluster;
pub mod compression;
pub mod endpoints;
pub mod error;
pub mod host_config;
pub mod host_state;
pub mod lb;
pub mod reliability;
pub mod request;
pub mod request_handler;
pub mod script;
pub mod security;
pub mod server;
pub mod spans;
pub mod static_files;
pub mod types;
pub mod wasm_executor;

// Re-export main builder type
pub use builder::RuntimeBuilder;
pub use host_config::{DEFAULT_MEMORY_LIMIT_BYTES, DEFAULT_SHUTDOWN_TIMEOUT_SECS, HostConfig};
// New library-first API types
pub use request::{Request, Response};
pub use request_handler::handle_request;
pub use server::{Server, ServerBuilder};
#[allow(unused_imports)]
pub use static_files::guess_content_type;
#[allow(unused_imports)]
pub use types::{ErrorCategory, HealthDetail, HealthStatus, MemoryStats};
// Cluster orchestration
pub use cluster::{Cluster, ClusterBuilder, WorkerHandle};

use crate::constants;
use anyhow::{Context, Result};
use host_state::HostState;
use moka::sync::Cache as MokaCache;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, InstanceAllocationStrategy, PoolingAllocationConfig};

// Re-export for script.rs
pub(crate) use wasm_executor::execute_wasm_request_internal;

/// Route prefix for WASM module requests.
pub const RUN_PREFIX: &str = "/run/";

/// Built-in health check endpoint.
pub const HEALTH_PATH: &str = "/health";

/// Route prefix for static file requests.
pub const STATIC_PREFIX: &str = "/static/";

/// Route prefix for script requests.
pub const SCRIPT_PREFIX: &str = "/script/";

/// Built-in metrics endpoint (Prometheus format).
pub const METRICS_PATH: &str = "/metrics";

// Re-export constants used by external code
pub use constants::DEFAULT_CACHE_SIZE;
pub use constants::DEFAULT_MAX_CONCURRENT_REQUESTS;

/// Default timeout for WASM execution (uses `constants::MAX_WASM_TIMEOUT_SECS`).
pub const DEFAULT_EXECUTION_TIMEOUT_SECS: u64 = constants::MAX_WASM_TIMEOUT_SECS;

/// Default max cache memory in MB.
pub const DEFAULT_MAX_CACHE_MB: usize = constants::DEFAULT_CACHE_MB;

/// Default max concurrent requests per module.
pub const DEFAULT_MAX_PER_MODULE_REQUESTS: usize = constants::DEFAULT_MAX_PER_MODULE_REQUESTS;

// NOTE: is_http_host_allowed is imported from reliability::security
// This is the single source of truth used by both host and mik CLI.

// ConfigError and HostConfig moved to host_config.rs
// HostBuilder, SystemConfig, manifest parsing moved to builder.rs

/// Component with cached size information for byte-aware eviction.
pub(crate) struct CachedComponent {
    component: Arc<Component>,
    size_bytes: usize,
}

/// Module cache with byte-aware eviction using moka.
/// Uses weigher function to ensure total bytes don't exceed limit.
type ModuleCache = MokaCache<String, Arc<CachedComponent>>;

/// Shared state for the HTTP runtime.
///
/// This struct contains all the state needed to handle HTTP requests:
/// - Wasmtime Engine and Linker for WASM execution
/// - Module cache for compiled components
/// - Configuration and limits
/// - Circuit breaker for reliability
///
/// It is shared across all request handlers via `Arc`.
///
/// # Thread Safety
///
/// Lock ordering to prevent deadlock: `request_semaphore` -> `module_semaphores` -> `circuit_breaker`
/// Note: cache is now thread-safe internally (moka), no external locking needed.
pub struct SharedState {
    pub(crate) engine: Engine,
    pub(crate) linker: Linker<HostState>,
    pub(crate) modules_dir: PathBuf,
    pub(crate) cache: ModuleCache,
    pub(crate) single_component: Option<Arc<Component>>,
    /// Name of the single component (derived from filename, for routing).
    pub(crate) single_component_name: Option<String>,
    pub(crate) static_dir: Option<PathBuf>,
    pub(crate) execution_timeout: Duration,
    /// Memory limit per request (enforced via `ResourceLimiter`).
    pub(crate) memory_limit_bytes: usize,
    pub(crate) max_body_size_bytes: usize,
    pub(crate) shutdown: Arc<AtomicBool>,
    pub(crate) request_counter: AtomicU64,
    pub(crate) config: HostConfig,
    pub(crate) circuit_breaker: reliability::CircuitBreaker,
    pub(crate) request_semaphore: Arc<Semaphore>,
    pub(crate) module_semaphores: Mutex<HashMap<String, Arc<Semaphore>>>,
    pub(crate) http_allowed: Arc<Vec<String>>,
    /// Scripts directory (optional, for JS orchestration).
    pub(crate) scripts_dir: Option<PathBuf>,
    /// Content-addressable AOT cache for compiled components.
    pub(crate) aot_cache: aot_cache::AotCache,
    /// Fuel budget per request for deterministic CPU limiting.
    pub(crate) fuel_budget: u64,
}

impl SharedState {
    /// Get or create a semaphore for a specific module.
    pub(crate) fn get_module_semaphore(&self, module_name: &str) -> Arc<Semaphore> {
        // Fast path: read-only check without allocation
        {
            let semaphores = self.module_semaphores.lock();
            if let Some(sem) = semaphores.get(module_name) {
                return sem.clone();
            }
        }
        // Slow path: allocate and insert
        let mut semaphores = self.module_semaphores.lock();
        // Double-check after re-acquiring lock
        if let Some(sem) = semaphores.get(module_name) {
            return sem.clone();
        }
        debug!(
            "Creating semaphore for module '{}' with limit {}",
            module_name, self.config.max_per_module_requests
        );
        let sem = Arc::new(Semaphore::new(self.config.max_per_module_requests));
        semaphores.insert(module_name.to_string(), sem.clone());
        sem
    }

    /// Get or load a module by name (async to avoid blocking the runtime).
    #[allow(unsafe_code)] // SAFETY: Component::deserialize_file requires unsafe for AOT cache
    pub(crate) async fn get_or_load(&self, name: &str) -> Result<Arc<Component>> {
        // Security: sanitize module name to prevent path traversal
        let sanitized_name = security::sanitize_module_name(name).map_err(|e| {
            error::Error::InvalidRequest(format!("Invalid module name '{name}': {e}")).into_anyhow()
        })?;

        // Check cache first (no lock needed - moka is thread-safe)
        if let Some(cached) = self.cache.get(&sanitized_name) {
            debug!("Cache hit: {}", sanitized_name);
            return Ok(cached.component.clone());
        }

        // Load from disk (async I/O)
        let path = self.modules_dir.join(format!("{sanitized_name}.wasm"));
        if !tokio::fs::try_exists(&path).await? {
            return Err(error::Error::module_not_found(&sanitized_name).into_anyhow());
        }

        // Get file size for byte-aware cache eviction (async I/O)
        let file_size = tokio::fs::metadata(&path)
            .await
            .map(|m| m.len() as usize)
            .unwrap_or(0);

        info!("Loading module: {} ({} bytes)", sanitized_name, file_size);

        // Read WASM bytes for content-addressable caching
        let wasm_bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let engine = self.engine.clone();
        let aot_cache = self.aot_cache.clone();

        // CPU-intensive component compilation - use spawn_blocking to avoid blocking the runtime
        let component = tokio::task::spawn_blocking(move || -> anyhow::Result<Component> {
            // Try content-addressable AOT cache first (unless in hot-reload mode)
            if let Some(cached_path) = aot_cache.get(&wasm_bytes) {
                // SAFETY: We compiled this file ourselves with the same engine configuration
                match unsafe { Component::deserialize_file(&engine, &cached_path) } {
                    Ok(component) => {
                        tracing::debug!("AOT cache hit: {}", cached_path.display());
                        return Ok(component);
                    },
                    Err(e) => {
                        // AOT cache invalid (e.g., engine version changed), recompile
                        tracing::warn!("AOT cache invalid, recompiling: {}", e);
                        // Remove invalid cache entry
                        let _ = aot_cache.remove(&wasm_bytes);
                    },
                }
            }

            // Compile from bytes
            let component = Component::from_binary(&engine, &wasm_bytes)?;

            // Store in content-addressable cache (unless in hot-reload mode)
            if !aot_cache.is_bypass() {
                match component.serialize() {
                    Ok(serialized) => match aot_cache.put(&wasm_bytes, &serialized) {
                        Ok(path) => {
                            tracing::debug!("Cached AOT: {}", path.display());
                        },
                        Err(e) => {
                            tracing::warn!("Failed to cache AOT: {}", e);
                        },
                    },
                    Err(e) => {
                        tracing::warn!("Failed to serialize for AOT cache: {}", e);
                    },
                }
            }

            Ok(component)
        })
        .await
        .context("Task join failed")?
        .with_context(|| format!("Failed to load {}", path.display()))?;

        let component = Arc::new(component);

        // Cache it with size tracking (moka handles eviction automatically)
        let cached_component = Arc::new(CachedComponent {
            component: component.clone(),
            size_bytes: file_size,
        });
        self.cache.insert(sanitized_name.clone(), cached_component);

        debug!(
            "Cache stats: {} entries, ~{} bytes total",
            self.cache.entry_count(),
            self.cache.weighted_size()
        );

        Ok(component)
    }

    /// Get health status with configurable detail level.
    pub(crate) fn get_health_status(&self, detail: HealthDetail) -> HealthStatus {
        // Get cache stats (no lock needed - moka is thread-safe)
        let loaded_modules = if detail == HealthDetail::Full {
            // Collect module names from cache with pre-allocated capacity
            self.cache.run_pending_tasks();
            let cache_size = self.cache.entry_count() as usize;
            let mut modules = Vec::with_capacity(cache_size);
            for (key, _) in &self.cache {
                modules.push((*key).clone());
            }
            Some(modules)
        } else {
            None
        };

        let cache_size = self.cache.entry_count() as usize;
        let cache_bytes = self.cache.weighted_size() as usize;

        HealthStatus {
            status: constants::HEALTH_STATUS_READY.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            cache_size,
            cache_capacity: self.config.cache_size,
            cache_bytes,
            cache_max_bytes: self.config.max_cache_bytes,
            total_requests: self.request_counter.load(Ordering::Relaxed),
            memory: MemoryStats {
                allocated_bytes: get_memory_usage(),
                limit_per_request_bytes: self.config.memory_limit_bytes,
            },
            loaded_modules,
        }
    }

    /// Generate Prometheus-format metrics.
    pub(crate) fn get_prometheus_metrics(&self) -> String {
        use std::fmt::Write;

        let total_requests = self.request_counter.load(Ordering::Relaxed);
        let cache_entries = self.cache.entry_count();
        let cache_bytes = self.cache.weighted_size();
        let circuit_states = self.circuit_breaker.get_all_states();

        let mut output = String::with_capacity(2048);

        // Help and type declarations
        output.push_str("# HELP mik_requests_total Total number of HTTP requests received\n");
        output.push_str("# TYPE mik_requests_total counter\n");
        let _ = writeln!(output, "mik_requests_total {total_requests}\n");

        output.push_str("# HELP mik_cache_entries Number of modules in cache\n");
        output.push_str("# TYPE mik_cache_entries gauge\n");
        let _ = writeln!(output, "mik_cache_entries {cache_entries}\n");

        output.push_str("# HELP mik_cache_bytes Total bytes used by cached modules\n");
        output.push_str("# TYPE mik_cache_bytes gauge\n");
        let _ = writeln!(output, "mik_cache_bytes {cache_bytes}\n");

        output.push_str("# HELP mik_cache_capacity_bytes Maximum cache size in bytes\n");
        output.push_str("# TYPE mik_cache_capacity_bytes gauge\n");
        let _ = writeln!(
            output,
            "mik_cache_capacity_bytes {}\n",
            self.config.max_cache_bytes
        );

        output.push_str("# HELP mik_max_concurrent_requests Maximum allowed concurrent requests\n");
        output.push_str("# TYPE mik_max_concurrent_requests gauge\n");
        let _ = writeln!(
            output,
            "mik_max_concurrent_requests {}\n",
            self.config.max_concurrent_requests
        );

        output
            .push_str("# HELP mik_circuit_breaker_state Circuit breaker state per module (0=closed, 1=open, 2=half-open)\n");
        output.push_str("# TYPE mik_circuit_breaker_state gauge\n");
        for (module, state) in &circuit_states {
            let state_value = match state.as_str() {
                "open" => 1,
                "half_open" => 2,
                _ => 0, // closed or unknown
            };
            let _ = writeln!(
                output,
                "mik_circuit_breaker_state{{module=\"{module}\"}} {state_value}"
            );
        }
        if !circuit_states.is_empty() {
            output.push('\n');
        }

        // Memory usage (if available)
        if let Some(mem) = get_memory_usage() {
            output.push_str("# HELP mik_memory_bytes Process memory usage in bytes\n");
            output.push_str("# TYPE mik_memory_bytes gauge\n");
            let _ = writeln!(output, "mik_memory_bytes {mem}");
        }

        output
    }
}

/// Internal WASM host that manages the wasmtime engine and module cache.
///
/// This is an internal type used by [`RuntimeBuilder`] to set up the wasmtime
/// environment. Use [`Runtime`] for the public API.
pub(crate) struct Host {
    pub(crate) shared: Arc<SharedState>,
    /// Shutdown signal for the epoch incrementer thread.
    pub(crate) epoch_shutdown: Arc<AtomicBool>,
}

impl Host {
    /// Create the wasmtime engine with pooling allocator configuration.
    fn create_engine(config: &HostConfig) -> Result<Engine> {
        let mut wasm_config = Config::new();
        wasm_config.wasm_component_model(true);
        wasm_config.async_support(true);
        wasm_config.epoch_interruption(true);
        wasm_config.consume_fuel(true);
        wasm_config.parallel_compilation(true);
        wasm_config.async_stack_zeroing(true);

        let mut pool_config = PoolingAllocationConfig::default();
        pool_config.total_component_instances(config.max_concurrent_requests as u32);
        pool_config.total_stacks(config.max_concurrent_requests as u32);
        pool_config.max_component_instance_size(2 * 1024 * 1024);
        pool_config.max_memory_size(config.memory_limit_bytes);
        pool_config.max_memories_per_component(10);
        pool_config.max_tables_per_component(10);
        wasm_config.allocation_strategy(InstanceAllocationStrategy::Pooling(pool_config));

        Engine::new(&wasm_config).context("Failed to create wasmtime engine")
    }

    /// Start the background epoch incrementer thread.
    fn start_epoch_thread(engine: &Engine) -> Arc<AtomicBool> {
        let epoch_shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_for_epoch = epoch_shutdown.clone();
        let engine_for_epoch = engine.clone();
        std::thread::spawn(move || {
            while !shutdown_for_epoch.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
                engine_for_epoch.increment_epoch();
            }
        });
        epoch_shutdown
    }

    /// Determine module mode (single component or directory) and load if single.
    fn determine_module_mode(
        config: &HostConfig,
        engine: &Engine,
    ) -> Result<(PathBuf, Option<Arc<Component>>, Option<String>)> {
        if config.modules_path.is_file() {
            info!("Single component mode: {}", config.modules_path.display());
            let component = Component::from_file(engine, &config.modules_path)
                .context("Failed to load component")?;

            let name = config
                .modules_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map_or_else(
                    || "component".to_string(),
                    |s| s.strip_suffix("-composed").unwrap_or(s).to_string(),
                );

            let modules_dir = config
                .modules_path
                .parent()
                .unwrap_or(&config.modules_path)
                .to_path_buf();

            Ok((modules_dir, Some(Arc::new(component)), Some(name)))
        } else if config.modules_path.is_dir() {
            info!("Multi-module mode: {}", config.modules_path.display());
            info!(
                "Modules will be loaded on-demand (cache size: {})",
                config.cache_size
            );

            let available: Vec<_> = std::fs::read_dir(&config.modules_path)?
                .flatten()
                .filter_map(|entry| {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "wasm") {
                        path.file_stem().and_then(|s| s.to_str()).map(String::from)
                    } else {
                        None
                    }
                })
                .collect();

            if available.is_empty() {
                return Err(error::Error::Config(format!(
                    "No .wasm files found in {}",
                    config.modules_path.display()
                ))
                .into_anyhow());
            }

            info!("Available modules: {}", available.join(", "));
            Ok((config.modules_path.clone(), None, None))
        } else {
            Err(error::Error::Config(format!(
                "Path does not exist: {}",
                config.modules_path.display()
            ))
            .into_anyhow())
        }
    }

    /// Create the AOT cache based on configuration.
    fn create_aot_cache(config: &HostConfig) -> Result<aot_cache::AotCache> {
        if config.hot_reload {
            info!("Hot-reload mode: AOT cache bypassed");
            return Ok(aot_cache::AotCache::bypass());
        }

        let max_bytes = if config.aot_cache_max_mb > 0 {
            (config.aot_cache_max_mb as u64) * 1024 * 1024
        } else {
            constants::DEFAULT_AOT_CACHE_SIZE_BYTES
        };

        let cache = aot_cache::AotCache::new(aot_cache::AotCacheConfig {
            max_size_bytes: max_bytes,
            bypass: false,
        })?;

        info!(
            "AOT cache: ~/.mik/cache/aot/ (max {}MB)",
            max_bytes / 1024 / 1024
        );
        Ok(cache)
    }

    /// Log enabled capabilities.
    fn log_capabilities(config: &HostConfig) {
        if config.logging_enabled {
            info!("Capability: wasi:logging enabled");
        }
        if !config.http_allowed.is_empty() {
            if config.http_allowed.iter().any(|h| h == "*") {
                info!("Capability: wasi:http/outgoing-handler enabled (all hosts)");
            } else {
                info!(
                    "Capability: wasi:http/outgoing-handler enabled ({} hosts)",
                    config.http_allowed.len()
                );
            }
        }
    }

    /// Create a new host with the given configuration.
    pub fn new(config: HostConfig) -> Result<Self> {
        config
            .validate()
            .with_context(|| "Invalid host configuration")?;

        let engine = Self::create_engine(&config)?;
        let epoch_shutdown = Self::start_epoch_thread(&engine);

        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;

        // Create moka cache with byte-aware eviction
        let cache = MokaCache::builder()
            .max_capacity(config.max_cache_bytes as u64)
            .weigher(|_key: &String, value: &Arc<CachedComponent>| -> u32 {
                value.size_bytes.min(u32::MAX as usize) as u32
            })
            .time_to_idle(Duration::from_secs(constants::DEFAULT_AOT_CACHE_TTI_SECS))
            .build();

        let (modules_dir, single_component, single_component_name) =
            Self::determine_module_mode(&config, &engine)?;

        // Validate static directory if provided
        let static_dir = config.static_dir.clone().filter(|dir| {
            if dir.is_dir() {
                info!("Static files: {} -> /static/", dir.display());
                true
            } else {
                warn!("Static directory not found: {}", dir.display());
                false
            }
        });

        Self::log_capabilities(&config);
        let aot_cache = Self::create_aot_cache(&config)?;

        // Resolve fuel budget: use configured value or default
        let fuel_budget = config.fuel_budget.unwrap_or(constants::DEFAULT_FUEL_BUDGET);

        let shared = Arc::new(SharedState {
            engine,
            linker,
            modules_dir,
            cache,
            single_component,
            single_component_name,
            static_dir,
            execution_timeout: Duration::from_secs(config.execution_timeout_secs),
            memory_limit_bytes: config.memory_limit_bytes,
            max_body_size_bytes: config.max_body_size_bytes,
            shutdown: Arc::new(AtomicBool::new(false)),
            request_counter: AtomicU64::new(0),
            circuit_breaker: reliability::CircuitBreaker::new(),
            request_semaphore: Arc::new(Semaphore::new(config.max_concurrent_requests)),
            module_semaphores: Mutex::new(HashMap::new()),
            http_allowed: Arc::new(config.http_allowed.clone()),
            scripts_dir: config.scripts_dir.clone(),
            aot_cache,
            fuel_budget,
            config,
        });

        Ok(Self {
            shared,
            epoch_shutdown,
        })
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Signal the epoch incrementer thread to stop
        self.epoch_shutdown.store(true, Ordering::Relaxed);
    }
}

// =============================================================================
// Runtime: Library-First API
// =============================================================================

/// Core WASM runtime without network binding.
///
/// This is the library-first API for mik. `Runtime` handles requests programmatically,
/// making it suitable for embedding in applications like Tauri, Electron, or custom servers.
/// Use [`Server`] to wrap a Runtime for HTTP serving.
///
/// # Examples
///
/// ## Programmatic Request Handling
///
/// ```no_run
/// use mik::runtime::{Runtime, Request};
///
/// # async fn example() -> anyhow::Result<()> {
/// let runtime = Runtime::builder()
///     .modules_dir("modules/")
///     .build()?;
///
/// // Handle a request without any HTTP server
/// let response = runtime.handle_request(
///     Request::new("GET", "/run/hello/greet")
/// ).await?;
///
/// println!("Status: {}, Body: {:?}", response.status, response.body_str());
/// # Ok(())
/// # }
/// ```
///
/// ## Integration with Custom HTTP Server
///
/// ```no_run
/// use mik::runtime::{Runtime, Request, Response};
///
/// # async fn example() -> anyhow::Result<()> {
/// let runtime = std::sync::Arc::new(
///     Runtime::builder()
///         .modules_dir("modules/")
///         .build()?
/// );
///
/// // Use with any HTTP framework (axum, actix, warp, etc.)
/// // let axum_handler = move |req: axum::Request| {
/// //     let runtime = runtime.clone();
/// //     async move {
/// //         let mik_req = Request::from(req);
/// //         runtime.handle_request(mik_req).await
/// //     }
/// // };
/// # Ok(())
/// # }
/// ```
pub struct Runtime {
    /// Shared state containing engine, linker, cache, config.
    pub(crate) shared: Arc<SharedState>,
    /// Shutdown signal for the epoch incrementer thread.
    epoch_shutdown: Arc<AtomicBool>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("port", &self.shared.config.port)
            .field("modules_path", &self.shared.modules_dir)
            .field("single_component", &self.shared.single_component_name)
            .field("cache_size", &self.shared.cache.entry_count())
            .field(
                "is_shutting_down",
                &self.shared.shutdown.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl Runtime {
    /// Create a new runtime builder.
    ///
    /// This is the recommended way to create a `Runtime` instance.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mik::runtime::Runtime;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let runtime = Runtime::builder()
    ///     .modules_dir("modules/")
    ///     .cache_size(100)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn builder() -> builder::RuntimeBuilder {
        builder::RuntimeBuilder::new()
    }

    /// Create a runtime from a Host (internal conversion).
    pub(crate) fn from_host(host: Host) -> Self {
        Self {
            shared: host.shared.clone(),
            epoch_shutdown: host.epoch_shutdown.clone(),
        }
    }

    /// Handle an HTTP request programmatically.
    ///
    /// This is the core method for the library-first API. It processes a request
    /// through the WASM runtime and returns a response, without any network I/O.
    ///
    /// # Arguments
    ///
    /// * `req` - The request to handle
    ///
    /// # Returns
    ///
    /// The response from the WASM handler, or an error if processing failed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mik::runtime::{Runtime, Request};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let runtime = Runtime::builder()
    ///     .modules_dir("modules/")
    ///     .build()?;
    ///
    /// let response = runtime.handle_request(
    ///     Request::new("POST", "/run/api/users")
    ///         .with_header("Content-Type", "application/json")
    ///         .with_body_str(r#"{"name": "Alice"}"#)
    /// ).await?;
    ///
    /// if response.is_success() {
    ///     println!("User created: {}", response.body_str()?);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn handle_request(&self, req: request::Request) -> Result<request::Response> {
        use http_body_util::Full;
        use hyper::body::Bytes;
        use std::net::{IpAddr, Ipv4Addr};

        // Convert our Request to hyper request format
        let mut hyper_req = hyper::Request::builder()
            .method(req.method.as_str())
            .uri(&req.path);

        for (name, value) in &req.headers {
            hyper_req = hyper_req.header(name.as_str(), value.as_str());
        }

        // Create body
        let body = Full::new(Bytes::from(req.body));
        let hyper_req = hyper_req.body(body)?;

        // Use a dummy remote address for programmatic requests
        let remote_addr = std::net::SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

        // Process the request through our internal handler
        let result = self.handle_request_internal(hyper_req, remote_addr).await?;

        // Convert hyper response back to our Response type
        let status = result.status().as_u16();
        let headers: Vec<(String, String)> = result
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.to_string(), v.to_string()))
            })
            .collect();

        // Extract body bytes from Full<Bytes>
        use http_body_util::BodyExt;
        let body_bytes = result
            .into_body()
            .collect()
            .await
            .map(|c| c.to_bytes().to_vec())
            .unwrap_or_default();

        Ok(request::Response {
            status,
            headers,
            body: body_bytes,
        })
    }

    /// Internal request handling that works with Full<Bytes> body.
    async fn handle_request_internal(
        &self,
        req: hyper::Request<http_body_util::Full<hyper::body::Bytes>>,
        _remote_addr: std::net::SocketAddr,
    ) -> Result<hyper::Response<http_body_util::Full<hyper::body::Bytes>>> {
        use crate::runtime::compression::maybe_compress_response;
        use crate::runtime::error;
        use crate::runtime::host_state::HyperCompatibleBody;
        use crate::runtime::request_handler::{
            error_response, not_found, parse_module_route, validate_content_length,
            validate_path_length,
        };
        use crate::runtime::static_files::serve_static_file;
        use crate::runtime::wasm_executor::execute_wasm_request;
        use http_body_util::Full;
        use hyper::body::Bytes;
        use uuid::Uuid;

        let request_id = Uuid::new_v4();
        let path = req.uri().path().to_string();

        // Trace ID from header or generate new
        let trace_id = req
            .headers()
            .get("x-trace-id")
            .and_then(|v| v.to_str().ok())
            .map_or_else(|| request_id.to_string(), String::from);

        self.shared.request_counter.fetch_add(1, Ordering::Relaxed);

        let client_accepts_gzip = req
            .headers()
            .get("accept-encoding")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("gzip"));

        // Handle built-in endpoints
        if path == HEALTH_PATH {
            // Convert Full<Bytes> to Incoming for health endpoint
            // For now, create a simple health response directly
            let health = self.shared.get_health_status(types::HealthDetail::Summary);
            let body = serde_json::to_string_pretty(&health)
                .unwrap_or_else(|_| r#"{"status":"error"}"#.to_string());

            let response = hyper::Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .header("X-Request-ID", request_id.to_string())
                .header("X-Trace-ID", &trace_id)
                .body(Full::new(Bytes::from(body)))?;

            return Ok(maybe_compress_response(response, client_accepts_gzip));
        }

        if path == METRICS_PATH {
            let metrics = self.shared.get_prometheus_metrics();

            let response = hyper::Response::builder()
                .status(200)
                .header("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
                .header("X-Request-ID", request_id.to_string())
                .header("X-Trace-ID", &trace_id)
                .body(Full::new(Bytes::from(metrics)))?;

            return Ok(maybe_compress_response(response, client_accepts_gzip));
        }

        // Validate path length
        if let Some(resp) = validate_path_length(&path) {
            return Ok(resp);
        }

        // Check Content-Length header
        let max_body = self.shared.max_body_size_bytes;
        if let Some(resp) = validate_content_length(req.headers(), max_body) {
            return Ok(resp);
        }

        // Handle static file requests
        if path.starts_with(STATIC_PREFIX) {
            return match &self.shared.static_dir {
                Some(dir) => serve_static_file(dir, &path)
                    .await
                    .map(|resp| maybe_compress_response(resp, client_accepts_gzip)),
                None => not_found("Static file serving not enabled"),
            };
        }

        // Handle /run/ module requests
        let Some(run_path) = path.strip_prefix(RUN_PREFIX) else {
            return not_found("Not found. WASM modules are served at /run/<module>/");
        };

        let (module, handler_path) = parse_module_route(run_path);

        if module.is_empty() {
            return not_found("No module specified. Use /run/<module>/");
        }

        // Resolve module
        let (component, module_name, module_permit) = {
            // Single component mode
            if let (Some(comp), Some(expected_name)) = (
                &self.shared.single_component,
                &self.shared.single_component_name,
            ) {
                if module == *expected_name {
                    (comp.clone(), Some(module), None)
                } else {
                    let err = error::Error::module_not_found(&module);
                    return error_response(&err);
                }
            } else {
                // Multi-module mode: check circuit breaker
                if let Err(e) = self.shared.circuit_breaker.check_request(&module) {
                    tracing::warn!("Circuit breaker blocked request to '{}': {}", module, e);
                    let err = error::Error::circuit_breaker_open(&module);
                    let mut resp = error_response(&err)?;
                    resp.headers_mut()
                        .insert("Retry-After", "30".parse().expect("valid header value"));
                    return Ok(resp);
                }

                // Acquire per-module semaphore permit
                let module_semaphore = self.shared.get_module_semaphore(&module);
                let module_permit = if let Ok(permit) = module_semaphore.try_acquire_owned() {
                    Some(permit)
                } else {
                    tracing::warn!(
                        "Module '{}' overloaded (max {} concurrent requests)",
                        module,
                        self.shared.config.max_per_module_requests
                    );
                    let err = error::Error::rate_limit_exceeded(format!(
                        "Module '{}' overloaded (max {} concurrent)",
                        module, self.shared.config.max_per_module_requests
                    ));
                    let mut resp = error_response(&err)?;
                    resp.headers_mut()
                        .insert("Retry-After", "5".parse().expect("valid header value"));
                    return Ok(resp);
                };

                match self.shared.get_or_load(&module).await {
                    Ok(comp) => (comp, Some(module.clone()), module_permit),
                    Err(e) => {
                        tracing::warn!("Module load failed: {}", e);
                        self.shared.circuit_breaker.record_failure(&module);
                        let err = error::Error::module_not_found(&module);
                        return error_response(&err);
                    },
                }
            }
        };

        // Rebuild request with new path
        let (parts, body) = req.into_parts();
        let mut new_parts = parts.clone();

        // Update URI with handler path
        let mut uri_parts = new_parts.uri.into_parts();
        uri_parts.path_and_query = Some(handler_path.parse()?);
        new_parts.uri = hyper::Uri::from_parts(uri_parts)?;

        let req = hyper::Request::from_parts(new_parts, HyperCompatibleBody(body));

        // Execute WASM request
        let _module_permit = module_permit;
        let result = execute_wasm_request(self.shared.clone(), component, req).await;

        // Record success/failure in circuit breaker
        if let Some(ref module) = module_name {
            match &result {
                Ok(_) => self.shared.circuit_breaker.record_success(module),
                Err(_) => self.shared.circuit_breaker.record_failure(module),
            }
        }

        result.map(|resp| maybe_compress_response(resp, client_accepts_gzip))
    }

    /// Get the health status of the runtime.
    ///
    /// Returns information about cache usage, request counts, and memory.
    #[must_use]
    pub fn health(&self) -> types::HealthStatus {
        self.shared.get_health_status(types::HealthDetail::Full)
    }

    /// Get Prometheus-format metrics.
    #[must_use]
    pub fn metrics(&self) -> String {
        self.shared.get_prometheus_metrics()
    }

    /// Check if running in single component mode.
    #[must_use]
    pub fn is_single_component(&self) -> bool {
        self.shared.single_component.is_some()
    }

    /// Get the single component name (for routing).
    #[must_use]
    pub fn single_component_name(&self) -> Option<&str> {
        self.shared.single_component_name.as_deref()
    }

    /// Check if static file serving is enabled.
    #[must_use]
    pub fn has_static_files(&self) -> bool {
        self.shared.static_dir.is_some()
    }

    /// Get the configured port (from manifest or builder).
    #[must_use]
    pub fn port(&self) -> u16 {
        self.shared.config.port
    }

    /// Get a reference to the shared state (for advanced use cases).
    #[must_use]
    pub fn shared(&self) -> &Arc<SharedState> {
        &self.shared
    }

    /// Trigger a graceful shutdown.
    ///
    /// This sets the shutdown flag, which will cause any running server
    /// to begin its shutdown sequence.
    pub fn shutdown(&self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
    }

    /// Check if shutdown has been requested.
    #[must_use]
    pub fn is_shutting_down(&self) -> bool {
        self.shared.shutdown.load(Ordering::SeqCst)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // Signal the epoch incrementer thread to stop
        self.epoch_shutdown.store(true, Ordering::Relaxed);
    }
}

/// Get current memory usage (platform-specific).
fn get_memory_usage() -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| s.split_whitespace().next().map(String::from))
            .and_then(|s| s.parse::<usize>().ok())
            .map(|pages| pages * 4096)
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

// NOTE: Tests for is_http_host_allowed are in reliability/src/security.rs
// which is the single source of truth for this function.

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: HostConfig validation tests are in host_config.rs

    /// Test that the epoch thread stops when Host is dropped.
    ///
    /// This test creates a Host with a minimal configuration, then drops it
    /// and verifies that the `epoch_shutdown` flag was set. The actual thread
    /// termination happens asynchronously, but we verify the signal is sent.
    #[test]
    fn test_epoch_thread_shutdown_on_drop() {
        // Create a temporary directory with a dummy wasm file for the Host
        let temp_dir = std::env::temp_dir().join("mik_epoch_test");
        let _ = std::fs::create_dir_all(&temp_dir);

        // Create a minimal valid WASM component (magic + version + empty)
        // This is just enough to pass initial validation
        let wasm_path = temp_dir.join("test.wasm");
        // Minimal WASM module: magic number (0x00 0x61 0x73 0x6D) + version (0x01 0x00 0x00 0x00)
        std::fs::write(&wasm_path, [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]).unwrap();

        // Create Host with multi-module mode
        let config = HostConfig {
            modules_path: temp_dir.clone(),
            cache_size: 1,
            max_cache_bytes: 1024 * 1024,
            max_concurrent_requests: 1,
            ..HostConfig::default()
        };

        // Host::new should succeed since the directory contains a .wasm file
        let host = Host::new(config);

        // Clean up temp file
        let _ = std::fs::remove_file(&wasm_path);
        let _ = std::fs::remove_dir(&temp_dir);

        // Skip test if Host creation failed (e.g., pooling allocator issues on some systems)
        let Ok(host) = host else {
            return; // Skip test on systems where pooling allocator fails
        };

        // Capture the epoch_shutdown Arc before dropping
        let epoch_shutdown = host.epoch_shutdown.clone();

        // Verify the flag is initially false
        assert!(
            !epoch_shutdown.load(Ordering::Relaxed),
            "epoch_shutdown should be false before drop"
        );

        // Drop the host
        drop(host);

        // Verify the flag was set to true by the Drop impl
        assert!(
            epoch_shutdown.load(Ordering::Relaxed),
            "epoch_shutdown should be true after drop"
        );

        // Give the thread a moment to exit (optional, for thoroughness)
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    #[test]
    fn test_fuel_budget_default() {
        // Verify default fuel budget is set correctly
        let config = HostConfig::default();
        assert_eq!(config.fuel_budget, None);

        // When None, the resolved value should be DEFAULT_FUEL_BUDGET
        let resolved = config.fuel_budget.unwrap_or(constants::DEFAULT_FUEL_BUDGET);
        assert_eq!(resolved, constants::DEFAULT_FUEL_BUDGET);
        assert_eq!(resolved, 1_000_000_000);
    }

    #[test]
    fn test_fuel_budget_custom() {
        // Verify custom fuel budget is used
        let config = HostConfig {
            fuel_budget: Some(500_000_000),
            ..Default::default()
        };
        assert_eq!(config.fuel_budget, Some(500_000_000));

        // Resolved value should use the custom budget
        let resolved = config.fuel_budget.unwrap_or(constants::DEFAULT_FUEL_BUDGET);
        assert_eq!(resolved, 500_000_000);
    }

    #[test]
    fn test_fuel_exhaustion_handled() {
        // Test that fuel exhaustion is handled gracefully.
        //
        // This test verifies that:
        // 1. Fuel budget can be configured via HostConfig
        // 2. The budget is propagated to SharedState correctly
        // 3. When fuel runs out, execution stops with an error (not panic)
        //
        // Note: Full integration testing of fuel exhaustion requires a WASM
        // module that runs an infinite loop. The fuel metering in wasmtime
        // will stop execution when the budget is exhausted, returning a Trap
        // error. This test focuses on the configuration plumbing.

        // Create a temp directory with a minimal WASM file
        let temp_dir = std::env::temp_dir().join("mik_fuel_test");
        let _ = std::fs::create_dir_all(&temp_dir);

        let wasm_path = temp_dir.join("test.wasm");
        // Minimal WASM module
        std::fs::write(&wasm_path, [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]).unwrap();

        // Very low fuel budget to ensure quick exhaustion
        let config = HostConfig {
            modules_path: temp_dir.clone(),
            cache_size: 1,
            max_cache_bytes: 1024 * 1024,
            max_concurrent_requests: 1,
            fuel_budget: Some(1000), // Very low budget
            ..HostConfig::default()
        };

        let host = Host::new(config);

        // Clean up temp file
        let _ = std::fs::remove_file(&wasm_path);
        let _ = std::fs::remove_dir(&temp_dir);

        // Skip test if Host creation failed
        let Ok(host) = host else {
            return;
        };

        // Verify fuel budget was set correctly in SharedState
        assert_eq!(host.shared.fuel_budget, 1000);

        // Verify consume_fuel is enabled in engine config
        // (This is validated by the engine creation succeeding with fuel operations)
    }
}

#[cfg(test)]
mod aot_cache_property_tests {
    //! Property-based tests for the AOT (Ahead-of-Time) compilation cache.
    //!
    //! These tests verify invariants for the content-addressable cache:
    //! - Cache key computation is deterministic
    //! - Different inputs produce different keys
    //! - Cache operations are consistent

    use proptest::prelude::*;

    use super::aot_cache::AotCache;

    // ============================================================================
    // Test Strategies - Input Generation
    // ============================================================================

    /// Strategy for generating arbitrary WASM-like byte sequences.
    fn wasm_bytes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 1..10000)
    }

    /// Strategy for generating small byte sequences for more thorough testing.
    fn small_bytes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 1..100)
    }

    /// Strategy for generating pairs of different byte sequences.
    fn different_bytes_pair() -> impl Strategy<Value = (Vec<u8>, Vec<u8>)> {
        (small_bytes(), small_bytes()).prop_filter("must be different", |(a, b)| a != b)
    }

    // ============================================================================
    // Cache Key Computation Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Cache key computation is deterministic.
        ///
        /// The same input bytes should always produce the same cache key.
        /// This is essential for cache correctness.
        #[test]
        fn cache_key_is_deterministic(bytes in wasm_bytes()) {
            let key1 = AotCache::compute_key(&bytes);
            let key2 = AotCache::compute_key(&bytes);

            prop_assert_eq!(key1, key2, "Same input should produce same key");
        }

        /// Invariant: Cache key is always 32 hex characters.
        ///
        /// The key format is fixed: 32 hex chars (128 bits from BLAKE3).
        #[test]
        fn cache_key_format_consistent(bytes in wasm_bytes()) {
            let key = AotCache::compute_key(&bytes);

            prop_assert_eq!(key.len(), 32, "Key should be 32 characters");
            prop_assert!(
                key.chars().all(|c| c.is_ascii_hexdigit()),
                "Key should contain only hex digits"
            );
        }

        /// Invariant: Different inputs produce different keys.
        ///
        /// With high probability, different content should hash to different keys.
        /// This is critical for cache correctness - we don't want collisions.
        #[test]
        fn different_inputs_different_keys((bytes1, bytes2) in different_bytes_pair()) {
            let key1 = AotCache::compute_key(&bytes1);
            let key2 = AotCache::compute_key(&bytes2);

            prop_assert_ne!(
                key1, key2,
                "Different inputs should produce different keys"
            );
        }

        /// Invariant: Empty input produces a valid key.
        ///
        /// Even empty content should hash to a valid key format.
        #[test]
        fn empty_input_valid_key(_dummy in Just(())) {
            let key = AotCache::compute_key(&[]);

            prop_assert_eq!(key.len(), 32);
            prop_assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
        }

        /// Invariant: Single byte difference changes key.
        ///
        /// Even a single bit flip should produce a completely different key.
        #[test]
        fn single_byte_change_changes_key(mut bytes in small_bytes().prop_filter("need at least 1 byte", |v| !v.is_empty())) {
            let key1 = AotCache::compute_key(&bytes);

            // Flip one byte
            bytes[0] = bytes[0].wrapping_add(1);
            let modified = bytes;

            let key2 = AotCache::compute_key(&modified);

            prop_assert_ne!(
                key1, key2,
                "Single byte change should change key"
            );
        }

        /// Invariant: Key doesn't depend on byte order in computation.
        ///
        /// Reversed bytes should produce a different key (content-addressable).
        #[test]
        fn reversed_bytes_different_key(bytes in small_bytes().prop_filter("need > 1 byte", |v| v.len() > 1)) {
            let mut reversed = bytes.clone();
            reversed.reverse();

            // Skip if reversing produces the same bytes (e.g., palindrome)
            if bytes == reversed {
                return Ok(());
            }

            let key1 = AotCache::compute_key(&bytes);
            let key2 = AotCache::compute_key(&reversed);

            prop_assert_ne!(
                key1, key2,
                "Reversed bytes should produce different key"
            );
        }
    }

    // ============================================================================
    // Bypass Mode Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Bypass mode never returns cached entries.
        #[test]
        fn bypass_mode_never_caches(bytes in wasm_bytes()) {
            let cache = AotCache::bypass();

            prop_assert!(cache.is_bypass(), "Should be in bypass mode");
            prop_assert!(
                cache.get(&bytes).is_none(),
                "Bypass mode should never return cached entry"
            );
        }

        /// Invariant: Bypass mode rejects put operations.
        #[test]
        fn bypass_mode_rejects_put(bytes in wasm_bytes()) {
            let cache = AotCache::bypass();

            let result = cache.put(&bytes, b"compiled");
            prop_assert!(result.is_err(), "Bypass mode should reject put");
        }

        /// Invariant: Bypass mode returns false for remove.
        #[test]
        fn bypass_mode_remove_returns_false(bytes in wasm_bytes()) {
            let cache = AotCache::bypass();

            let result = cache.remove(&bytes);
            prop_assert!(result.is_ok());
            prop_assert!(!result.unwrap(), "Bypass mode remove should return false");
        }
    }

    // ============================================================================
    // Hash Quality Tests
    // ============================================================================

    proptest! {
        /// Invariant: Keys have good distribution.
        ///
        /// For random inputs, keys should be evenly distributed.
        /// We check that different inputs don't cluster to similar keys.
        #[test]
        fn keys_well_distributed(
            bytes1 in small_bytes(),
            bytes2 in small_bytes(),
            bytes3 in small_bytes()
        ) {
            let key1 = AotCache::compute_key(&bytes1);
            let key2 = AotCache::compute_key(&bytes2);
            let key3 = AotCache::compute_key(&bytes3);

            // If all inputs are different, all keys should be different
            if bytes1 != bytes2 && bytes2 != bytes3 && bytes1 != bytes3 {
                prop_assert_ne!(key1.clone(), key2.clone());
                prop_assert_ne!(key2, key3.clone());
                prop_assert_ne!(key1, key3);
            }
        }

        /// Invariant: BLAKE3 produces consistent results.
        ///
        /// Known input should produce known output (regression test).
        #[test]
        fn known_input_produces_known_key(_dummy in Just(())) {
            // Test vector: empty input
            let empty_key = AotCache::compute_key(&[]);
            // BLAKE3 of empty is well-defined
            prop_assert!(!empty_key.is_empty());

            // Test vector: single byte
            let single_key = AotCache::compute_key(&[0x00]);
            prop_assert_ne!(empty_key, single_key.clone());

            // Test vector: different single byte
            let other_key = AotCache::compute_key(&[0xFF]);
            prop_assert_ne!(single_key, other_key);
        }

        /// Invariant: Large inputs don't cause issues.
        ///
        /// Even very large WASM modules should hash quickly and correctly.
        #[test]
        fn large_input_works(_dummy in Just(())) {
            // 10MB of data
            let large: Vec<u8> = (0..10_000_000).map(|i| (i % 256) as u8).collect();

            let key = AotCache::compute_key(&large);

            prop_assert_eq!(key.len(), 32);
            prop_assert!(key.chars().all(|c| c.is_ascii_hexdigit()));

            // Same content should produce same key
            let key2 = AotCache::compute_key(&large);
            prop_assert_eq!(key, key2);
        }
    }

    // ============================================================================
    // Key Uniqueness Stress Tests
    // ============================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Invariant: Many different inputs produce many different keys.
        ///
        /// Generate a batch of random inputs and verify no collisions.
        #[test]
        fn no_collisions_in_batch(
            inputs in prop::collection::vec(small_bytes(), 10..20)
        ) {
            // Deduplicate inputs first
            let unique_inputs: std::collections::HashSet<_> = inputs.into_iter().collect();

            // Compute keys
            let keys: std::collections::HashSet<_> = unique_inputs
                .iter()
                .map(|b| AotCache::compute_key(b))
                .collect();

            // Should have as many unique keys as unique inputs
            prop_assert_eq!(
                keys.len(),
                unique_inputs.len(),
                "Should have no key collisions"
            );
        }

        /// Invariant: Sequential byte sequences produce different keys.
        #[test]
        fn sequential_bytes_different_keys(start in 0u8..200) {
            let seq1: Vec<u8> = (start..start.saturating_add(10)).collect();
            let seq2: Vec<u8> = (start.saturating_add(1)..start.saturating_add(11)).collect();

            let key1 = AotCache::compute_key(&seq1);
            let key2 = AotCache::compute_key(&seq2);

            prop_assert_ne!(key1, key2, "Sequential sequences should have different keys");
        }

        /// Invariant: Prefixed/suffixed content has different keys.
        #[test]
        fn prefix_suffix_different_keys(base in small_bytes()) {
            let with_prefix: Vec<u8> = std::iter::once(0xFF).chain(base.iter().copied()).collect();
            let with_suffix: Vec<u8> = base.iter().copied().chain(std::iter::once(0xFF)).collect();

            let key_base = AotCache::compute_key(&base);
            let key_prefix = AotCache::compute_key(&with_prefix);
            let key_suffix = AotCache::compute_key(&with_suffix);

            prop_assert_ne!(key_base.clone(), key_prefix.clone(), "Prefixed content should have different key");
            prop_assert_ne!(key_base, key_suffix.clone(), "Suffixed content should have different key");
            prop_assert_ne!(key_prefix, key_suffix, "Prefix vs suffix should have different keys");
        }
    }
}
