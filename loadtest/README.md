# Load Testing

Load tests using wrk2 for sustained load scenarios.

## Requirements

- wrk2: https://github.com/giltene/wrk2

## Usage

```bash
# Basic load test (1000 req/s for 60 seconds)
wrk2 -t4 -c100 -d60s -R1000 http://localhost:3000/run/hello/

# High load test (10000 req/s)
wrk2 -t8 -c400 -d120s -R10000 --latency http://localhost:3000/run/hello/
```

## Scripts

- `basic.lua` - Simple GET request
- `mixed_workload.lua` - Realistic mixed operations
