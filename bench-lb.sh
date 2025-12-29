#!/bin/bash
# Benchmark: 1 worker vs auto workers (workers=0)
#
# Usage (from WSL):
#   cd /mnt/c/Users/el_to/OneDrive/Documentos/Github/mik
#   chmod +x bench-lb.sh
#   ./bench-lb.sh
#
# Requires: wrk or oha installed in WSL
#   sudo apt install wrk  # or
#   cargo install oha

set -e

COMPONENT="examples/hello-world/dist/hello-world-composed.wasm"
PORT=3000
DURATION=10
CONNECTIONS=100
THREADS=4

# Detect benchmark tool
if command -v oha &> /dev/null; then
    BENCH_TOOL="oha"
elif command -v wrk &> /dev/null; then
    BENCH_TOOL="wrk"
else
    echo "Error: Neither 'oha' nor 'wrk' found. Install one of them:"
    echo "  sudo apt install wrk"
    echo "  cargo install oha"
    exit 1
fi

echo "Using benchmark tool: $BENCH_TOOL"
echo "Component: $COMPONENT"
echo "Duration: ${DURATION}s, Connections: $CONNECTIONS"
echo ""

# Build mik if needed
if [ ! -f "target/release/mik.exe" ]; then
    echo "Building mik..."
    cargo build --release
fi

MIK="./target/release/mik.exe"

# Function to run benchmark
run_bench() {
    local name="$1"
    local workers="$2"

    echo "=========================================="
    echo "Benchmark: $name (workers=$workers)"
    echo "=========================================="

    # Start mik with LB
    echo "Starting mik with $workers workers..."
    $MIK run "$COMPONENT" --workers "$workers" --lb --port $PORT &
    MIK_PID=$!

    # Wait for startup
    sleep 3

    # Verify server is running
    if ! curl -s "http://127.0.0.1:$PORT/health" > /dev/null; then
        echo "Error: Server not responding on port $PORT"
        kill $MIK_PID 2>/dev/null || true
        return 1
    fi

    echo "Server ready. Running benchmark..."
    echo ""

    # Run benchmark
    if [ "$BENCH_TOOL" = "oha" ]; then
        oha -z ${DURATION}s -c $CONNECTIONS "http://127.0.0.1:$PORT/run/hello-world/"
    else
        wrk -t$THREADS -c$CONNECTIONS -d${DURATION}s "http://127.0.0.1:$PORT/run/hello-world/"
    fi

    echo ""

    # Stop mik
    echo "Stopping server..."
    kill $MIK_PID 2>/dev/null || true
    wait $MIK_PID 2>/dev/null || true
    sleep 2

    echo ""
}

# Run benchmarks
echo ""
echo "############################################"
echo "#          LB WORKER BENCHMARK             #"
echo "############################################"
echo ""

run_bench "1 Worker" 1
run_bench "Auto Workers (CPU cores)" 0

echo "############################################"
echo "#              DONE                        #"
echo "############################################"
