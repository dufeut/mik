//! Slow Response Handler - Delays response for testing client disconnect handling
//!
//! This fixture is used for testing client disconnect scenarios (wasmCloud bug #3920).
//! It delays the response for a configurable number of seconds before returning.
//!
//! Request body (JSON):
//! - `delay_secs`: Number of seconds to delay (default: 5)
//! - `response_size_kb`: Size of response body in KB (default: 1)

#[allow(warnings)]
mod bindings;

use bindings::exports::wasi::http::incoming_handler::Guest;
use bindings::wasi::clocks::monotonic_clock;
use bindings::wasi::http::types::{
    Fields, IncomingRequest, OutgoingBody, OutgoingResponse, ResponseOutparam,
};

struct Component;

impl Guest for Component {
    fn handle(request: IncomingRequest, response_out: ResponseOutparam) {
        // Parse request body to get delay configuration
        let body_bytes = read_body(&request);
        let (delay_secs, response_size_kb) = parse_config(&body_bytes);

        // Delay using monotonic clock polling
        let delay_nanos = delay_secs * 1_000_000_000;
        let start = monotonic_clock::now();
        while monotonic_clock::now() - start < delay_nanos {
            // Busy wait - in real WASM this would use pollable
            // For testing purposes, this creates the delay we need
            core::hint::spin_loop();
        }

        // Create response headers
        let headers = Fields::new();
        let _ = headers.append(
            &"content-type".to_string(),
            &b"application/octet-stream".to_vec(),
        );

        // Create response
        let response = OutgoingResponse::new(headers);
        response.set_status_code(200).unwrap();

        let outgoing_body = response.body().unwrap();
        ResponseOutparam::set(response_out, Ok(response));

        // Write response body of specified size
        let response_bytes = vec![b'X'; response_size_kb * 1024];
        let stream = outgoing_body.write().unwrap();
        stream.blocking_write_and_flush(&response_bytes).unwrap();
        drop(stream);
        OutgoingBody::finish(outgoing_body, None).unwrap();
    }
}

fn read_body(request: &IncomingRequest) -> Vec<u8> {
    let incoming_body = match request.consume() {
        Ok(body) => body,
        Err(_) => return Vec::new(),
    };

    let stream = match incoming_body.stream() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut body_bytes = Vec::new();
    loop {
        match stream.blocking_read(65536) {
            Ok(chunk) => {
                if chunk.is_empty() {
                    break;
                }
                body_bytes.extend_from_slice(&chunk);
            }
            Err(_) => break,
        }
    }

    body_bytes
}

fn parse_config(body: &[u8]) -> (u64, usize) {
    // Simple JSON parsing without dependencies
    let text = core::str::from_utf8(body).unwrap_or("{}");

    let delay_secs = extract_number(text, "delay_secs").unwrap_or(5);
    let response_size_kb = extract_number(text, "response_size_kb").unwrap_or(1) as usize;

    (delay_secs, response_size_kb)
}

fn extract_number(json: &str, key: &str) -> Option<u64> {
    // Very simple JSON number extraction
    let key_pattern = format!("\"{}\"", key);
    let pos = json.find(&key_pattern)?;
    let after_key = &json[pos + key_pattern.len()..];

    // Find the colon and then the number
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();

    // Parse the number
    let end = after_colon
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_colon.len());
    after_colon[..end].parse().ok()
}

bindings::export!(Component with_types_in bindings);
