// k6 Stress Test for mikrozen-host
//
// This script performs stress testing to find breaking points and verify
// resilience under extreme load. Based on wasmCloud's k6 testing practices.
//
// Usage:
//   k6 run stress_test.js
//   k6 run --env BASE_URL=http://localhost:8080 stress_test.js
//
// This test will:
//   1. Gradually increase load until the server struggles
//   2. Test recovery after stress
//   3. Verify circuit breaker behavior
//   4. Test concurrent module loading
//
// WARNING: This test is intentionally aggressive. Run on isolated systems.

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');
const wasmLatency = new Trend('wasm_latency');
const circuitBreakerTrips = new Counter('circuit_breaker_trips');
const timeoutErrors = new Counter('timeout_errors');
const serverErrors = new Counter('server_errors');

// Configuration
const BASE_URL = __ENV.BASE_URL || 'http://localhost:3000';

// Stress test options - aggressive load increase
export const options = {
    stages: [
        // Warm up
        { duration: '10s', target: 10 },

        // Normal load
        { duration: '20s', target: 20 },

        // Push to stress
        { duration: '20s', target: 50 },

        // Maximum stress
        { duration: '30s', target: 100 },

        // Spike test
        { duration: '10s', target: 200 },

        // Recovery check
        { duration: '20s', target: 20 },

        // Cool down
        { duration: '10s', target: 0 },
    ],
    thresholds: {
        // More lenient thresholds for stress test
        http_req_duration: ['p(95)<2000'],  // 95% under 2s
        http_req_failed: ['rate<0.10'],     // Less than 10% errors (stress expected)
    },
};

// Setup
export function setup() {
    const healthRes = http.get(`${BASE_URL}/health`);

    if (healthRes.status !== 200) {
        throw new Error(`Server not ready: ${healthRes.status}`);
    }

    console.log('Starting stress test - expect some errors under load');
    return { baseUrl: BASE_URL };
}

// Main stress test
export default function(data) {
    const baseUrl = data.baseUrl;

    group('WASM Module Stress', function() {
        // Stress the echo module
        const payload = JSON.stringify({
            stress: true,
            iteration: __ITER,
            vu: __VU,
            data: 'x'.repeat(1000), // 1KB payload
        });

        const start = Date.now();
        const res = http.post(`${baseUrl}/run/echo/`, payload, {
            headers: { 'Content-Type': 'application/json' },
            timeout: '10s',
        });
        const duration = Date.now() - start;

        wasmLatency.add(duration);

        // Track different error types
        if (res.status === 0) {
            timeoutErrors.add(1);
            errorRate.add(true);
        } else if (res.status === 503 || res.status === 429) {
            circuitBreakerTrips.add(1);
            // 503/429 during stress is expected, not an error
            errorRate.add(false);
        } else if (res.status >= 500) {
            serverErrors.add(1);
            errorRate.add(true);
        } else if (res.status === 200) {
            errorRate.add(false);
            check(res, {
                'response is valid JSON': (r) => {
                    try {
                        JSON.parse(r.body);
                        return true;
                    } catch {
                        return false;
                    }
                },
            });
        } else {
            errorRate.add(true);
        }
    });

    // Occasionally test health to verify server is responsive
    if (__ITER % 10 === 0) {
        group('Health Check', function() {
            const healthRes = http.get(`${baseUrl}/health`, {
                timeout: '5s',
            });

            check(healthRes, {
                'health endpoint responds': (r) => r.status === 200,
            });
        });
    }

    // Very short sleep - stress test
    sleep(0.05);
}

// Teardown
export function teardown(data) {
    console.log('\nStress test completed');
    console.log('Check the metrics for:');
    console.log('  - circuit_breaker_trips: Times server returned 503/429');
    console.log('  - timeout_errors: Requests that timed out');
    console.log('  - server_errors: 5xx errors (excluding 503)');
    console.log('  - wasm_latency: WASM execution times under load');
}
