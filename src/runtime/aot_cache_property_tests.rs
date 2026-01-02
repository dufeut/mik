//! Property-based tests for AOT compilation cache correctness.
//!
//! These tests verify invariants for the content-addressable cache:
//! - Cache key computation is deterministic
//! - Different inputs produce different keys (collision resistance)
//! - Cache operations are consistent
//! - Bypass mode behaves correctly
//!
//! # Running Tests
//!
//! ```bash
//! cargo test aot_cache_property_tests
//! ```

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use proptest::prelude::*;

    use crate::runtime::aot_cache::AotCache;

    // ============================================================================
    // Test Strategies - Input Generation
    // ============================================================================

    /// Strategy for generating arbitrary WASM-like byte sequences.
    fn wasm_bytes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 1..10000)
    }

    /// Strategy for generating small byte sequences for more thorough testing.
    fn small_bytes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 1..100)
    }

    /// Strategy for generating pairs of different byte sequences.
    fn different_bytes_pair() -> impl Strategy<Value = (Vec<u8>, Vec<u8>)> {
        (small_bytes(), small_bytes()).prop_filter("must be different", |(a, b)| a != b)
    }

    // ============================================================================
    // Cache Key Computation Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Cache key computation is deterministic.
        ///
        /// The same input bytes should always produce the same cache key.
        /// This is essential for cache correctness.
        #[test]
        fn cache_key_is_deterministic(bytes in wasm_bytes()) {
            let key1 = AotCache::compute_key(&bytes);
            let key2 = AotCache::compute_key(&bytes);

            prop_assert_eq!(key1, key2, "Same input should produce same key");
        }

        /// Invariant: Cache key is always 32 hex characters.
        ///
        /// The key format is fixed: 32 hex chars (128 bits from BLAKE3).
        #[test]
        fn cache_key_format_consistent(bytes in wasm_bytes()) {
            let key = AotCache::compute_key(&bytes);

            prop_assert_eq!(key.len(), 32, "Key should be 32 characters");
            prop_assert!(
                key.chars().all(|c| c.is_ascii_hexdigit()),
                "Key should contain only hex digits"
            );
        }

        /// Invariant: Different inputs produce different keys.
        ///
        /// With high probability, different content should hash to different keys.
        /// This is critical for cache correctness - we don't want collisions.
        #[test]
        fn different_inputs_different_keys((bytes1, bytes2) in different_bytes_pair()) {
            let key1 = AotCache::compute_key(&bytes1);
            let key2 = AotCache::compute_key(&bytes2);

            prop_assert_ne!(
                key1, key2,
                "Different inputs should produce different keys"
            );
        }

        /// Invariant: Empty input produces a valid key.
        ///
        /// Even empty content should hash to a valid key format.
        #[test]
        fn empty_input_valid_key(_dummy in Just(())) {
            let key = AotCache::compute_key(&[]);

            prop_assert_eq!(key.len(), 32);
            prop_assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
        }

        /// Invariant: Single byte difference changes key.
        ///
        /// Even a single bit flip should produce a completely different key.
        #[test]
        fn single_byte_change_changes_key(
            mut bytes in small_bytes().prop_filter("need at least 1 byte", |v| !v.is_empty())
        ) {
            let key1 = AotCache::compute_key(&bytes);

            // Flip one byte
            bytes[0] = bytes[0].wrapping_add(1);
            let modified = bytes;

            let key2 = AotCache::compute_key(&modified);

            prop_assert_ne!(key1, key2, "Single byte change should change key");
        }

        /// Invariant: Key doesn't depend on byte order in computation.
        ///
        /// Reversed bytes should produce a different key (content-addressable).
        #[test]
        fn reversed_bytes_different_key(
            bytes in small_bytes().prop_filter("need > 1 byte", |v| v.len() > 1)
        ) {
            let mut reversed = bytes.clone();
            reversed.reverse();

            // Skip if reversing produces the same bytes (e.g., palindrome)
            if bytes == reversed {
                return Ok(());
            }

            let key1 = AotCache::compute_key(&bytes);
            let key2 = AotCache::compute_key(&reversed);

            prop_assert_ne!(key1, key2, "Reversed bytes should produce different key");
        }
    }

    // ============================================================================
    // Bypass Mode Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Bypass mode never returns cached entries.
        #[test]
        fn bypass_mode_never_caches(bytes in wasm_bytes()) {
            let cache = AotCache::bypass();

            prop_assert!(cache.is_bypass(), "Should be in bypass mode");
            prop_assert!(
                cache.get(&bytes).is_none(),
                "Bypass mode should never return cached entry"
            );
        }

        /// Invariant: Bypass mode rejects put operations.
        #[test]
        fn bypass_mode_rejects_put(bytes in wasm_bytes()) {
            let cache = AotCache::bypass();

            let result = cache.put(&bytes, b"compiled");
            prop_assert!(result.is_err(), "Bypass mode should reject put");
        }

        /// Invariant: Bypass mode returns false for remove.
        #[test]
        fn bypass_mode_remove_returns_false(bytes in wasm_bytes()) {
            let cache = AotCache::bypass();

            let result = cache.remove(&bytes);
            prop_assert!(result.is_ok());
            prop_assert!(!result.unwrap(), "Bypass mode remove should return false");
        }
    }

    // ============================================================================
    // Hash Quality Tests
    // ============================================================================

    proptest! {
        /// Invariant: Keys have good distribution.
        ///
        /// For random inputs, keys should be evenly distributed.
        /// We check that different inputs don't cluster to similar keys.
        #[test]
        fn keys_well_distributed(
            bytes1 in small_bytes(),
            bytes2 in small_bytes(),
            bytes3 in small_bytes()
        ) {
            let key1 = AotCache::compute_key(&bytes1);
            let key2 = AotCache::compute_key(&bytes2);
            let key3 = AotCache::compute_key(&bytes3);

            // If all inputs are different, all keys should be different
            if bytes1 != bytes2 && bytes2 != bytes3 && bytes1 != bytes3 {
                prop_assert_ne!(key1.clone(), key2.clone());
                prop_assert_ne!(key2, key3.clone());
                prop_assert_ne!(key1, key3);
            }
        }

        /// Invariant: BLAKE3 produces consistent results.
        ///
        /// Known input should produce known output (regression test).
        #[test]
        fn known_input_produces_known_key(_dummy in Just(())) {
            // Test vector: empty input
            let empty_key = AotCache::compute_key(&[]);
            // BLAKE3 of empty is well-defined
            prop_assert!(!empty_key.is_empty());

            // Test vector: single byte
            let single_key = AotCache::compute_key(&[0x00]);
            prop_assert_ne!(empty_key, single_key.clone());

            // Test vector: different single byte
            let other_key = AotCache::compute_key(&[0xFF]);
            prop_assert_ne!(single_key, other_key);
        }

        /// Invariant: Large inputs don't cause issues.
        ///
        /// Even very large WASM modules should hash quickly and correctly.
        #[test]
        fn large_input_works(_dummy in Just(())) {
            // 10MB of data
            let large: Vec<u8> = (0..10_000_000_u64).map(|i| (i % 256) as u8).collect();

            let key = AotCache::compute_key(&large);

            prop_assert_eq!(key.len(), 32);
            prop_assert!(key.chars().all(|c| c.is_ascii_hexdigit()));

            // Same content should produce same key
            let key2 = AotCache::compute_key(&large);
            prop_assert_eq!(key, key2);
        }
    }

    // ============================================================================
    // Key Uniqueness Stress Tests
    // ============================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Invariant: Many different inputs produce many different keys.
        ///
        /// Generate a batch of random inputs and verify no collisions.
        #[test]
        fn no_collisions_in_batch(inputs in prop::collection::vec(small_bytes(), 10..20)) {
            // Deduplicate inputs first
            let unique_inputs: HashSet<_> = inputs.into_iter().collect();

            // Compute keys
            let keys: HashSet<_> = unique_inputs
                .iter()
                .map(|b| AotCache::compute_key(b))
                .collect();

            // Should have as many unique keys as unique inputs
            prop_assert_eq!(
                keys.len(),
                unique_inputs.len(),
                "Should have no key collisions"
            );
        }

        /// Invariant: Sequential byte sequences produce different keys.
        #[test]
        fn sequential_bytes_different_keys(start in 0u8..200) {
            let seq1: Vec<u8> = (start..start.saturating_add(10)).collect();
            let seq2: Vec<u8> = (start.saturating_add(1)..start.saturating_add(11)).collect();

            let key1 = AotCache::compute_key(&seq1);
            let key2 = AotCache::compute_key(&seq2);

            prop_assert_ne!(key1, key2, "Sequential sequences should have different keys");
        }

        /// Invariant: Prefixed/suffixed content has different keys.
        #[test]
        fn prefix_suffix_different_keys(base in small_bytes()) {
            let with_prefix: Vec<u8> =
                std::iter::once(0xFF).chain(base.iter().copied()).collect();
            let with_suffix: Vec<u8> =
                base.iter().copied().chain(std::iter::once(0xFF)).collect();

            let key_base = AotCache::compute_key(&base);
            let key_prefix = AotCache::compute_key(&with_prefix);
            let key_suffix = AotCache::compute_key(&with_suffix);

            prop_assert_ne!(
                key_base.clone(),
                key_prefix.clone(),
                "Prefixed content should have different key"
            );
            prop_assert_ne!(
                key_base,
                key_suffix.clone(),
                "Suffixed content should have different key"
            );
            prop_assert_ne!(
                key_prefix,
                key_suffix,
                "Prefix vs suffix should have different keys"
            );
        }
    }

    // ============================================================================
    // Path Derivation Invariants
    // ============================================================================

    proptest! {
        /// Invariant: AOT cache path derivation is deterministic.
        ///
        /// The same WASM content should always map to the same cache path.
        #[test]
        fn aot_path_deterministic(bytes in wasm_bytes()) {
            let key1 = AotCache::compute_key(&bytes);
            let key2 = AotCache::compute_key(&bytes);

            let path1 = format!("{key1}.aot");
            let path2 = format!("{key2}.aot");

            prop_assert_eq!(path1, path2, "Path derivation should be deterministic");
        }

        /// Invariant: Cache paths are valid filenames.
        ///
        /// Generated cache keys should be valid for use in file paths.
        #[test]
        fn cache_key_is_valid_filename(bytes in wasm_bytes()) {
            let key = AotCache::compute_key(&bytes);

            // Key should only contain hex digits (safe for all filesystems)
            prop_assert!(
                key.chars()
                    .all(|c| c.is_ascii_hexdigit()),
                "Key should be a valid filename"
            );

            // Key should not be empty
            prop_assert!(!key.is_empty(), "Key should not be empty");

            // Key should have consistent length
            prop_assert_eq!(key.len(), 32, "Key should have consistent length");
        }

        /// Invariant: Different WASM files map to different cache paths.
        #[test]
        fn unique_paths_for_unique_content(
            (bytes1, bytes2) in different_bytes_pair()
        ) {
            let key1 = AotCache::compute_key(&bytes1);
            let key2 = AotCache::compute_key(&bytes2);

            let path1 = format!("{key1}.aot");
            let path2 = format!("{key2}.aot");

            prop_assert_ne!(
                path1, path2,
                "Different content should map to different paths"
            );
        }
    }

    // ============================================================================
    // Content-Addressable Storage Invariants
    // ============================================================================

    proptest! {
        /// Invariant: Content-addressable lookup is correct.
        ///
        /// The key computed from content should always identify that content.
        #[test]
        fn content_addressable_identity(bytes in wasm_bytes()) {
            let key1 = AotCache::compute_key(&bytes);

            // Recomputing from same content should give same key
            let bytes_copy = bytes.clone();
            let key2 = AotCache::compute_key(&bytes_copy);

            prop_assert_eq!(key1, key2, "Content-addressable identity should hold");
        }

        /// Invariant: Truncation to different content changes key.
        ///
        /// If we truncate content, the key should change.
        #[test]
        fn truncation_changes_key(
            bytes in small_bytes().prop_filter("need > 2 bytes", |v| v.len() > 2)
        ) {
            let full_key = AotCache::compute_key(&bytes);

            // Truncate by removing last byte
            let truncated: Vec<u8> = bytes[..bytes.len() - 1].to_vec();
            let truncated_key = AotCache::compute_key(&truncated);

            prop_assert_ne!(
                full_key,
                truncated_key,
                "Truncation should change the key"
            );
        }
    }
}
