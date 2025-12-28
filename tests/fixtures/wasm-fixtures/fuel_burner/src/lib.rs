//! Fuel Burner Handler - Performs expensive computation
//!
//! This test fixture is used for testing fuel consumption limits.
//! It performs CPU-intensive operations that should exhaust the fuel budget.

#[allow(warnings)]
mod bindings;

use bindings::exports::wasi::http::incoming_handler::Guest;
use bindings::wasi::http::types::{IncomingRequest, ResponseOutparam};

struct Component;

impl Guest for Component {
    fn handle(_request: IncomingRequest, _response_out: ResponseOutparam) {
        // Perform expensive computation that burns fuel
        let mut result: u64 = 1;

        // Nested loops with expensive operations
        for i in 0..10_000 {
            for j in 0..1_000 {
                // Multiple arithmetic operations per iteration
                result = result.wrapping_mul(i as u64 + 1);
                result = result.wrapping_add(j as u64);
                result = result.wrapping_mul(result);
                result ^= result >> 17;
                result = result.wrapping_mul(0xed5ad4bb);
                result ^= result >> 11;
                result = result.wrapping_mul(0xac4c1b51);
                result ^= result >> 15;
                result = result.wrapping_mul(0x31848bab);
                result ^= result >> 14;
            }
        }

        // Prevent optimization from removing the computation
        core::hint::black_box(result);
    }
}

bindings::export!(Component with_types_in bindings);
