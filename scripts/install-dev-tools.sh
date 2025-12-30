#!/usr/bin/env bash
# Install Rust dev tools from RUST_DEV.md
# Run: ./scripts/install-dev-tools.sh

set -e

echo "Installing Rust dev tools..."

# Static analysis tools
cargo install cargo-geiger      # Unsafe surface area audit
cargo install cargo-deny        # License, advisory, duplicate deps
# cargo-vet skipped: broken on Windows with Rust 2024 (winapi c_void mismatch)

# Fuzzing
cargo install cargo-fuzz        # Structure-aware fuzzing

# Performance
cargo install cargo-criterion   # Benchmarks runner
cargo install cargo-llvm-lines  # Codegen analysis
cargo install flamegraph        # Profiling

# Miri (nightly only - UB detection)
rustup +nightly component add miri

echo ""
echo "Done! Tools installed:"
echo "  - cargo-geiger, cargo-deny (audits)"
echo "  - cargo-fuzz (fuzzing)"
echo "  - cargo-criterion, cargo-llvm-lines, flamegraph (perf)"
echo "  - miri (UB detection via: cargo +nightly miri test)"
echo ""
echo "Skipped: cargo-vet (broken on Windows with Rust 2024)"
echo ""
echo "Note: ASan/TSan require nightly + RUSTFLAGS:"
echo '  RUSTFLAGS="-Z sanitizer=address" cargo +nightly test'
echo '  RUSTFLAGS="-Z sanitizer=thread" cargo +nightly test'
