//! Integration test runner for mikrozen-host.
//!
//! This file serves as the entry point for the integration test suite.
//! The actual tests are organized in the `integration/` subdirectory.
//!
//! Run tests with:
//! ```bash
//! cargo test --test integration
//! ```
//!
//! Run ignored tests (that require WASM modules):
//! ```bash
//! cargo test --test integration -- --ignored
//! ```

#[path = "common.rs"]
mod common;

pub use common::RealTestHost;
pub use common::TestHost;

// Bring in the HTTP tests
#[path = "http_tests.rs"]
mod http_tests;

// Bring in the script orchestration tests
#[path = "script_tests.rs"]
mod script_tests;

// Bring in the WASM integration tests (uses RealTestHost)
#[path = "wasm_tests.rs"]
mod wasm_tests;
