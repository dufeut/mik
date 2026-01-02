//! Property-based tests for circuit breaker invariants.
//!
//! These tests use proptest to verify that the circuit breaker maintains its
//! correctness guarantees under arbitrary operation sequences and configurations.
//!
//! # Tested Invariants
//!
//! - State transitions follow the valid state machine
//! - Failure counting is monotonic until reset
//! - Circuit opens exactly at threshold
//! - Multi-key isolation is maintained
//! - Concurrent operations are safe
//!
//! # Running Tests
//!
//! ```bash
//! cargo test circuit_breaker::property_tests
//! ```

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use proptest::prelude::*;

    use crate::reliability::circuit_breaker::{
        CircuitBreaker, CircuitBreakerConfig, CircuitOpenReason, CircuitState,
    };

    // ============================================================================
    // Test Strategies - Input Generation
    // ============================================================================

    /// Strategy for generating valid failure thresholds.
    fn threshold_strategy() -> impl Strategy<Value = u32> {
        1u32..100
    }

    /// Strategy for generating operation sequences (true = success, false = failure).
    fn operations_strategy() -> impl Strategy<Value = Vec<bool>> {
        prop::collection::vec(any::<bool>(), 0..200)
    }

    /// Strategy for generating key names.
    fn key_strategy() -> impl Strategy<Value = String> {
        "[a-z]{1,20}".prop_map(|s| s)
    }

    /// Strategy for generating multiple unique keys.
    #[allow(dead_code)] // Reserved for future tests
    fn multi_key_strategy() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec(key_strategy(), 1..10)
    }

    // ============================================================================
    // State Machine Invariants
    // ============================================================================

    proptest! {
        /// Invariant: State is always one of the three valid states.
        ///
        /// After any sequence of operations, the circuit state must be
        /// Closed, Open, or HalfOpen - never an invalid state.
        #[test]
        fn state_always_valid(ops in operations_strategy()) {
            let cb = CircuitBreaker::new();

            for success in ops {
                if success {
                    cb.record_success("test");
                } else {
                    cb.record_failure("test");
                }

                let state = cb.get_state("test");
                prop_assert!(
                    matches!(
                        state,
                        CircuitState::Closed { .. }
                            | CircuitState::Open { .. }
                            | CircuitState::HalfOpen { .. }
                    ),
                    "State should be valid, got: {:?}",
                    state
                );
            }
        }

        /// Invariant: Circuit opens exactly at threshold.
        ///
        /// The circuit should transition to Open state only when the failure
        /// count reaches the configured threshold, not before.
        #[test]
        fn opens_at_threshold(threshold in threshold_strategy(), extra_failures in 0u32..20) {
            let config = CircuitBreakerConfig {
                failure_threshold: threshold,
                timeout: Duration::from_secs(300), // Long timeout to prevent half-open
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            // Record failures up to threshold - 1
            for i in 0..threshold.saturating_sub(1) {
                cb.record_failure("test");
                prop_assert!(
                    !cb.is_open("test"),
                    "Circuit should not be open after {} failures (threshold={})",
                    i + 1,
                    threshold
                );
            }

            // The threshold-th failure should open the circuit
            cb.record_failure("test");
            prop_assert!(
                cb.is_open("test"),
                "Circuit should be open after {} failures (threshold={})",
                threshold,
                threshold
            );

            // Additional failures should keep it open
            for _ in 0..extra_failures {
                cb.record_failure("test");
                prop_assert!(cb.is_open("test"), "Circuit should remain open");
            }
        }

        /// Invariant: Success resets failure count in Closed state.
        ///
        /// Recording a success while in Closed state should reset the
        /// failure count to zero.
        #[test]
        fn success_resets_failures(
            failures in 1u32..10,
            threshold in 10u32..50,
        ) {
            let config = CircuitBreakerConfig {
                failure_threshold: threshold,
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            // Record some failures (less than threshold)
            for _ in 0..failures {
                cb.record_failure("test");
            }
            prop_assert_eq!(cb.failure_count("test"), failures);

            // Success should reset
            cb.record_success("test");
            prop_assert_eq!(
                cb.failure_count("test"),
                0,
                "Success should reset failure count"
            );
        }

        /// Invariant: Failure count never exceeds what was recorded.
        ///
        /// The failure count should accurately reflect the number of
        /// failures recorded since the last reset.
        #[test]
        fn failure_count_accurate(
            ops in prop::collection::vec(any::<bool>(), 0..100),
            threshold in 50u32..100, // High threshold to stay in Closed
        ) {
            let config = CircuitBreakerConfig {
                failure_threshold: threshold,
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            let mut expected_failures = 0u32;

            for success in ops {
                if success {
                    cb.record_success("test");
                    expected_failures = 0;
                } else {
                    cb.record_failure("test");
                    // Only increment if we haven't opened yet
                    if expected_failures < threshold {
                        expected_failures = expected_failures.saturating_add(1);
                    }
                }

                // In Closed state, failure count should match
                if let CircuitState::Closed { failure_count } = cb.get_state("test") {
                    prop_assert!(
                        failure_count <= expected_failures,
                        "Failure count {} exceeds expected {}",
                        failure_count,
                        expected_failures
                    );
                }
            }
        }
    }

    // ============================================================================
    // Key Isolation Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Keys are isolated from each other.
        ///
        /// Operations on one key should not affect the state of another key.
        #[test]
        fn keys_isolated(
            key1 in key_strategy(),
            key2 in key_strategy(),
            failures in 1u32..5,
        ) {
            // Skip if keys are the same
            if key1 == key2 {
                return Ok(());
            }

            let config = CircuitBreakerConfig {
                failure_threshold: failures,
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            // Open circuit for key1
            for _ in 0..failures {
                cb.record_failure(&key1);
            }

            // key1 should be open
            prop_assert!(cb.is_open(&key1), "key1 should be open");

            // key2 should still be closed
            prop_assert!(!cb.is_open(&key2), "key2 should be closed");
            prop_assert_eq!(cb.failure_count(&key2), 0, "key2 should have zero failures");

            // Operations on key2 should work normally
            prop_assert!(cb.check_request(&key2).is_ok(), "key2 requests should be allowed");
        }

        /// Invariant: Each key maintains independent failure counts.
        #[test]
        fn independent_failure_counts(
            key_count in 2usize..6,
            failures_per_key in prop::collection::vec(1u32..10, 2..6),
        ) {
            let config = CircuitBreakerConfig {
                failure_threshold: 100, // High threshold to stay in Closed
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            // Generate unique keys based on index
            let keys: Vec<String> = (0..key_count).map(|i| format!("key-{i}")).collect();

            // Record different numbers of failures for each key
            for (i, key) in keys.iter().enumerate() {
                let failures = failures_per_key.get(i % failures_per_key.len()).copied().unwrap_or(1);
                for _ in 0..failures {
                    cb.record_failure(key);
                }
            }

            // Verify each key has its own failure count
            for (i, key) in keys.iter().enumerate() {
                let expected = failures_per_key.get(i % failures_per_key.len()).copied().unwrap_or(1);
                let count = cb.failure_count(key);
                prop_assert_eq!(
                    count,
                    expected,
                    "Key '{}': failure count {} should equal expected {}",
                    key,
                    count,
                    expected
                );
            }
        }
    }

    // ============================================================================
    // Request Blocking Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Open circuit blocks requests.
        ///
        /// When a circuit is Open (and timeout not elapsed), check_request
        /// should return an error.
        #[test]
        fn open_circuit_blocks(threshold in 1u32..10) {
            let config = CircuitBreakerConfig {
                failure_threshold: threshold,
                timeout: Duration::from_secs(300), // Long timeout
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            // Open the circuit
            for _ in 0..threshold {
                cb.record_failure("test");
            }

            // Requests should be blocked
            let result = cb.check_request("test");
            prop_assert!(result.is_err(), "Open circuit should block requests");

            if let Err(e) = result {
                prop_assert_eq!(e.reason, CircuitOpenReason::Open);
                prop_assert_eq!(e.key, "test");
            }
        }

        /// Invariant: Closed circuit allows requests.
        ///
        /// When a circuit is Closed, check_request should always succeed.
        #[test]
        fn closed_circuit_allows(failures in 0u32..5, threshold in 10u32..20) {
            let config = CircuitBreakerConfig {
                failure_threshold: threshold,
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            // Record some failures (less than threshold)
            for _ in 0..failures.min(threshold - 1) {
                cb.record_failure("test");
            }

            // Requests should still be allowed
            prop_assert!(
                cb.check_request("test").is_ok(),
                "Closed circuit should allow requests"
            );
        }
    }

    // ============================================================================
    // Reset Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Reset returns circuit to initial state.
        ///
        /// After reset, a circuit should be in Closed state with zero failures.
        #[test]
        fn reset_restores_initial_state(
            ops in operations_strategy(),
            threshold in 1u32..20,
        ) {
            let config = CircuitBreakerConfig {
                failure_threshold: threshold,
                timeout: Duration::from_secs(300),
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            // Perform arbitrary operations
            for success in ops {
                if success {
                    cb.record_success("test");
                } else {
                    cb.record_failure("test");
                }
            }

            // Reset
            cb.reset("test");

            // Should be back to initial state
            prop_assert_eq!(
                cb.get_state("test"),
                CircuitState::Closed { failure_count: 0 },
                "Reset should restore Closed state with zero failures"
            );
            prop_assert!(!cb.is_open("test"), "Reset circuit should not be open");
            prop_assert!(
                cb.check_request("test").is_ok(),
                "Reset circuit should allow requests"
            );
        }

        /// Invariant: Reset of nonexistent key is a no-op.
        ///
        /// Resetting a key that was never used should not panic or cause issues.
        #[test]
        fn reset_nonexistent_is_noop(key in key_strategy()) {
            let cb = CircuitBreaker::new();

            // Should not panic
            cb.reset(&key);

            // State should be default (Closed with 0 failures)
            prop_assert_eq!(
                cb.get_state(&key),
                CircuitState::Closed { failure_count: 0 }
            );
        }
    }

    // ============================================================================
    // Concurrent Access Invariants
    // ============================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// Invariant: Concurrent operations don't cause panics or data corruption.
        ///
        /// Multiple threads performing operations simultaneously should not
        /// cause panics, deadlocks, or leave the circuit in an invalid state.
        #[test]
        fn concurrent_operations_safe(
            threshold in 10u32..50,
            num_threads in 2usize..8,
            ops_per_thread in 10usize..50,
        ) {
            let config = CircuitBreakerConfig {
                failure_threshold: threshold,
                ..Default::default()
            };
            let cb = Arc::new(CircuitBreaker::with_config(config));

            let handles: Vec<_> = (0..num_threads)
                .map(|i| {
                    let cb = Arc::clone(&cb);
                    thread::spawn(move || {
                        for j in 0..ops_per_thread {
                            // Mix of operations based on thread id and iteration
                            match (i + j) % 4 {
                                0 => cb.record_failure("shared"),
                                1 => cb.record_success("shared"),
                                2 => { let _ = cb.check_request("shared"); },
                                _ => { let _ = cb.get_state("shared"); },
                            }
                        }
                    })
                })
                .collect();

            // All threads should complete without panic
            for handle in handles {
                prop_assert!(handle.join().is_ok(), "Thread should not panic");
            }

            // State should still be valid
            let state = cb.get_state("shared");
            prop_assert!(
                matches!(
                    state,
                    CircuitState::Closed { .. }
                        | CircuitState::Open { .. }
                        | CircuitState::HalfOpen { .. }
                ),
                "State should be valid after concurrent operations"
            );
        }

        /// Invariant: Concurrent operations on different keys are isolated.
        #[test]
        fn concurrent_keys_isolated(
            keys in prop::collection::vec("[a-z]{3,8}".prop_map(|s| s), 3..6),
            threshold in 5u32..15,
        ) {
            let config = CircuitBreakerConfig {
                failure_threshold: threshold,
                timeout: Duration::from_secs(300),
                ..Default::default()
            };
            let cb = Arc::new(CircuitBreaker::with_config(config));

            // Each thread opens a different key
            // Note: .cloned() is needed because we move the key into the spawned thread
            #[allow(clippy::redundant_iter_cloned)]
            let handles: Vec<_> = keys
                .iter()
                .cloned()
                .map(|key| {
                    let cb = Arc::clone(&cb);
                    let t = threshold;
                    thread::spawn(move || {
                        for _ in 0..t {
                            cb.record_failure(&key);
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }

            // Each key should be independently open
            for key in &keys {
                prop_assert!(
                    cb.is_open(key),
                    "Key '{}' should be open after {} failures",
                    key,
                    threshold
                );
            }
        }
    }

    // ============================================================================
    // Edge Case Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Threshold of 1 opens on first failure.
        #[test]
        fn threshold_one_opens_immediately(key in key_strategy()) {
            let config = CircuitBreakerConfig {
                failure_threshold: 1,
                timeout: Duration::from_secs(300),
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            prop_assert!(!cb.is_open(&key), "Should not be open initially");

            cb.record_failure(&key);

            prop_assert!(
                cb.is_open(&key),
                "Should be open after single failure with threshold=1"
            );
        }

        /// Invariant: Success on new key is a no-op.
        ///
        /// Recording success for a key that was never used should not
        /// create any state entry or cause issues.
        #[test]
        fn success_on_new_key_is_noop(key in key_strategy()) {
            let cb = CircuitBreaker::new();

            // Should not panic
            cb.record_success(&key);

            // Should still have zero failures (no entry created)
            prop_assert_eq!(cb.failure_count(&key), 0);
        }

        /// Invariant: Failure count saturates instead of overflowing.
        ///
        /// Even with a very high threshold, failure_count should use
        /// saturating arithmetic and never overflow.
        #[test]
        fn failure_count_saturates(iterations in 100u32..1000) {
            let config = CircuitBreakerConfig {
                failure_threshold: u32::MAX,
                ..Default::default()
            };
            let cb = CircuitBreaker::with_config(config);

            for _ in 0..iterations {
                cb.record_failure("test");
            }

            // Should not panic and count should be accurate
            let count = cb.failure_count("test");
            prop_assert!(count > 0, "Should have recorded failures");
            prop_assert!(count <= iterations, "Count should not exceed iterations");
        }
    }

    // ============================================================================
    // Clone Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Clones share state.
        ///
        /// Cloning a CircuitBreaker should create a handle that shares
        /// the same underlying state.
        #[test]
        fn clones_share_state(
            failures in 1u32..10,
            threshold in 20u32..50, // High threshold so we stay in Closed
        ) {
            let config = CircuitBreakerConfig {
                failure_threshold: threshold,
                ..Default::default()
            };
            let cb1 = CircuitBreaker::with_config(config);
            let cb2 = cb1.clone();

            // Record failures on cb1
            for _ in 0..failures {
                cb1.record_failure("test");
            }

            // cb2 should see the same count
            prop_assert_eq!(
                cb2.failure_count("test"),
                failures,
                "Clones should share state"
            );

            // Record success on cb2 - this should reset the count
            // because there's already state for this key
            cb2.record_success("test");

            // cb1 should see the reset
            prop_assert_eq!(
                cb1.failure_count("test"),
                0,
                "State changes through clone should be visible"
            );
        }
    }
}
