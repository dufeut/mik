//! WASM execution for the runtime.
//!
//! This module provides the core WASM execution functions:
//! - [`execute_wasm_request`]: Execute a WASM HTTP handler
//! - [`execute_wasm_request_internal`]: Public API for script orchestration

use crate::runtime::SharedState;
use crate::runtime::host_state::{HostState, HyperCompatibleBody};
use anyhow::{Context, Result};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Request, Response};
use std::sync::Arc;
use wasmtime::Store;
use wasmtime::component::{Component, ResourceTable};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi_http::bindings::http::types::Scheme;
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

/// Execute a WASM request with a Full<Bytes> body (for script orchestration).
pub(crate) async fn execute_wasm_request_internal(
    shared: Arc<SharedState>,
    component: Arc<Component>,
    req: Request<Full<Bytes>>,
) -> Result<Response<Full<Bytes>>> {
    let (parts, body) = req.into_parts();
    let req = Request::from_parts(parts, HyperCompatibleBody(body));
    execute_wasm_request(shared, component, req).await
}

/// Execute a WASM request (internal helper).
///
/// Body is pre-collected with size limits already enforced.
pub(crate) async fn execute_wasm_request(
    shared: Arc<SharedState>,
    component: Arc<Component>,
    req: Request<HyperCompatibleBody>,
) -> Result<Response<Full<Bytes>>> {
    // Create fresh WASI context
    let wasi = WasiCtxBuilder::new().inherit_stdio().inherit_env().build();

    // Use pre-computed Arc (cheap pointer copy instead of cloning Vec)
    let http_allowed = shared.http_allowed.clone();

    let state = HostState {
        wasi,
        http: WasiHttpCtx::new(),
        table: ResourceTable::new(),
        http_allowed,
        memory_limit: shared.memory_limit_bytes,
    };

    let mut store = Store::new(&shared.engine, state);

    // Enable ResourceLimiter for memory enforcement
    store.limiter(|state| state);

    // Configure epoch deadline for async yielding (100 epochs/second, so timeout_secs * 100)
    // Using epoch_deadline_async_yield_and_update instead of set_epoch_deadline because:
    // 1. On shutdown, the epoch incrementer thread stops, causing WASM to hit its deadline
    // 2. With async yielding, WASM will yield (return Pending) instead of trapping
    // 3. The tokio::time::timeout wrapper will then cancel the execution gracefully
    // This provides cooperative cancellation during shutdown rather than abrupt traps.
    let timeout_epochs = shared.execution_timeout.as_secs().saturating_mul(100);
    store.epoch_deadline_async_yield_and_update(timeout_epochs);

    // Set fuel budget for deterministic CPU limiting
    // Fuel provides deterministic limits complementing epoch-based preemption
    store.set_fuel(shared.fuel_budget)?;

    // Create response channel
    let (sender, receiver) = tokio::sync::oneshot::channel();

    // Create request/response resources
    let req_resource = store.data_mut().new_incoming_request(Scheme::Http, req)?;
    let out_resource = store.data_mut().new_response_outparam(sender)?;

    // Instantiate and call handler with timeout enforcement
    let timeout = shared.execution_timeout;

    let proxy = tokio::time::timeout(
        timeout,
        wasmtime_wasi_http::bindings::Proxy::instantiate_async(
            &mut store,
            &component,
            &shared.linker,
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("WASM instantiation timed out after {timeout:?}"))?
    .context("Failed to instantiate proxy")?;

    tokio::time::timeout(timeout, async {
        proxy
            .wasi_http_incoming_handler()
            .call_handle(&mut store, req_resource, out_resource)
            .await
    })
    .await
    .map_err(|_| anyhow::anyhow!("WASM execution timed out after {timeout:?}"))?
    .context("Handler call failed")?;

    // Get response
    let response = receiver
        .await
        .context("No response received")?
        .context("Response error")?;

    // Convert to hyper response
    let mut builder = Response::builder().status(response.status());

    for (name, value) in response.headers() {
        builder = builder.header(name, value);
    }

    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map(http_body_util::Collected::to_bytes)
        .unwrap_or_default();

    Ok(builder.body(Full::new(body_bytes))?)
}
