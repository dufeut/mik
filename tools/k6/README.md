# k6 Load Testing Scripts

Load testing scripts for mik using [k6](https://k6.io/).

## Installation

```bash
# macOS
brew install k6

# Windows
choco install k6

# Linux (Debian/Ubuntu)
sudo gpg -k
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update
sudo apt-get install k6
```

## Scripts

### basic_load.js

Basic load test with gradual ramp-up. Tests normal operation.

```bash
# Run with defaults
k6 run basic_load.js

# Custom virtual users and duration
k6 run --vus 20 --duration 1m basic_load.js

# Custom server URL
k6 run --env BASE_URL=http://localhost:8080 basic_load.js
```

**Stages:**
1. Ramp up to 5 users (10s)
2. Hold at 5 users (30s)
3. Ramp to 10 users (10s)
4. Hold at 10 users (30s)
5. Ramp down (10s)

**Thresholds:**
- 95th percentile latency < 500ms
- Error rate < 1%

### stress_test.js

Aggressive stress test to find breaking points.

```bash
k6 run stress_test.js
```

**Stages:**
1. Warm up: 10 users (10s)
2. Normal: 20 users (20s)
3. Stress: 50 users (20s)
4. Maximum: 100 users (30s)
5. Spike: 200 users (10s)
6. Recovery: 20 users (20s)
7. Cool down (10s)

**Custom Metrics:**
- `wasm_latency`: WASM module execution time
- `circuit_breaker_trips`: 503/429 responses (rate limiting)
- `timeout_errors`: Request timeouts
- `server_errors`: 5xx errors

## Prerequisites

Before running tests, ensure:

1. mik is running:
   ```bash
   cd mik && cargo run -- run
   ```

2. echo.wasm module is available in the modules directory

## Output

k6 provides built-in output formats:

```bash
# JSON output
k6 run --out json=results.json basic_load.js

# InfluxDB (for Grafana dashboards)
k6 run --out influxdb=http://localhost:8086/k6 basic_load.js

# Cloud (k6 Cloud)
k6 cloud basic_load.js
```

## Interpreting Results

Key metrics to watch:

| Metric | Good | Warning | Bad |
|--------|------|---------|-----|
| http_req_duration p95 | < 200ms | < 500ms | > 1s |
| http_req_failed | < 0.1% | < 1% | > 5% |
| iterations | Stable | Declining | Crashing |

## Integration with CI/CD

```yaml
# GitHub Actions example
- name: Run k6 load tests
  uses: grafana/k6-action@v0.3.1
  with:
    filename: tools/k6/basic_load.js
    flags: --env BASE_URL=http://localhost:3000
```
