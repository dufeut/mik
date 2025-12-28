//! Echo Handler - Echoes back the request body
//!
//! This is a minimal test fixture that returns the request body as the response.

#[allow(warnings)]
mod bindings;

use bindings::exports::wasi::http::incoming_handler::Guest;
use bindings::wasi::http::types::{
    Fields, IncomingRequest, OutgoingBody, OutgoingResponse, ResponseOutparam,
};

struct Component;

impl Guest for Component {
    fn handle(request: IncomingRequest, response_out: ResponseOutparam) {
        // Read the request body
        let body_bytes = read_body(&request);

        // Create response headers
        let headers = Fields::new();
        let _ = headers.append(&"content-type".to_string(), &b"application/octet-stream".to_vec());

        // Create response
        let response = OutgoingResponse::new(headers);
        response.set_status_code(200).unwrap();

        let outgoing_body = response.body().unwrap();
        ResponseOutparam::set(response_out, Ok(response));

        // Write the echo body
        let stream = outgoing_body.write().unwrap();
        stream.blocking_write_and_flush(&body_bytes).unwrap();
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

bindings::export!(Component with_types_in bindings);
