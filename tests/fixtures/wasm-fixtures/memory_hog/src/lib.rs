//! Memory Hog Handler - Allocates excessive memory
//!
//! This test fixture is used for testing ResourceLimiter when a WASM
//! module attempts to allocate more memory than allowed.

#[allow(warnings)]
mod bindings;

use bindings::exports::wasi::http::incoming_handler::Guest;
use bindings::wasi::http::types::{IncomingRequest, ResponseOutparam};

struct Component;

impl Guest for Component {
    fn handle(_request: IncomingRequest, _response_out: ResponseOutparam) {
        // Try to allocate 1GB of memory
        // This should be stopped by the ResourceLimiter
        let mut allocations: Vec<Vec<u8>> = Vec::new();

        // Allocate in 1MB chunks until we hit the limit
        for _ in 0..1024 {
            let chunk = vec![0u8; 1024 * 1024]; // 1MB
            // Prevent optimization
            core::hint::black_box(&chunk);
            allocations.push(chunk);
        }

        // If we get here, memory limits weren't enforced
        core::hint::black_box(&allocations);
    }
}

bindings::export!(Component with_types_in bindings);
