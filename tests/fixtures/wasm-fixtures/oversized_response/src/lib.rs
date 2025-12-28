//! Oversized Response Handler - Returns body larger than configured limit
//!
//! This fixture is used for testing Wasmtime bug #12141 where HTTP bodies
//! exceeding the configured limit would cause the server to freeze.
//!
//! Returns a 2MB response body by default (exceeds typical 1MB limit).

#[allow(warnings)]
mod bindings;

use bindings::exports::wasi::http::incoming_handler::Guest;
use bindings::wasi::http::types::{
    Fields, IncomingRequest, OutgoingBody, OutgoingResponse, ResponseOutparam,
};

struct Component;

/// Default response size: 2MB (should exceed most default limits)
const DEFAULT_SIZE_BYTES: usize = 2 * 1024 * 1024;

/// Chunk size for writing response
const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks

impl Guest for Component {
    fn handle(_request: IncomingRequest, response_out: ResponseOutparam) {
        // Create response headers
        let headers = Fields::new();
        let _ = headers.append(
            &"content-type".to_string(),
            &b"application/octet-stream".to_vec(),
        );
        let _ = headers.append(
            &"content-length".to_string(),
            &DEFAULT_SIZE_BYTES.to_string().into_bytes(),
        );

        // Create response
        let response = OutgoingResponse::new(headers);
        response.set_status_code(200).unwrap();

        let outgoing_body = response.body().unwrap();
        ResponseOutparam::set(response_out, Ok(response));

        // Write large response body in chunks
        let stream = outgoing_body.write().unwrap();
        let chunk = vec![b'X'; CHUNK_SIZE];
        let num_chunks = DEFAULT_SIZE_BYTES / CHUNK_SIZE;

        for _ in 0..num_chunks {
            if stream.blocking_write_and_flush(&chunk).is_err() {
                // Client may have disconnected or limit reached
                break;
            }
        }

        drop(stream);
        let _ = OutgoingBody::finish(outgoing_body, None);
    }
}

bindings::export!(Component with_types_in bindings);
