//! # Circuit Breaker Example
//!
//! This example demonstrates how to use the circuit breaker pattern for
//! building resilient service calls. The circuit breaker protects your
//! handlers from cascading failures when downstream services are unhealthy.
//!
//! ## What This Example Shows
//!
//! - Creating a `CircuitBreaker` with custom configuration
//! - The check/record pattern for request handling
//! - Understanding the three circuit states: Closed, Open, HalfOpen
//! - State transitions and recovery behavior
//!
//! ## Circuit Breaker States
//!
//! ```text
//! +--------+     failures >= threshold     +------+
//! | Closed | -----------------------------> | Open |
//! +--------+                                +------+
//!     ^                                        |
//!     |                                        | timeout elapsed
//!     |                                        v
//!     |     probe succeeds               +----------+
//!     +--------------------------------- | HalfOpen |
//!                                        +----------+
//!                                             |
//!                probe fails                  |
//!                +----------------------------+
//!                |
//!                v
//!             +------+
//!             | Open |
//!             +------+
//! ```
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example 03_circuit_breaker
//! ```
//!
//! ## Related Documentation
//!
//! - [`mik::reliability::CircuitBreaker`] - The main circuit breaker type
//! - [`mik::reliability::CircuitBreakerConfig`] - Configuration options
//! - [`mik::reliability::CircuitState`] - The three possible states

use anyhow::Result;
use std::time::Duration;

use mik::reliability::{CircuitBreaker, CircuitBreakerConfig};

fn main() -> Result<()> {
    println!("=== mik Circuit Breaker Example ===\n");

    // =========================================================================
    // Part 1: Creating a Circuit Breaker
    // =========================================================================
    //
    // The circuit breaker is configured with thresholds and timeouts.
    // You can use defaults or customize for your use case.

    println!("--- Part 1: Creating a Circuit Breaker ---\n");

    // Create with default configuration:
    // - failure_threshold: 5 (opens after 5 consecutive failures)
    // - timeout: 60 seconds (time in open state before half-open)
    let default_cb = CircuitBreaker::new();
    println!("Default circuit breaker created");
    println!("  Failure threshold: 5");
    println!("  Recovery timeout: 60 seconds");

    // Create with custom configuration for faster demonstration
    let config = CircuitBreakerConfig {
        // Number of consecutive failures before opening the circuit
        failure_threshold: 3,
        // How long to wait in Open state before allowing a probe request
        timeout: Duration::from_secs(5),
        // Maximum time for a probe request in HalfOpen state
        probe_timeout: Duration::from_secs(10),
        // Maximum number of keys to track (uses LRU eviction)
        max_tracked_keys: 1000,
        // Time after which idle keys are evicted
        idle_timeout: Duration::from_secs(600),
    };

    let cb = CircuitBreaker::with_config(config);
    println!("\nCustom circuit breaker created:");
    println!("  Failure threshold: 3");
    println!("  Recovery timeout: 5 seconds");

    // =========================================================================
    // Part 2: The Check/Record Pattern
    // =========================================================================
    //
    // The circuit breaker uses a check/record pattern:
    // 1. check_request() - Check if request should be allowed
    // 2. Make the actual call (if allowed)
    // 3. record_success() or record_failure() - Record the outcome

    println!("\n--- Part 2: The Check/Record Pattern ---\n");

    let service_key = "external-api";

    // Simulate a successful request
    println!("Simulating successful request to '{}':", service_key);
    if cb.check_request(service_key).is_ok() {
        println!("  Request allowed - making call...");
        // In a real application, you would make the actual HTTP call here
        let success = true; // Simulated success
        if success {
            cb.record_success(service_key);
            println!("  Call succeeded - recorded success");
        }
    }

    // Check the current state
    println!("  Current state: {:?}", cb.get_state(service_key));

    // =========================================================================
    // Part 3: Failure Tracking and Circuit Opening
    // =========================================================================
    //
    // When consecutive failures reach the threshold, the circuit opens
    // and subsequent requests are rejected immediately (fail-fast).

    println!("\n--- Part 3: Failure Tracking and Circuit Opening ---\n");

    let flaky_service = "flaky-database";
    println!("Simulating failures to '{}':", flaky_service);

    // Record 3 failures (our threshold)
    for i in 1..=3 {
        if cb.check_request(flaky_service).is_ok() {
            println!("  Request {} allowed - simulating failure...", i);
            cb.record_failure(flaky_service);
            println!(
                "  Failure recorded. Failure count: {}",
                cb.failure_count(flaky_service)
            );
        } else {
            println!("  Request {} blocked by circuit breaker", i);
        }
        println!(
            "  State after failure {}: {:?}",
            i,
            cb.get_state(flaky_service)
        );
    }

    // Try another request - should be blocked
    println!("\nTrying request after circuit opened:");
    match cb.check_request(flaky_service) {
        Ok(()) => println!("  Request allowed (unexpected)"),
        Err(e) => println!("  Request blocked: {}", e),
    }

    // =========================================================================
    // Part 4: Understanding Circuit States
    // =========================================================================
    //
    // The circuit has three states:
    // - Closed: Normal operation, requests allowed
    // - Open: Too many failures, requests blocked
    // - HalfOpen: Testing recovery with a single probe request

    println!("\n--- Part 4: Understanding Circuit States ---\n");

    // Create a fresh circuit breaker for this demonstration
    let demo_cb = CircuitBreaker::with_config(CircuitBreakerConfig {
        failure_threshold: 2,
        timeout: Duration::from_millis(100), // Very short for demo
        probe_timeout: Duration::from_secs(5),
        ..Default::default()
    });

    let demo_key = "demo-service";

    // State 1: Closed (default)
    println!("State 1: CLOSED (initial state)");
    println!(
        "  Requests allowed: {}",
        demo_cb.check_request(demo_key).is_ok()
    );
    demo_cb.record_success(demo_key); // Reset any state

    // State 2: Open (after failures)
    println!("\nState 2: OPEN (after failures exceed threshold)");
    demo_cb.record_failure(demo_key);
    demo_cb.record_failure(demo_key);
    println!("  Circuit opened: {}", demo_cb.is_open(demo_key));
    println!(
        "  Requests blocked: {}",
        demo_cb.check_request(demo_key).is_err()
    );

    // State 3: HalfOpen (after timeout)
    println!("\nState 3: HALF_OPEN (after timeout, testing recovery)");
    println!("  Waiting for timeout to elapse...");
    std::thread::sleep(Duration::from_millis(150));

    // The next check_request will transition to HalfOpen and allow ONE probe
    match demo_cb.check_request(demo_key) {
        Ok(()) => {
            println!("  Probe request allowed");
            // If the probe succeeds, circuit closes
            demo_cb.record_success(demo_key);
            println!("  Probe succeeded - circuit closed");
            println!("  Final state: {:?}", demo_cb.get_state(demo_key));
        },
        Err(e) => {
            println!("  Probe blocked: {}", e);
        },
    }

    // =========================================================================
    // Part 5: Per-Key Circuit Breakers
    // =========================================================================
    //
    // The circuit breaker tracks state per key, allowing independent
    // failure tracking for different services or endpoints.

    println!("\n--- Part 5: Per-Key Circuit Breakers ---\n");

    let multi_cb = CircuitBreaker::new();

    // Different services can have different states
    let services = ["api-gateway", "auth-service", "database"];

    // Simulate different failure patterns
    multi_cb.record_failure("auth-service");
    multi_cb.record_failure("auth-service");
    // auth-service has 2 failures but threshold is 5, so still closed

    multi_cb.record_success("database");
    // database is healthy

    // Check states
    println!("Service states:");
    for service in services {
        let state = multi_cb.get_state(service);
        let allowed = multi_cb.check_request(service).is_ok();
        println!(
            "  {}: {:?} (requests {})",
            service,
            state,
            if allowed { "allowed" } else { "blocked" }
        );
    }

    // Get all tracked states
    println!("\nAll tracked circuits:");
    for (key, state) in multi_cb.get_all_states() {
        println!("  {}: {}", key, state);
    }

    // =========================================================================
    // Part 6: Real-World Usage Pattern
    // =========================================================================
    //
    // Here's how you would typically use the circuit breaker in a request handler.

    println!("\n--- Part 6: Real-World Usage Pattern ---\n");

    // In a real application, you would have a shared circuit breaker
    // (e.g., stored in application state or dependency injection)
    let app_cb = CircuitBreaker::new();

    // Simulated request handler
    fn handle_request(cb: &CircuitBreaker, service: &str) -> std::result::Result<String, String> {
        // Step 1: Check if request should be allowed
        if let Err(e) = cb.check_request(service) {
            // Circuit is open - fail fast without calling the service
            return Err(format!("Service unavailable: {}", e));
        }

        // Step 2: Make the actual call
        let result = call_external_service(service);

        // Step 3: Record the outcome
        match &result {
            Ok(_) => cb.record_success(service),
            Err(_) => cb.record_failure(service),
        }

        result
    }

    // Simulated external service call
    fn call_external_service(service: &str) -> std::result::Result<String, String> {
        // In reality, this would be an HTTP call, database query, etc.
        if service == "broken-service" {
            Err("Connection refused".to_string())
        } else {
            Ok(format!("Response from {}", service))
        }
    }

    // Test the pattern
    println!("Testing request handler pattern:");

    // Successful calls
    for i in 1..=3 {
        match handle_request(&app_cb, "healthy-service") {
            Ok(response) => println!("  Request {}: {}", i, response),
            Err(e) => println!("  Request {}: Error - {}", i, e),
        }
    }

    // Failed calls (will eventually open the circuit)
    println!("\nCalling broken service (will trigger failures):");
    for i in 1..=6 {
        match handle_request(&app_cb, "broken-service") {
            Ok(response) => println!("  Request {}: {}", i, response),
            Err(e) => println!("  Request {}: {}", i, e),
        }
    }

    // =========================================================================
    // Part 7: Inspecting Circuit Breaker State
    // =========================================================================
    //
    // The circuit breaker provides methods for monitoring and debugging.

    println!("\n--- Part 7: Inspecting Circuit Breaker State ---\n");

    println!("Inspection methods:");
    println!("  tracked_count(): {}", app_cb.tracked_count());
    println!(
        "  failure_count('healthy-service'): {}",
        app_cb.failure_count("healthy-service")
    );
    println!(
        "  failure_count('broken-service'): {}",
        app_cb.failure_count("broken-service")
    );
    println!(
        "  is_open('broken-service'): {}",
        app_cb.is_open("broken-service")
    );
    println!(
        "  is_blocking('broken-service'): {}",
        app_cb.is_blocking("broken-service")
    );

    // Manual reset (useful for admin endpoints)
    println!("\nManual reset:");
    println!("  Before reset: {:?}", app_cb.get_state("broken-service"));
    app_cb.reset("broken-service");
    println!("  After reset: {:?}", app_cb.get_state("broken-service"));

    println!("\n=== Example Complete ===");

    // Keep the default_cb variable used to avoid warning
    drop(default_cb);

    Ok(())
}
