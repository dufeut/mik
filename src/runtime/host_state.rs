//! Host state management for WASI HTTP runtime.
//!
//! This module provides the core state types used by the wasmtime runtime:
//! - [`HyperCompatibleBody`]: Wrapper for HTTP body compatibility
//! - [`HostState`]: Per-request WASI/HTTP context and resource limits

use http_body_util::Full;
use hyper::body::Bytes;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use tracing::{debug, warn};
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use crate::runtime::reliability::is_http_host_allowed;

/// Wrapper around `Full<Bytes>` that produces `hyper::Error` (for wasmtime-wasi-http compatibility).
///
/// Since `Full<Bytes>` has `Error = Infallible`, this wrapper maps errors to `hyper::Error`,
/// though in practice no errors will ever occur.
pub(crate) struct HyperCompatibleBody(pub(crate) Full<Bytes>);

impl hyper::body::Body for HyperCompatibleBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        // Full<Bytes> never errors, so we can safely map Infallible to hyper::Error
        match Pin::new(&mut self.0).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
            Poll::Ready(Some(Err(infallible))) => match infallible {},
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.0.size_hint()
    }
}

/// Host state for each request (internal).
pub(crate) struct HostState {
    pub(crate) wasi: WasiCtx,
    pub(crate) http: WasiHttpCtx,
    pub(crate) table: ResourceTable,
    /// Allowed hosts for outgoing HTTP requests (shared reference).
    pub(crate) http_allowed: Arc<Vec<String>>,
    /// Memory limit for this request (bytes).
    pub(crate) memory_limit: usize,
}

/// `ResourceLimiter` implementation to enforce per-request memory limits.
impl wasmtime::ResourceLimiter for HostState {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> anyhow::Result<bool> {
        if desired > self.memory_limit {
            tracing::warn!(
                current_bytes = current,
                desired_bytes = desired,
                limit_bytes = self.memory_limit,
                "WASM memory limit exceeded"
            );
            return Ok(false);
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> anyhow::Result<bool> {
        // Reasonable table size limit (10k entries)
        Ok(desired <= 10_000)
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl WasiHttpView for HostState {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn send_request(
        &mut self,
        request: hyper::Request<wasmtime_wasi_http::body::HyperOutgoingBody>,
        config: wasmtime_wasi_http::types::OutgoingRequestConfig,
    ) -> wasmtime_wasi_http::HttpResult<wasmtime_wasi_http::types::HostFutureIncomingResponse> {
        use wasmtime_wasi_http::bindings::http::types::ErrorCode;

        // If no allowed hosts configured, deny all outgoing requests
        if self.http_allowed.is_empty() {
            warn!("Outgoing HTTP denied: no allowed hosts configured");
            return Err(ErrorCode::HttpRequestDenied.into());
        }

        // Extract host from request
        let host = request.uri().host().unwrap_or("");

        // Check if host is allowed
        if !is_http_host_allowed(host, &self.http_allowed) {
            warn!("Outgoing HTTP denied: host '{}' not in allowed list", host);
            return Err(ErrorCode::HttpRequestDenied.into());
        }

        debug!("Outgoing HTTP allowed: {}", host);

        // Delegate to default implementation
        Ok(wasmtime_wasi_http::types::default_send_request(
            request, config,
        ))
    }
}
