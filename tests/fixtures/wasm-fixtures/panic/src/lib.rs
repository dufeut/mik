//! Panic Handler - Always panics when invoked
//!
//! This test fixture is used for testing resilience and error handling
//! when a WASM module panics during execution.

#[allow(warnings)]
mod bindings;

use bindings::exports::wasi::http::incoming_handler::Guest;
use bindings::wasi::http::types::{IncomingRequest, ResponseOutparam};

struct Component;

impl Guest for Component {
    fn handle(_request: IncomingRequest, _response_out: ResponseOutparam) {
        // Always panic to test error handling
        panic!("Intentional panic for testing error handling");
    }
}

bindings::export!(Component with_types_in bindings);
