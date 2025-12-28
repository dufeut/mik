# Test Fixtures

This directory contains WASM test fixtures for integration testing.

## Required Fixtures

### For Timeout Tests (`timeout_tests.rs`)

The following WASM modules are needed to run the ignored timeout tests:

#### `modules/infinite_loop.wasm`

A WASM component that contains an infinite loop in the request handler.
Used to test epoch-based interruption.

**How to create:**

```rust
// In a mik_sdk handler project
use mik_sdk::prelude::*;

fn handle(_req: Request) -> Response {
    // Infinite loop - will be interrupted by epoch mechanism
    loop {}
}
```

Build with: `cargo component build --release`

#### `modules/slow_handler.wasm`

A WASM component that takes longer than the timeout to complete.
Used to test timeout behavior with async-style delays.

#### `modules/echo.wasm`

A simple echo handler that returns the request body.
Used to verify normal handlers work correctly with timeouts enabled.

**How to create:**

```rust
use mik_sdk::prelude::*;

fn handle(req: Request) -> Response {
    Response::ok()
        .json(req.body())
}
```

## Running Tests with Fixtures

Once fixtures are in place:

```bash
# Run all tests including those requiring fixtures
cargo test -p mik timeout -- --ignored
```
