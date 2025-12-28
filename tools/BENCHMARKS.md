# mik Performance Benchmarks

This document describes the benchmarking tools and methodology for mik performance testing.

## Overview

mik provides two types of benchmarks:

1. **Criterion Benchmarks** (`mik/benches/`) - In-process micro-benchmarks for component-level performance
2. **HTTP Load Testing** (`tools/benchmark.py` and `tools/bench/`) - End-to-end HTTP performance testing

## Criterion Benchmarks

Criterion benchmarks measure the overhead of individual runtime components without network I/O.

### Running Criterion Benchmarks

```bash
# Run all benchmarks
cargo bench -p mik --bench runtime_benchmarks

# Run specific benchmark group
cargo bench -p mik --bench runtime_benchmarks -- circuit_breaker
cargo bench -p mik --bench runtime_benchmarks -- module_cache
cargo bench -p mik --bench runtime_benchmarks -- script_execution
cargo bench -p mik --bench runtime_benchmarks -- concurrent_access

# Generate HTML report
cargo bench -p mik --bench runtime_benchmarks -- --verbose
# Open: target/criterion/report/index.html
```

### Benchmark Groups

| Group | Description |
|-------|-------------|
| `circuit_breaker` | Check/record operations, multi-key performance |
| `module_cache` | Cache hit/miss, insert, LRU eviction overhead |
| `script_execution` | JS runtime creation, script evaluation |
| `concurrent_access` | Multi-threaded circuit breaker and cache access |

### Expected Results

Typical results on modern hardware:

| Benchmark | Expected | Notes |
|-----------|----------|-------|
| `circuit_breaker/check_request_closed` | < 1 us | Fast path, no state change |
| `circuit_breaker/check_and_record_success` | < 5 us | Full request cycle |
| `module_cache/cache_hit` | < 500 ns | moka cache lookup |
| `module_cache/cache_miss` | < 200 ns | Missing key check |
| `script_execution/runtime_create` | ~50-100 ms | QuickJS initialization (cold) |
| `script_execution/eval_simple` | < 10 us | Pre-warmed runtime |

## HTTP Load Testing

### Python Benchmark Script

A comprehensive HTTP benchmarking tool for load testing.

#### Requirements

```bash
pip install httpx rich
# or using uv:
uv add httpx rich
```

#### Basic Usage

```bash
# Basic health endpoint benchmark
uv run tools/benchmark.py

# Custom endpoint with options
uv run tools/benchmark.py --url http://localhost:3000/run/hello --duration 30 --concurrency 100

# Compare cold start vs warm cache
uv run tools/benchmark.py --compare-cold-warm

# POST with body
uv run tools/benchmark.py -m POST -b '{"key":"value"}' --url http://localhost:3000/run/echo

# JSON output for CI
uv run tools/benchmark.py --json > results.json
```

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--url`, `-u` | `http://127.0.0.1:3000/health` | Target URL |
| `--duration`, `-d` | 10 | Benchmark duration (seconds) |
| `--concurrency`, `-c` | 50 | Concurrent connections |
| `--method`, `-m` | GET | HTTP method |
| `--body`, `-b` | - | Request body for POST/PUT |
| `--warmup`, `-w` | 2 | Warmup duration (seconds) |
| `--compare-cold-warm` | - | Compare cold start vs warm cache |
| `--json` | - | Output as JSON |

### mik-bench Tool

High-performance Rust-based benchmark tool with HDR histograms.

```bash
# Build the tool
cd tools/bench
cargo build --release

# Run benchmark
./target/release/mik-bench http://localhost:3000/health -c 100 -d 20
```

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `-c, --connections` | 50 | Concurrent connections |
| `-d, --duration` | 10 | Duration (seconds) |
| `-m, --method` | GET | HTTP method |
| `-b, --body` | - | Request body |
| `-w, --warmup` | 2 | Warmup duration |
| `--no-keepalive` | - | Disable keep-alive |

### Comparison with hey/wrk

For quick ad-hoc benchmarks, use standard tools:

```bash
# Using hey
hey -z 20s -c 150 http://localhost:3000/health

# Using wrk
wrk -t4 -c100 -d30s http://localhost:3000/health

# Using ab (Apache Bench)
ab -n 10000 -c 100 http://localhost:3000/health
```

## Benchmark Scenarios

### 1. Health Check Throughput

Baseline performance with minimal processing:

```bash
uv run tools/benchmark.py --url http://localhost:3000/health -c 100 -d 20
```

Expected: 10,000+ req/s on modern hardware

### 2. WASM Module Execution

Test handler execution performance:

```bash
# Ensure a test handler is deployed
uv run tools/benchmark.py --url http://localhost:3000/run/hello -c 50 -d 20
```

### 3. Cold vs Warm Cache

Measure module caching effectiveness:

```bash
# Restart the server first, then:
uv run tools/benchmark.py --url http://localhost:3000/run/hello --compare-cold-warm
```

### 4. Script Orchestration

Test JavaScript script execution overhead:

```bash
uv run tools/benchmark.py --url http://localhost:3000/script/checkout -m POST \
  -b '{"token":"abc","userId":1,"items":[1,2,3]}' -c 25 -d 20
```

### 5. Concurrent Load

Stress test with high concurrency:

```bash
uv run tools/benchmark.py -c 500 -d 60 --url http://localhost:3000/health
```

## Performance Targets

Based on production requirements:

| Metric | Target | Critical |
|--------|--------|----------|
| Health check RPS | > 10,000 | > 5,000 |
| WASM handler RPS | > 1,000 | > 500 |
| Script RPS | > 500 | > 200 |
| p50 latency | < 1ms | < 5ms |
| p99 latency | < 10ms | < 50ms |
| Cold start | < 100ms | < 500ms |
| Warm start | < 5ms | < 20ms |

## CI Integration

### GitHub Actions Example

```yaml
- name: Run benchmarks
  run: |
    # Start server in background
    cargo run -p mik --release -- run &
    sleep 5

    # Run benchmarks
    uv run tools/benchmark.py --json > benchmark-results.json

    # Check thresholds
    python -c "
    import json
    results = json.load(open('benchmark-results.json'))
    assert results['requests_per_second'] > 5000, 'RPS below threshold'
    assert results['latency_ms']['p99'] < 50, 'P99 latency too high'
    "
```

### Tracking Performance Over Time

Use criterion's built-in comparison:

```bash
# Save baseline
cargo bench -p mik --bench runtime_benchmarks -- --save-baseline main

# Compare against baseline
cargo bench -p mik --bench runtime_benchmarks -- --baseline main
```

## Troubleshooting

### Low Throughput

1. Check if running in release mode: `cargo run --release`
2. Verify CPU isn't throttled
3. Check for resource limits: `ulimit -n`
4. Ensure keep-alive is enabled

### High Latency Variance

1. Use warmup phase: `--warmup 5`
2. Run longer benchmarks: `--duration 60`
3. Reduce concurrency to avoid saturation

### Connection Errors

1. Increase file descriptor limit: `ulimit -n 65535`
2. Check server's max connections configuration
3. Verify server is running and healthy

## Related Documentation

- [CLAUDE.md](../CLAUDE.md) - Project overview and configuration
- [stress_tests.rs](../mik/tests/stress_tests.rs) - Integration stress tests
- [wasmtime Documentation](https://docs.wasmtime.dev/) - Core runtime
