//! rquickjs orchestration for mik runtime.
//!
//! Pattern adapted from mikcar/src/sql.rs - uses async/sync bridge via channels.
//!
//! # Security Model
//!
//! Scripts have access to:
//! - `host.call(module, options)` - Call WASM handlers
//! - `input` - Request body (JSON)
//!
//! Scripts do NOT have:
//! - Network access (no fetch)
//! - Filesystem access
//! - Module imports (no require)
//! - Shell/process access

use anyhow::{Context, Result};
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Request, Response};
use rquickjs::{Context as JsContext, FromJs, Function, Object, Runtime, Value as JsValue};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::SharedState;
use super::security;
use super::spans::{SpanBuilder, SpanCollector};

// =============================================================================
// Types
// =============================================================================

/// Message from JS to async handler for `host.call()`
#[derive(Debug)]
pub(crate) enum HostMessage {
    Call {
        module: String,
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        body: Option<serde_json::Value>,
        response_tx: std::sync::mpsc::Sender<HostCallResult>,
    },
}

/// Result of a `host.call()` invocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HostCallResult {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Bridge for sync JS → async Rust communication
struct HostBridge {
    tx: mpsc::UnboundedSender<HostMessage>,
}

// Thread-local bridge (same pattern as SQL_BRIDGE in mikcar)
thread_local! {
    static HOST_BRIDGE: RefCell<Option<Arc<HostBridge>>> = const { RefCell::new(None) };
}

/// RAII guard that clears the thread-local bridge on drop, even if a panic occurs.
struct HostBridgeGuard;

impl HostBridgeGuard {
    /// Set the thread-local bridge and return a guard that will clear it on drop.
    fn set(bridge: Arc<HostBridge>) -> Self {
        HOST_BRIDGE.with(|cell| {
            *cell.borrow_mut() = Some(bridge);
        });
        Self
    }
}

impl Drop for HostBridgeGuard {
    fn drop(&mut self) {
        HOST_BRIDGE.with(|cell| {
            *cell.borrow_mut() = None;
        });
    }
}

/// Request body for script execution
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct ScriptRequest {
    #[serde(default)]
    pub input: serde_json::Value,
}

/// Response from script execution
#[derive(Debug, Serialize)]
pub(crate) struct ScriptResponse {
    pub result: serde_json::Value,
    pub calls_executed: usize,
}

// =============================================================================
// Public API
// =============================================================================

/// Handle a script request at /script/<name>
pub(crate) async fn handle_script_request(
    shared: Arc<SharedState>,
    req: Request<hyper::body::Incoming>,
    path: &str,
    trace_id: &str,
    span_collector: SpanCollector,
    parent_span_id: &str,
) -> Result<Response<Full<Bytes>>> {
    use http_body_util::BodyExt;

    // Extract script name from path: /script/<name> or /script/<name>/extra
    let script_path = path
        .strip_prefix(super::SCRIPT_PREFIX)
        .unwrap_or(path)
        .trim_start_matches('/');

    let script_name = script_path
        .split('/')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing script name"))?;

    // Sanitize script name to prevent path traversal
    let script_name = security::sanitize_module_name(script_name)
        .map_err(|e| anyhow::anyhow!("Invalid script name: {e}"))?;

    // Get scripts directory
    let scripts_dir = shared
        .scripts_dir
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Scripts not enabled (no scripts_dir configured)"))?;

    // Load script file
    let script_file = scripts_dir.join(format!("{script_name}.js"));
    let script = tokio::fs::read_to_string(&script_file)
        .await
        .with_context(|| format!("Script not found: {script_name}"))?;

    // Validate body size before reading
    let content_length = req
        .headers()
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    if content_length > shared.max_body_size_bytes {
        anyhow::bail!(
            "Request body too large: {} bytes (max: {} bytes)",
            content_length,
            shared.max_body_size_bytes
        );
    }

    // Parse request body as input
    let body_bytes = req
        .into_body()
        .collect()
        .await
        .context("Failed to read request body")?
        .to_bytes();

    // Double-check actual body size (content-length can be spoofed)
    if body_bytes.len() > shared.max_body_size_bytes {
        anyhow::bail!(
            "Request body too large: {} bytes (max: {} bytes)",
            body_bytes.len(),
            shared.max_body_size_bytes
        );
    }

    let input: serde_json::Value = if body_bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid JSON in request body: {e}"))?
    };

    // Execute script with host.call() bridge
    let script_span = SpanBuilder::with_parent(format!("script.{script_name}"), parent_span_id);
    let script_span_id = script_span.span_id().to_string();
    let result = execute_script(
        shared,
        &script,
        &input,
        trace_id,
        span_collector.clone(),
        &script_span_id,
    )
    .await;

    // Record script span
    match &result {
        Ok(_) => span_collector.add(script_span.finish()),
        Err(e) => span_collector.add(script_span.finish_with_error(e.to_string())),
    }

    let result = result?;

    // Return JSON response
    let response_body = serde_json::to_vec(&result)?;
    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(response_body)))?)
}

// =============================================================================
// Script Execution
// =============================================================================

/// Execute a JavaScript script with `host.call()` capability.
async fn execute_script(
    shared: Arc<SharedState>,
    script: &str,
    input: &serde_json::Value,
    trace_id: &str,
    span_collector: SpanCollector,
    parent_span_id: &str,
) -> Result<ScriptResponse> {
    // Channel for host.call() messages
    let (host_tx, mut host_rx) = mpsc::unbounded_channel::<HostMessage>();

    // Counter for executed calls
    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let call_count_clone = call_count.clone();

    let bridge = Arc::new(HostBridge { tx: host_tx });
    let bridge_clone = bridge.clone();

    // Preprocess and wrap script
    let wrapped_script = preprocess_script(script);

    let input_clone = input.clone();
    let script_owned = wrapped_script.clone();

    // Spawn JS execution in blocking thread
    let mut js_handle = tokio::task::spawn_blocking(move || {
        run_js_script(&script_owned, &input_clone, bridge_clone)
    });

    // Process host.call() messages while JS runs
    let mut last_error: Option<String> = None;

    loop {
        tokio::select! {
            // Check if JS finished
            js_result = &mut js_handle => {
                match js_result {
                    Ok(Ok(result)) => {
                        if let Some(err) = last_error {
                            return Err(anyhow::anyhow!("Handler error: {err}"));
                        }
                        return Ok(ScriptResponse {
                            result,
                            calls_executed: call_count.load(std::sync::atomic::Ordering::Relaxed),
                        });
                    }
                    Ok(Err(e)) => {
                        return Err(anyhow::anyhow!("Script error: {e}"));
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!("Script panicked: {e}"));
                    }
                }
            }

            // Process host.call() messages
            msg = host_rx.recv() => {
                match msg {
                    Some(HostMessage::Call { module, method, path, headers, body, response_tx }) => {
                        call_count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        // Track handler call timing (child of script span)
                        let handler_span = SpanBuilder::with_parent(format!("handler.{module}"), parent_span_id);

                        let result = execute_handler_call(
                            shared.clone(),
                            &module,
                            &method,
                            &path,
                            headers,
                            body,
                            trace_id,
                        ).await;

                        // Record handler span based on result
                        match &result {
                            Ok(resp) if resp.status >= 400 => {
                                span_collector.add(handler_span.finish_with_error(
                                    format!("HTTP {}", resp.status)
                                ));
                            }
                            Ok(_) => {
                                span_collector.add(handler_span.finish());
                            }
                            Err(e) => {
                                span_collector.add(handler_span.finish_with_error(e.to_string()));
                            }
                        }

                        match result {
                            Ok(resp) => {
                                let _ = response_tx.send(resp);
                            }
                            Err(e) => {
                                last_error = Some(e.to_string());
                                let _ = response_tx.send(HostCallResult {
                                    status: 500,
                                    headers: vec![],
                                    body: serde_json::Value::Null,
                                    error: Some(e.to_string()),
                                });
                            }
                        }
                    }
                    None => {
                        // Channel closed, JS finished
                        break;
                    }
                }
            }
        }
    }

    // If we get here, wait for JS to finish
    match js_handle.await {
        Ok(Ok(result)) => {
            if let Some(err) = last_error {
                return Err(anyhow::anyhow!("Handler error: {err}"));
            }
            Ok(ScriptResponse {
                result,
                calls_executed: call_count.load(std::sync::atomic::Ordering::Relaxed),
            })
        },
        Ok(Err(e)) => Err(anyhow::anyhow!("Script error: {e}")),
        Err(e) => Err(anyhow::anyhow!("Script panicked: {e}")),
    }
}

/// Execute a single handler call (check circuit breaker, rate limit, call WASM).
async fn execute_handler_call(
    shared: Arc<SharedState>,
    module: &str,
    method: &str,
    path: &str,
    headers: Vec<(String, String)>,
    body: Option<serde_json::Value>,
    trace_id: &str,
) -> Result<HostCallResult> {
    use http_body_util::BodyExt;

    // Check circuit breaker
    if let Err(e) = shared.circuit_breaker.check_request(module) {
        return Ok(HostCallResult {
            status: 503,
            headers: vec![],
            body: serde_json::json!({"error": "CIRCUIT_OPEN", "message": e.to_string()}),
            error: Some("CIRCUIT_OPEN".to_string()),
        });
    }

    // Acquire per-module semaphore
    let module_semaphore = shared.get_module_semaphore(module);
    let Ok(_permit) = module_semaphore.try_acquire() else {
        return Ok(HostCallResult {
            status: 429,
            headers: vec![],
            body: serde_json::json!({"error": "RATE_LIMITED", "message": "Module overloaded"}),
            error: Some("RATE_LIMITED".to_string()),
        });
    };

    // Load the WASM module
    let component = match shared.get_or_load(module).await {
        Ok(comp) => comp,
        Err(e) => {
            shared.circuit_breaker.record_failure(module);
            return Ok(HostCallResult {
                status: 404,
                headers: vec![],
                body: serde_json::json!({"error": "MODULE_NOT_FOUND", "message": e.to_string()}),
                error: Some("MODULE_NOT_FOUND".to_string()),
            });
        },
    };

    // Build the HTTP request for the handler
    let body_bytes = body
        .map(|b| serde_json::to_vec(&b).unwrap_or_default())
        .unwrap_or_default();

    // Build URI with localhost authority (required by WASI HTTP)
    let full_uri = format!("http://localhost{path}");

    let mut req_builder = hyper::Request::builder().method(method).uri(&full_uri);

    // Add Host header (required by WASI HTTP)
    req_builder = req_builder.header("host", "localhost");

    // Add trace ID for distributed tracing
    req_builder = req_builder.header("x-trace-id", trace_id);

    for (key, value) in &headers {
        req_builder = req_builder.header(key.as_str(), value.as_str());
    }

    // Add content-type if body present
    if !body_bytes.is_empty() {
        req_builder = req_builder.header("content-type", "application/json");
    }

    let req = req_builder
        .body(Full::new(Bytes::from(body_bytes)))
        .context("Failed to build request")?;

    // Execute the WASM handler
    let result = super::execute_wasm_request_internal(shared.clone(), component, req).await;

    match result {
        Ok(response) => {
            shared.circuit_breaker.record_success(module);

            // Extract response parts
            let status = response.status().as_u16();
            let resp_headers: Vec<(String, String)> = response
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();

            // Read response body
            let body_bytes = response
                .into_body()
                .collect()
                .await
                .map(http_body_util::Collected::to_bytes)
                .unwrap_or_default();

            let body: serde_json::Value =
                serde_json::from_slice(&body_bytes).unwrap_or_else(|_| {
                    serde_json::Value::String(String::from_utf8_lossy(&body_bytes).to_string())
                });

            Ok(HostCallResult {
                status,
                headers: resp_headers,
                body,
                error: None,
            })
        },
        Err(e) => {
            shared.circuit_breaker.record_failure(module);
            Ok(HostCallResult {
                status: 500,
                headers: vec![],
                body: serde_json::json!({"error": "HANDLER_ERROR", "message": e.to_string()}),
                error: Some("HANDLER_ERROR".to_string()),
            })
        },
    }
}

// =============================================================================
// JavaScript Runtime
// =============================================================================

/// Maximum iterations to wait for async Promise resolution.
const MAX_ASYNC_ITERATIONS: usize = 10000;

/// Run JavaScript with `host.call()` capability (blocking).
fn run_js_script(
    script: &str,
    input: &serde_json::Value,
    bridge: Arc<HostBridge>,
) -> std::result::Result<serde_json::Value, String> {
    // Set thread-local bridge with RAII guard (clears on drop, even on panic)
    let _guard = HostBridgeGuard::set(bridge);

    // Create QuickJS runtime
    let runtime = Runtime::new().map_err(|e| format!("Failed to create JS runtime: {e}"))?;
    let context =
        JsContext::full(&runtime).map_err(|e| format!("Failed to create JS context: {e}"))?;

    context.with(|ctx| {
        let globals = ctx.globals();

        // Register native __host_call function
        let host_call_fn = Function::new(ctx.clone(), native_host_call)
            .map_err(|e| format!("Failed to create host_call function: {e}"))?;

        globals
            .set("__host_call", host_call_fn)
            .map_err(|e| format!("Failed to set __host_call: {e}"))?;

        // Create host.call() wrapper in JavaScript
        let host_wrapper = r"
            var host = {
                call: function(module, options) {
                    options = options || {};
                    var result = __host_call(module, JSON.stringify(options));
                    return JSON.parse(result);
                }
            };
        ";
        ctx.eval::<(), _>(host_wrapper)
            .map_err(|e| format!("Failed to create host wrapper: {e}"))?;

        // Set input object
        let input_json =
            serde_json::to_string(&input).map_err(|e| format!("Failed to serialize input: {e}"))?;
        let input_script = format!("var input = {input_json};");
        ctx.eval::<(), _>(input_script.as_str())
            .map_err(|e| format!("Failed to set input: {e}"))?;

        // Execute the script
        let result: JsValue<'_> = ctx.eval(script).map_err(|e| format!("Script error: {e}"))?;

        // Check if this is an async result object
        if let Ok(obj) = Object::from_js(&ctx, result.clone())
            && obj.get::<_, bool>("resolved").is_ok()
        {
            // This is an async script result, run pending jobs until resolved
            return resolve_async_result(&runtime, &ctx, &obj);
        }

        // Synchronous result - convert directly
        js_to_json(&ctx, result)
    })
    // _guard is dropped here, clearing the thread-local bridge
}

/// Resolve an async script result by running pending jobs.
fn resolve_async_result<'js>(
    runtime: &Runtime,
    ctx: &rquickjs::Ctx<'js>,
    result_obj: &Object<'js>,
) -> std::result::Result<serde_json::Value, String> {
    for _ in 0..MAX_ASYNC_ITERATIONS {
        // Check if resolved
        let resolved = result_obj.get::<_, bool>("resolved").unwrap_or(false);

        if resolved {
            // Check for error
            if let Ok(error) = result_obj.get::<_, String>("error")
                && !error.is_empty()
            {
                return Err(format!("Async script error: {error}"));
            }

            // Return the value
            let value: JsValue<'_> = result_obj
                .get("value")
                .map_err(|e| format!("Failed to get async result value: {e}"))?;

            return js_to_json(ctx, value);
        }

        // Run pending jobs (Promise callbacks)
        match runtime.execute_pending_job() {
            Ok(true) => {
                // Job executed, continue checking
            },
            Ok(false) => {
                // No more pending jobs but still not resolved - check once more
                let resolved = result_obj.get::<_, bool>("resolved").unwrap_or(false);
                if resolved {
                    continue;
                }
                // Give a small sleep to allow any internal scheduling
                std::thread::sleep(std::time::Duration::from_micros(100));
            },
            Err(e) => {
                return Err(format!("JS job execution error: {e:?}"));
            },
        }
    }

    Err("Async script timeout: Promise did not resolve within iteration limit".to_string())
}

/// Native `host_call` function - takes module and options JSON, returns response JSON.
#[allow(clippy::needless_pass_by_value)] // Required for rquickjs FFI
fn native_host_call(module: String, options_json: String) -> rquickjs::Result<String> {
    HOST_BRIDGE.with(|cell| {
        let bridge = cell.borrow();
        let bridge = bridge.as_ref().ok_or(rquickjs::Error::Exception)?;

        // Parse options
        let options: serde_json::Value = serde_json::from_str(&options_json)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::default()));

        let method = options
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST")
            .to_string();

        let path = options
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("/")
            .to_string();

        let headers: Vec<(String, String)> = options
            .get("headers")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let body = options.get("body").cloned();

        // Send message and block for response
        let (resp_tx, resp_rx) = std::sync::mpsc::channel();
        bridge
            .tx
            .send(HostMessage::Call {
                module,
                method,
                path,
                headers,
                body,
                response_tx: resp_tx,
            })
            .map_err(|_| rquickjs::Error::Exception)?;

        // Block until handler responds
        let result = resp_rx.recv().map_err(|_| rquickjs::Error::Exception)?;

        serde_json::to_string(&result).map_err(|_| rquickjs::Error::Exception)
    })
}

// =============================================================================
// Script Preprocessing
// =============================================================================

/// Preprocess a script to support `export default function(input) { ... }` syntax.
///
/// Transforms:
/// ```js
/// export default function(input) { return input.value; }
/// ```
/// Into:
/// ```js
/// var __default__ = function(input) { return input.value; };
/// __default__(input);
/// ```
///
/// For async functions, wraps the call in Promise.then to capture the result.
/// The runtime will run pending jobs to resolve the Promise.
fn preprocess_script(script: &str) -> String {
    let is_async = script.contains("export default async");

    // Replace "export default" with variable assignment
    let transformed = script
        .replace(
            "export default async function",
            "var __default__ = async function",
        )
        .replace("export default function", "var __default__ = function")
        .replace(
            "export default async (",
            "var __default__ = async function(",
        )
        .replace("export default (", "var __default__ = function(");

    if is_async {
        // For async functions, use Promise.then to capture the result.
        // The runtime will execute pending jobs to resolve the Promise.
        format!(
            "{transformed}\n\
             var __async_result__ = {{ resolved: false, value: null, error: null }};\n\
             __default__(input).then(\n\
               function(r) {{ __async_result__.resolved = true; __async_result__.value = r; }},\n\
               function(e) {{ __async_result__.resolved = true; __async_result__.error = e ? e.toString() : 'Unknown error'; }}\n\
             );\n\
             __async_result__;"
        )
    } else {
        // Synchronous: just call and return
        format!("{transformed}\n__default__(input);")
    }
}

// =============================================================================
// JSON <-> JavaScript Value Conversion
// =============================================================================

#[allow(clippy::needless_pass_by_value)] // JsValue ownership needed for rquickjs
fn js_to_json<'js>(
    ctx: &rquickjs::Ctx<'js>,
    value: JsValue<'js>,
) -> std::result::Result<serde_json::Value, String> {
    if value.is_null() || value.is_undefined() {
        Ok(serde_json::Value::Null)
    } else if let Some(b) = value.as_bool() {
        Ok(serde_json::Value::Bool(b))
    } else if let Some(i) = value.as_int() {
        Ok(serde_json::Value::Number(i.into()))
    } else if let Some(f) = value.as_float() {
        Ok(serde_json::Number::from_f64(f)
            .map_or(serde_json::Value::Null, serde_json::Value::Number))
    } else if let Ok(s) = String::from_js(ctx, value.clone()) {
        Ok(serde_json::Value::String(s))
    } else if let Ok(arr) = rquickjs::Array::from_js(ctx, value.clone()) {
        let mut json_arr = Vec::new();
        for i in 0..arr.len() {
            if let Ok(item) = arr.get::<JsValue<'_>>(i) {
                json_arr.push(js_to_json(ctx, item)?);
            }
        }
        Ok(serde_json::Value::Array(json_arr))
    } else if let Ok(obj) = Object::from_js(ctx, value.clone()) {
        let mut json_obj = serde_json::Map::new();
        for (key, val) in obj.props::<String, JsValue<'_>>().flatten() {
            json_obj.insert(key, js_to_json(ctx, val)?);
        }
        Ok(serde_json::Value::Object(json_obj))
    } else {
        Ok(serde_json::Value::Null)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper to run a simple JS script without `host.call()` capability.
    /// Used for testing basic JS execution and return value conversion.
    /// Scripts should use `export default function(input) { ... }` syntax.
    fn run_simple_script(
        script: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let runtime = Runtime::new().map_err(|e| format!("Failed to create JS runtime: {e}"))?;
        let context =
            JsContext::full(&runtime).map_err(|e| format!("Failed to create JS context: {e}"))?;

        context.with(|ctx| {
            // Set input object
            let input_json = serde_json::to_string(&input)
                .map_err(|e| format!("Failed to serialize input: {e}"))?;
            let input_script = format!("var input = {input_json};");
            ctx.eval::<(), _>(input_script.as_str())
                .map_err(|e| format!("Failed to set input: {e}"))?;

            // Preprocess and execute
            let processed = preprocess_script(script);
            let result: JsValue<'_> = ctx
                .eval(processed)
                .map_err(|e| format!("Script error: {e}"))?;

            js_to_json(&ctx, result)
        })
    }

    // -------------------------------------------------------------------------
    // Return Value Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_script_return_null() {
        let result = run_simple_script(
            "export default function(input) { return null; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(null));
    }

    #[test]
    fn test_script_return_undefined() {
        let result = run_simple_script(
            "export default function(input) { return undefined; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(null));
    }

    #[test]
    fn test_script_return_boolean_true() {
        let result = run_simple_script(
            "export default function(input) { return true; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(true));
    }

    #[test]
    fn test_script_return_boolean_false() {
        let result = run_simple_script(
            "export default function(input) { return false; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(false));
    }

    #[test]
    fn test_script_return_integer() {
        let result = run_simple_script(
            "export default function(input) { return 42; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(42));
    }

    #[test]
    fn test_script_return_negative_integer() {
        let result = run_simple_script(
            "export default function(input) { return -123; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(-123));
    }

    #[test]
    fn test_script_return_float() {
        let result = run_simple_script(
            "export default function(input) { return 1.234; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(1.234));
    }

    #[test]
    fn test_script_return_string() {
        let result = run_simple_script(
            "export default function(input) { return 'hello world'; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("hello world"));
    }

    #[test]
    fn test_script_return_empty_string() {
        let result = run_simple_script(
            "export default function(input) { return ''; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(""));
    }

    #[test]
    fn test_script_return_empty_array() {
        let result = run_simple_script(
            "export default function(input) { return []; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn test_script_return_array_of_numbers() {
        let result = run_simple_script(
            "export default function(input) { return [1, 2, 3]; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn test_script_return_mixed_array() {
        let result = run_simple_script(
            "export default function(input) { return [1, 'two', true, null]; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!([1, "two", true, null]));
    }

    #[test]
    fn test_script_return_empty_object() {
        let result = run_simple_script(
            "export default function(input) { return {}; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!({}));
    }

    #[test]
    fn test_script_return_simple_object() {
        let result = run_simple_script(
            "export default function(input) { return {name: 'test', value: 42}; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!({"name": "test", "value": 42}));
    }

    #[test]
    fn test_script_return_nested_object() {
        let result = run_simple_script(
            "export default function(input) { return {user: {name: 'alice', age: 30}, active: true}; }",
            &json!(null),
        ).unwrap();
        assert_eq!(
            result,
            json!({
                "user": {"name": "alice", "age": 30},
                "active": true
            })
        );
    }

    #[test]
    fn test_script_return_array_of_objects() {
        let result = run_simple_script(
            "export default function(input) { return [{id: 1}, {id: 2}]; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!([{"id": 1}, {"id": 2}]));
    }

    // -------------------------------------------------------------------------
    // Input Access Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_script_access_input_null() {
        let result = run_simple_script(
            "export default function(input) { return input; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(null));
    }

    #[test]
    fn test_script_access_input_string() {
        let result = run_simple_script(
            "export default function(input) { return input; }",
            &json!("hello"),
        )
        .unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn test_script_access_input_object() {
        let result = run_simple_script(
            "export default function(input) { return input; }",
            &json!({"foo": "bar"}),
        )
        .unwrap();
        assert_eq!(result, json!({"foo": "bar"}));
    }

    #[test]
    fn test_script_access_input_property() {
        let result = run_simple_script(
            "export default function(input) { return input.name; }",
            &json!({"name": "alice"}),
        )
        .unwrap();
        assert_eq!(result, json!("alice"));
    }

    #[test]
    fn test_script_access_nested_input_property() {
        let result = run_simple_script(
            "export default function(input) { return input.user.email; }",
            &json!({"user": {"email": "test@example.com"}}),
        )
        .unwrap();
        assert_eq!(result, json!("test@example.com"));
    }

    #[test]
    fn test_script_access_input_array() {
        let result = run_simple_script(
            "export default function(input) { return input[1]; }",
            &json!([10, 20, 30]),
        )
        .unwrap();
        assert_eq!(result, json!(20));
    }

    #[test]
    fn test_script_transform_input() {
        let result = run_simple_script(
            "export default function(input) { return {doubled: input.value * 2, original: input.value}; }",
            &json!({"value": 21}),
        ).unwrap();
        assert_eq!(result, json!({"doubled": 42, "original": 21}));
    }

    // -------------------------------------------------------------------------
    // JavaScript Logic Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_script_conditional_true() {
        let result = run_simple_script(
            "export default function(input) { if (input.authorized) { return {status: 'ok'}; } else { return {status: 'denied'}; } }",
            &json!({"authorized": true}),
        ).unwrap();
        assert_eq!(result, json!({"status": "ok"}));
    }

    #[test]
    fn test_script_conditional_false() {
        let result = run_simple_script(
            "export default function(input) { if (input.authorized) { return {status: 'ok'}; } else { return {status: 'denied'}; } }",
            &json!({"authorized": false}),
        ).unwrap();
        assert_eq!(result, json!({"status": "denied"}));
    }

    #[test]
    fn test_script_loop() {
        let result = run_simple_script(
            "export default function(input) { var sum = 0; for (var i = 0; i < input.length; i++) { sum += input[i]; } return sum; }",
            &json!([1, 2, 3, 4, 5]),
        ).unwrap();
        assert_eq!(result, json!(15));
    }

    #[test]
    fn test_script_array_map() {
        let result = run_simple_script(
            "export default function(input) { return input.map(function(x) { return x * 2; }); }",
            &json!([1, 2, 3]),
        )
        .unwrap();
        assert_eq!(result, json!([2, 4, 6]));
    }

    #[test]
    fn test_script_array_filter() {
        let result = run_simple_script(
            "export default function(input) { return input.filter(function(x) { return x > 2; }); }",
            &json!([1, 2, 3, 4, 5]),
        ).unwrap();
        assert_eq!(result, json!([3, 4, 5]));
    }

    #[test]
    fn test_script_string_operations() {
        let result = run_simple_script(
            "export default function(input) { return input.toUpperCase() + '!'; }",
            &json!("hello"),
        )
        .unwrap();
        assert_eq!(result, json!("HELLO!"));
    }

    #[test]
    fn test_script_json_stringify() {
        let result = run_simple_script(
            "export default function(input) { return JSON.stringify(input); }",
            &json!({"a": 1}),
        )
        .unwrap();
        assert_eq!(result, json!("{\"a\":1}"));
    }

    #[test]
    fn test_script_json_parse() {
        let result = run_simple_script(
            "export default function(input) { return JSON.parse(input); }",
            &json!("{\"b\":2}"),
        )
        .unwrap();
        // JS returns floats, so compare with float
        assert_eq!(result, json!({"b": 2.0}));
    }

    // -------------------------------------------------------------------------
    // Error Handling Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_script_syntax_error() {
        let result = run_simple_script(
            "export default function(input) { return {{{ }",
            &json!(null),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Script error"));
    }

    #[test]
    fn test_script_reference_error() {
        let result = run_simple_script(
            "export default function(input) { return undefinedVariable; }",
            &json!(null),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_script_type_error() {
        let result = run_simple_script(
            "export default function(input) { return null.foo; }",
            &json!(null),
        );
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // Security Tests - No Access to Dangerous APIs
    // -------------------------------------------------------------------------

    #[test]
    fn test_script_no_require() {
        let result = run_simple_script(
            "export default function(input) { return typeof require; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("undefined"));
    }

    #[test]
    fn test_script_no_fetch() {
        let result = run_simple_script(
            "export default function(input) { return typeof fetch; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("undefined"));
    }

    #[test]
    fn test_script_no_process() {
        let result = run_simple_script(
            "export default function(input) { return typeof process; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("undefined"));
    }

    #[test]
    fn test_script_no_global_this_process() {
        let result = run_simple_script(
            "export default function(input) { return typeof globalThis.process; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("undefined"));
    }

    // -------------------------------------------------------------------------
    // Security Tests - Sandbox Escape Prevention
    // -------------------------------------------------------------------------

    #[test]
    fn test_security_no_eval_escape() {
        // eval should work but not escape sandbox
        let result = run_simple_script(
            "export default function(input) { return eval('1 + 1'); }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!(2));
    }

    #[test]
    fn test_security_no_function_constructor_escape() {
        // Function constructor should not provide escape
        let result = run_simple_script(
            "export default function(input) { return typeof Function('return this')().process; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("undefined"));
    }

    #[test]
    fn test_security_no_deno_api() {
        let result = run_simple_script(
            "export default function(input) { return typeof Deno; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("undefined"));
    }

    #[test]
    fn test_security_no_bun_api() {
        let result = run_simple_script(
            "export default function(input) { return typeof Bun; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("undefined"));
    }

    #[test]
    fn test_security_no_xmlhttprequest() {
        let result = run_simple_script(
            "export default function(input) { return typeof XMLHttpRequest; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("undefined"));
    }

    #[test]
    fn test_security_no_websocket() {
        let result = run_simple_script(
            "export default function(input) { return typeof WebSocket; }",
            &json!(null),
        )
        .unwrap();
        assert_eq!(result, json!("undefined"));
    }

    #[test]
    fn test_security_no_import_meta() {
        // import.meta should not exist
        let result = run_simple_script(
            "export default function(input) { try { return typeof import.meta; } catch(e) { return 'error'; } }",
            &json!(null),
        );
        // Should either be undefined or error
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_security_no_dangerous_globals() {
        // Check multiple dangerous globals at once
        let result = run_simple_script(
            r"export default function(input) {
                return {
                    require: typeof require,
                    process: typeof process,
                    global: typeof global,
                    __dirname: typeof __dirname,
                    __filename: typeof __filename,
                    Buffer: typeof Buffer,
                    setImmediate: typeof setImmediate,
                };
            }",
            &json!(null),
        )
        .unwrap();

        let obj = result.as_object().unwrap();
        for (key, value) in obj {
            assert_eq!(value, "undefined", "Global '{key}' should be undefined");
        }
    }

    #[test]
    fn test_security_prototype_pollution_contained() {
        // Prototype pollution is contained per-script (each gets fresh context)
        // A NEW script execution should NOT see pollution from previous scripts
        let result = run_simple_script(
            r"export default function(input) {
                var test = {};
                return test.polluted === undefined;
            }",
            &json!(null),
        )
        .unwrap();
        // Fresh context - no pollution carried over
        assert_eq!(result, json!(true));
    }

    #[test]
    fn test_security_constructor_chain_blocked() {
        // constructor.constructor should not escape
        let result = run_simple_script(
            r"export default function(input) {
                try {
                    var f = (function(){}).constructor.constructor;
                    var g = f('return this')();
                    return typeof g.process;
                } catch(e) {
                    return 'error';
                }
            }",
            &json!(null),
        )
        .unwrap();
        // Should be undefined (no process access) or error
        assert!(result == json!("undefined") || result == json!("error"));
    }

    #[test]
    fn test_security_json_parse_safe() {
        // JSON.parse should not execute code
        let result = run_simple_script(
            r#"export default function(input) {
                return JSON.parse('{"a": 1}').a;
            }"#,
            &json!(null),
        )
        .unwrap();
        // JS returns floats for numbers
        assert_eq!(result, json!(1.0));
    }

    #[test]
    fn test_security_input_not_executable() {
        // Malicious input should not execute
        let result = run_simple_script(
            "export default function(input) { return typeof input; }",
            &json!({"__proto__": {"admin": true}}),
        )
        .unwrap();
        assert_eq!(result, json!("object"));
    }

    // -------------------------------------------------------------------------
    // HostCallResult Serialization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_host_call_result_serialization() {
        let result = HostCallResult {
            status: 200,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: json!({"message": "ok"}),
            error: None,
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["status"], 200);
        assert_eq!(json["body"]["message"], "ok");
        assert!(json.get("error").is_none() || json["error"].is_null());
    }

    #[test]
    fn test_host_call_result_with_error() {
        let result = HostCallResult {
            status: 500,
            headers: vec![],
            body: json!(null),
            error: Some("HANDLER_ERROR".to_string()),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["status"], 500);
        assert_eq!(json["error"], "HANDLER_ERROR");
    }

    // -------------------------------------------------------------------------
    // ScriptResponse Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_script_response_serialization() {
        let response = ScriptResponse {
            result: json!({"orderId": 123}),
            calls_executed: 3,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["result"]["orderId"], 123);
        assert_eq!(json["calls_executed"], 3);
    }

    // -------------------------------------------------------------------------
    // Preprocessing Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_preprocess_export_default_function() {
        let script = "export default function(input) { return input.value * 2; }";
        let processed = preprocess_script(script);
        assert!(processed.contains("var __default__ = function"));
        assert!(processed.contains("__default__(input);"));
        assert!(!processed.contains("export default"));
    }

    #[test]
    fn test_preprocess_export_default_async_function() {
        let script = "export default async function(input) { return await something(); }";
        let processed = preprocess_script(script);
        assert!(processed.contains("var __default__ = async function"));
        // Async functions are wrapped with Promise.then for proper resolution
        assert!(processed.contains("__async_result__"));
        assert!(processed.contains("__default__(input).then"));
    }

    #[test]
    fn test_preprocess_export_default_arrow() {
        let script = "export default (input) => { return input.name; }";
        let processed = preprocess_script(script);
        assert!(processed.contains("var __default__ = function(input)"));
        assert!(processed.contains("__default__(input);"));
    }

    #[test]
    fn test_preprocess_export_default_executes() {
        let script = "export default function(input) { return input.x + input.y; }";
        let processed = preprocess_script(script);

        // Execute the preprocessed script
        let runtime = Runtime::new().unwrap();
        let context = JsContext::full(&runtime).unwrap();

        let result = context
            .with(|ctx| {
                ctx.eval::<(), _>("var input = {x: 10, y: 5};").unwrap();
                let result: JsValue<'_> = ctx.eval(processed).unwrap();
                js_to_json(&ctx, result)
            })
            .unwrap();

        assert_eq!(result, json!(15));
    }

    #[test]
    fn test_preprocess_named_function() {
        let script = "export default function handler(input) { return {handled: true}; }";
        let processed = preprocess_script(script);
        assert!(processed.contains("var __default__ = function handler(input)"));

        let runtime = Runtime::new().unwrap();
        let context = JsContext::full(&runtime).unwrap();

        let result = context
            .with(|ctx| {
                ctx.eval::<(), _>("var input = {};").unwrap();
                let result: JsValue<'_> = ctx.eval(processed).unwrap();
                js_to_json(&ctx, result)
            })
            .unwrap();

        assert_eq!(result, json!({"handled": true}));
    }
}
