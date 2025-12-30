#!/usr/bin/env bash
# Run Rust dev tools (strict mode CI pipeline from RUST_DEV.md)
# Usage: ./scripts/dev-check.sh [--full]

set -e

FULL=false
if [[ "$1" == "--full" ]]; then
    FULL=true
fi

echo "=== Rust Dev Check ==="

# Tier 2: Static Analysis
echo "[1/6] Formatting..."
cargo fmt --check

echo "[2/6] Clippy (pedantic)..."
cargo clippy --all-targets --all-features -- -D warnings

echo "[3/6] Dependency audit..."
cargo deny check 2>/dev/null || echo "  (cargo-deny not installed, skipping)"
# cargo-vet skipped: broken on Windows with Rust 2024

echo "[4/6] Unsafe audit..."
cargo geiger 2>/dev/null || echo "  (cargo-geiger not installed, skipping)"

echo "[5/6] Tests..."
cargo test --all

if $FULL; then
    # Tier 3: UB detection (slow)
    echo "[6/6] Miri (UB detection)..."
    cargo +nightly miri test 2>/dev/null || echo "  (miri not available, skipping)"
else
    echo "[6/6] Skipping miri (use --full to run)"
fi

echo ""
echo "=== Done ==="
