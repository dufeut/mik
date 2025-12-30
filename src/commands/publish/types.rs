//! Types and constants for the publish command.

/// Common WASM build locations.
pub const WASM_PATTERNS: [&str; 3] = [
    "target/wasm32-wasip2/release/*.wasm",
    "target/wasm32-wasip1/release/*.wasm",
    "target/composed.wasm",
];

/// Static asset directory candidates.
pub const STATIC_DIRS: [&str; 6] = [
    "static",
    "dist",
    "build",
    "public",
    "frontend/dist",
    "web/dist",
];

/// WIT directory candidates.
pub const WIT_DIRS: [&str; 3] = ["wit", "WIT", "Wit"];
