//! HTTP proxy service for forwarding requests to backends.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::{http1, http2};
use hyper_util::rt::TokioExecutor;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::selection::Selection;
use super::{Backend, RoundRobin};

/// Result of backend selection.
enum SelectBackendResult {
    /// A backend was successfully selected.
    Selected(Backend),
    /// No healthy backends are available.
    NoHealthyBackends,
    /// All healthy backends are at connection capacity.
    AllAtCapacity,
}

/// HTTP proxy service that forwards requests to backend servers.
pub struct ProxyService {
    backends: Arc<RwLock<Vec<Backend>>>,
    selection: Arc<RwLock<RoundRobin>>,
    client: reqwest::Client,
    timeout: Duration,
    /// Whether to use HTTP/2 for incoming connections.
    http2_only: bool,
}

impl ProxyService {
    /// Create a new proxy service.
    #[allow(dead_code)]
    pub fn new(
        backends: Arc<RwLock<Vec<Backend>>>,
        selection: Arc<RwLock<RoundRobin>>,
        client: reqwest::Client,
        timeout: Duration,
    ) -> Self {
        Self {
            backends,
            selection,
            client,
            timeout,
            http2_only: false,
        }
    }

    /// Create a new proxy service with HTTP/2 support.
    pub fn with_http2(
        backends: Arc<RwLock<Vec<Backend>>>,
        selection: Arc<RwLock<RoundRobin>>,
        client: reqwest::Client,
        timeout: Duration,
        http2_only: bool,
    ) -> Self {
        Self {
            backends,
            selection,
            client,
            timeout,
            http2_only,
        }
    }

    /// Start serving on the given address.
    pub async fn serve(self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        let protocol = if self.http2_only { "h2" } else { "http/1.1" };
        info!("Proxy service listening on http://{} ({})", addr, protocol);

        let proxy = Arc::new(self);

        loop {
            let (stream, remote_addr) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let proxy = proxy.clone();
            let http2_only = proxy.http2_only;

            tokio::spawn(async move {
                let service = service_fn(move |req| {
                    let proxy = proxy.clone();
                    async move { proxy.handle_request(req, remote_addr).await }
                });

                let result = if http2_only {
                    // Use HTTP/2 with prior knowledge (no TLS/ALPN negotiation)
                    http2::Builder::new(TokioExecutor::new())
                        .serve_connection(io, service)
                        .await
                } else {
                    // Use HTTP/1.1
                    http1::Builder::new()
                        .serve_connection(io, service)
                        .await
                };

                if let Err(e) = result
                    && !e.to_string().contains("connection closed")
                {
                    error!("Connection error: {}", e);
                }
            });
        }
    }

    /// Handle a single request by proxying it to a backend.
    async fn handle_request(
        &self,
        req: Request<Incoming>,
        remote_addr: SocketAddr,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let path = uri.path();

        debug!(
            method = %method,
            path = %path,
            remote = %remote_addr,
            "Received request"
        );

        // Select a healthy backend with available capacity
        let backend = match self.select_backend().await {
            SelectBackendResult::Selected(backend) => backend,
            SelectBackendResult::NoHealthyBackends => {
                warn!("No healthy backends available");
                return Ok(Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(Full::new(Bytes::from("No healthy backends available")))
                    .unwrap());
            }
            SelectBackendResult::AllAtCapacity => {
                warn!("All backends at connection capacity");
                return Ok(Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(Full::new(Bytes::from("All backends at connection capacity")))
                    .unwrap());
            }
        };

        // Track request
        backend.start_request();

        // Forward the request
        let result = self.forward_request(req, &backend).await;

        // End request tracking
        backend.end_request();

        match result {
            Ok(response) => {
                backend.record_success();
                debug!(
                    backend = %backend.address(),
                    status = %response.status(),
                    "Request completed"
                );
                Ok(response)
            },
            Err(e) => {
                backend.record_failure();
                error!(
                    backend = %backend.address(),
                    error = %e,
                    "Request failed"
                );
                Ok(Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(Full::new(Bytes::from(format!("Backend error: {}", e))))
                    .unwrap())
            },
        }
    }

    /// Select a healthy backend with available capacity using the load balancing algorithm.
    ///
    /// Returns `SelectBackendResult::Selected` if a backend is available,
    /// `SelectBackendResult::NoHealthyBackends` if no backends are healthy,
    /// or `SelectBackendResult::AllAtCapacity` if all healthy backends are at their connection limit.
    async fn select_backend(&self) -> SelectBackendResult {
        let backends = self.backends.read().await;
        let selection = self.selection.read().await;

        // Get indices of healthy backends
        let healthy_indices: Vec<usize> = backends
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_healthy())
            .map(|(i, _)| i)
            .collect();

        if healthy_indices.is_empty() {
            return SelectBackendResult::NoHealthyBackends;
        }

        // Get indices of healthy backends with available capacity
        let available_indices: Vec<usize> = healthy_indices
            .iter()
            .copied()
            .filter(|&i| backends[i].has_capacity())
            .collect();

        if available_indices.is_empty() {
            return SelectBackendResult::AllAtCapacity;
        }

        // Select using the algorithm from available backends
        match selection.select(&available_indices) {
            Some(idx) => SelectBackendResult::Selected(backends[idx].clone()),
            None => SelectBackendResult::AllAtCapacity,
        }
    }

    /// Forward a request to the selected backend.
    async fn forward_request(
        &self,
        req: Request<Incoming>,
        backend: &Backend,
    ) -> Result<Response<Full<Bytes>>> {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

        // Build the backend URL
        let backend_url = backend.url(path);

        // Extract headers before consuming the body
        let request_headers: Vec<_> = req
            .headers()
            .iter()
            .filter(|(name, _)| !is_hop_by_hop_header(name.as_str()))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();

        // Collect request body
        let body_bytes = req.collect().await?.to_bytes();

        // Build the proxied request
        let mut request_builder = match method {
            Method::GET => self.client.get(&backend_url),
            Method::POST => self.client.post(&backend_url),
            Method::PUT => self.client.put(&backend_url),
            Method::DELETE => self.client.delete(&backend_url),
            Method::PATCH => self.client.patch(&backend_url),
            Method::HEAD => self.client.head(&backend_url),
            _ => self.client.request(method, &backend_url),
        };

        // Forward headers (skip hop-by-hop headers)
        for (name, value) in request_headers {
            request_builder = request_builder.header(name, value);
        }

        // Add body if not empty
        if !body_bytes.is_empty() {
            request_builder = request_builder.body(body_bytes.to_vec());
        }

        // Set timeout
        request_builder = request_builder.timeout(self.timeout);

        // Send the request
        let response = request_builder.send().await?;

        // Build the response
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await?;

        let mut builder = Response::builder().status(status);

        // Copy response headers (skip hop-by-hop headers)
        for (name, value) in headers.iter() {
            let name_str = name.as_str().to_lowercase();
            if !is_hop_by_hop_header(&name_str) {
                builder = builder.header(name, value);
            }
        }

        Ok(builder.body(Full::new(body))?)
    }
}

/// Check if a header is a hop-by-hop header that should not be forwarded.
fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hop_by_hop_headers() {
        assert!(is_hop_by_hop_header("connection"));
        assert!(is_hop_by_hop_header("keep-alive"));
        assert!(is_hop_by_hop_header("transfer-encoding"));
        assert!(!is_hop_by_hop_header("content-type"));
        assert!(!is_hop_by_hop_header("x-custom-header"));
    }
}
