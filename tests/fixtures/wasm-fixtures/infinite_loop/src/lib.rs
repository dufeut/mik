//! Infinite Loop Handler - Loops forever
//!
//! This test fixture is used for testing timeout/epoch interruption
//! when a WASM module enters an infinite loop.

#[allow(warnings)]
mod bindings;

use bindings::exports::wasi::http::incoming_handler::Guest;
use bindings::wasi::http::types::{IncomingRequest, ResponseOutparam};

struct Component;

impl Guest for Component {
    fn handle(_request: IncomingRequest, _response_out: ResponseOutparam) {
        // Infinite loop - should be interrupted by epoch/timeout
        let mut counter: u64 = 0;
        loop {
            counter = counter.wrapping_add(1);
            // Prevent the optimizer from removing the loop
            core::hint::black_box(counter);
        }
    }
}

bindings::export!(Component with_types_in bindings);
