// k6 Basic Load Test for mikrozen-host
//
// This script performs basic load testing against the mikrozen runtime.
// Based on wasmCloud's k6 testing practices.
//
// Usage:
//   k6 run basic_load.js
//   k6 run --vus 10 --duration 30s basic_load.js
//   k6 run --env BASE_URL=http://localhost:8080 basic_load.js
//
// Requirements:
//   - k6 installed: https://k6.io/docs/getting-started/installation/
//   - mikrozen-host running with echo.wasm module
//
// Metrics collected:
//   - http_req_duration: Response time
//   - http_req_failed: Error rate
//   - http_reqs: Request throughput

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');
const echoLatency = new Trend('echo_latency');

// Configuration
const BASE_URL = __ENV.BASE_URL || 'http://localhost:3000';

// Test options - basic load test
export const options = {
    stages: [
        { duration: '10s', target: 5 },   // Ramp up to 5 users
        { duration: '30s', target: 5 },   // Stay at 5 users
        { duration: '10s', target: 10 },  // Ramp up to 10 users
        { duration: '30s', target: 10 },  // Stay at 10 users
        { duration: '10s', target: 0 },   // Ramp down
    ],
    thresholds: {
        http_req_duration: ['p(95)<500'],  // 95% of requests under 500ms
        http_req_failed: ['rate<0.01'],    // Less than 1% errors
        errors: ['rate<0.01'],             // Custom error rate
    },
};

// Setup - verify server is running
export function setup() {
    const healthRes = http.get(`${BASE_URL}/health`);

    if (healthRes.status !== 200) {
        throw new Error(`Server not ready: ${healthRes.status}`);
    }

    console.log('Server is ready, starting load test...');
    return { baseUrl: BASE_URL };
}

// Main test function
export default function(data) {
    const baseUrl = data.baseUrl;

    // Test 1: Health endpoint (fast, no WASM)
    const healthRes = http.get(`${baseUrl}/health`);
    check(healthRes, {
        'health status is 200': (r) => r.status === 200,
        'health response is JSON': (r) => r.headers['Content-Type'].includes('application/json'),
    });

    // Test 2: Echo module (WASM execution)
    const echoPayload = JSON.stringify({
        message: 'load test',
        iteration: __ITER,
        vu: __VU,
        timestamp: Date.now(),
    });

    const echoStart = Date.now();
    const echoRes = http.post(`${baseUrl}/run/echo/`, echoPayload, {
        headers: { 'Content-Type': 'application/json' },
    });
    const echoEnd = Date.now();

    echoLatency.add(echoEnd - echoStart);

    const echoSuccess = check(echoRes, {
        'echo status is 200': (r) => r.status === 200,
        'echo returns JSON': (r) => {
            try {
                JSON.parse(r.body);
                return true;
            } catch {
                return false;
            }
        },
        'echo returns message': (r) => {
            try {
                const body = JSON.parse(r.body);
                return body.message === 'load test';
            } catch {
                return false;
            }
        },
    });

    errorRate.add(!echoSuccess);

    // Small sleep to prevent overwhelming
    sleep(0.1);
}

// Teardown - print summary
export function teardown(data) {
    console.log('Load test completed');
}
