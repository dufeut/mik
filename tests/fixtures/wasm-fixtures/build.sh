#!/bin/bash
# Build all WASM test fixtures
#
# Prerequisites:
#   cargo install cargo-component
#   rustup target add wasm32-wasip1
#
# Usage:
#   ./build.sh
#
# Output:
#   Built WASM files will be in target/wasm32-wasip1/release/*.wasm
#   and copied to ../modules/

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

echo "Building WASM test fixtures..."

# Build all components in release mode
cargo component build --release --workspace

# Create modules directory if it doesn't exist
mkdir -p ../modules

# Copy built WASM files to modules directory
echo ""
echo "Copying WASM files to modules directory..."

for name in echo panic infinite_loop memory_hog fuel_burner; do
    # cargo-component outputs with underscores replaced by hyphens in package name
    # but the crate name has underscores
    wasm_file="target/wasm32-wasip1/release/${name}.wasm"
    if [ -f "$wasm_file" ]; then
        cp "$wasm_file" "../modules/${name}.wasm"
        echo "  Copied ${name}.wasm"
    else
        echo "  Warning: ${name}.wasm not found"
    fi
done

echo ""
echo "Build complete! WASM files are in:"
echo "  - $(pwd)/target/wasm32-wasip1/release/"
echo "  - $(pwd)/../modules/"
