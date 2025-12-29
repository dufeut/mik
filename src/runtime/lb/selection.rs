//! Load balancing selection algorithms.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::RwLock;

/// Trait for load balancing selection algorithms.
pub(super) trait Selection: Send + Sync {
    /// Select the next backend index from the available backends.
    fn select(&self, healthy_indices: &[usize]) -> Option<usize>;
}

/// Round-robin load balancing algorithm.
///
/// Distributes requests evenly across all healthy backends in a circular fashion.
#[derive(Debug)]
pub struct RoundRobin {
    /// Current position in the rotation.
    current: AtomicUsize,
    /// Total number of backends (used for modulo).
    total: usize,
}

impl RoundRobin {
    /// Create a new round-robin selector for the given number of backends.
    pub fn new(total: usize) -> Self {
        Self {
            current: AtomicUsize::new(0),
            total,
        }
    }

    /// Get the next index, wrapping around.
    fn next_index(&self) -> usize {
        if self.total == 0 {
            return 0;
        }
        self.current.fetch_add(1, Ordering::Relaxed) % self.total
    }
}

impl Selection for RoundRobin {
    fn select(&self, healthy_indices: &[usize]) -> Option<usize> {
        if healthy_indices.is_empty() {
            return None;
        }

        // Get the next position and map it to a healthy backend
        let pos = self.next_index();
        let healthy_pos = pos % healthy_indices.len();
        Some(healthy_indices[healthy_pos])
    }
}

/// Weighted round-robin load balancing algorithm.
///
/// Distributes requests across backends proportionally to their weights.
/// A backend with weight 2 will receive twice as many requests as a backend
/// with weight 1.
///
/// # Algorithm
///
/// Uses an expanded index approach where each backend appears in the rotation
/// a number of times equal to its weight. For example, with backends:
/// - Backend 0: weight 2
/// - Backend 1: weight 1
/// - Backend 2: weight 3
///
/// The virtual rotation is: [0, 0, 1, 2, 2, 2] and we cycle through it.
/// This ensures smooth distribution without bursts.
///
/// # Example
///
/// ```ignore
/// use mik::runtime::lb::WeightedRoundRobin;
///
/// // Backend 0 gets 2x traffic, backend 1 gets 1x traffic
/// let wrr = WeightedRoundRobin::new(vec![2, 1]);
/// ```
#[derive(Debug)]
#[allow(dead_code)]
pub(super) struct WeightedRoundRobin {
    /// Weights for each backend (index matches backend index).
    weights: Vec<u32>,
    /// Current position in the weighted rotation.
    current: AtomicUsize,
    /// Total weight sum (for wrapping).
    total_weight: usize,
}

impl WeightedRoundRobin {
    /// Create a new weighted round-robin selector with the given weights.
    ///
    /// Each weight corresponds to a backend at the same index.
    /// Weights of 0 are treated as 1.
    #[allow(dead_code)]
    pub(super) fn new(weights: Vec<u32>) -> Self {
        let weights: Vec<u32> = weights.into_iter().map(|w| w.max(1)).collect();
        let total_weight: usize = weights.iter().map(|&w| w as usize).sum();

        Self {
            weights,
            current: AtomicUsize::new(0),
            total_weight,
        }
    }

    /// Compute expanded indices for the given weights and backend indices.
    ///
    /// Creates a virtual index list where each backend appears proportionally
    /// to its weight.
    fn compute_expanded_indices(weights: &[u32], indices: Vec<usize>) -> Vec<usize> {
        let mut expanded = Vec::new();
        for (i, &idx) in indices.iter().enumerate() {
            let weight = weights.get(idx).copied().unwrap_or(1) as usize;
            for _ in 0..weight {
                expanded.push(i); // Push the position in the healthy_indices array
            }
        }
        expanded
    }

    /// Get the next weighted index.
    fn next_weighted_index(&self, healthy_expanded: &[usize]) -> usize {
        if healthy_expanded.is_empty() {
            return 0;
        }
        let pos = self.current.fetch_add(1, Ordering::Relaxed);
        pos % healthy_expanded.len()
    }

    /// Get the weight for a specific backend index.
    #[allow(dead_code)]
    pub(super) fn weight(&self, index: usize) -> u32 {
        self.weights.get(index).copied().unwrap_or(1)
    }

    /// Get the total weight of all backends.
    #[allow(dead_code)]
    pub(super) fn total_weight(&self) -> usize {
        self.total_weight
    }
}

impl Selection for WeightedRoundRobin {
    fn select(&self, healthy_indices: &[usize]) -> Option<usize> {
        if healthy_indices.is_empty() {
            return None;
        }

        // Build expanded indices for healthy backends only
        let healthy_expanded = Self::compute_expanded_indices(&self.weights, healthy_indices.to_vec());

        if healthy_expanded.is_empty() {
            return None;
        }

        // Get next position in the weighted rotation
        let pos = self.next_weighted_index(&healthy_expanded);

        // Map back to the original backend index
        let healthy_pos = healthy_expanded[pos];
        Some(healthy_indices[healthy_pos])
    }
}

/// Key extraction strategy for consistent hashing.
///
/// Determines how the hash key is derived from incoming requests.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub enum KeyExtractor {
    /// Use the request path as the hash key (default).
    /// This provides path-based affinity where requests to the same path
    /// are routed to the same backend.
    #[default]
    Path,

    /// Use a specific header value as the hash key.
    /// Useful for session affinity based on session IDs.
    /// Falls back to round-robin if the header is not present.
    Header(String),

    /// Use the client IP address as the hash key.
    /// Provides source IP affinity (client stickiness).
    ClientIp,
}

/// Consistent hashing load balancing algorithm.
///
/// Uses a hash ring with virtual nodes to distribute requests based on a key.
/// This provides:
/// - **Sticky sessions**: Same key always maps to the same backend (when healthy)
/// - **Minimal redistribution**: When backends are added/removed, only a fraction
///   of keys are remapped
/// - **Even distribution**: Virtual nodes ensure balanced load across backends
///
/// # Algorithm
///
/// 1. Each backend is placed on a ring at multiple positions (virtual nodes)
/// 2. The key is hashed to find its position on the ring
/// 3. Walk clockwise from that position to find the first backend
/// 4. If that backend is unhealthy, continue walking to find the next healthy one
///
/// # Example
///
/// ```ignore
/// use mik::runtime::lb::ConsistentHash;
///
/// // Create with 150 virtual nodes per backend (default)
/// let ch = ConsistentHash::new(150);
///
/// // Add backends
/// ch.add_backend("127.0.0.1:3001", 0);
/// ch.add_backend("127.0.0.1:3002", 1);
///
/// // Select by key
/// let healthy = vec![0, 1];
/// let selected = ch.select_by_key("/api/users/123", &healthy);
/// ```
#[derive(Debug)]
#[allow(dead_code)]
pub struct ConsistentHash {
    /// Number of virtual nodes per backend.
    virtual_nodes: usize,
    /// The hash ring storing all virtual nodes.
    /// BTreeMap provides O(log n) lookup and ordered iteration for ring walking.
    ring: RwLock<BTreeMap<u64, usize>>,
    /// Mapping from backend index to its address for virtual node generation.
    backends: RwLock<Vec<String>>,
    /// Fallback round-robin for when no key is available.
    fallback: AtomicUsize,
    /// Key extraction strategy.
    key_extractor: KeyExtractor,
}

#[allow(dead_code)]
impl ConsistentHash {
    /// Default number of virtual nodes per backend.
    /// 150 provides good distribution while keeping memory usage reasonable.
    pub const DEFAULT_VIRTUAL_NODES: usize = 150;

    /// Create a new consistent hash selector with the specified number of virtual nodes.
    ///
    /// # Arguments
    ///
    /// * `virtual_nodes` - Number of virtual nodes per backend. Higher values
    ///   provide better distribution but use more memory. Recommended: 100-200.
    pub fn new(virtual_nodes: usize) -> Self {
        Self {
            virtual_nodes: virtual_nodes.max(1),
            ring: RwLock::new(BTreeMap::new()),
            backends: RwLock::new(Vec::new()),
            fallback: AtomicUsize::new(0),
            key_extractor: KeyExtractor::default(),
        }
    }

    /// Create a new consistent hash selector with a specific key extractor.
    #[allow(dead_code)]
    pub fn with_key_extractor(virtual_nodes: usize, key_extractor: KeyExtractor) -> Self {
        Self {
            virtual_nodes: virtual_nodes.max(1),
            ring: RwLock::new(BTreeMap::new()),
            backends: RwLock::new(Vec::new()),
            fallback: AtomicUsize::new(0),
            key_extractor,
        }
    }

    /// Add a backend to the hash ring with virtual nodes.
    ///
    /// This places the backend at multiple positions on the ring
    /// to ensure even distribution.
    ///
    /// # Arguments
    ///
    /// * `address` - The backend address (e.g., "127.0.0.1:3001")
    /// * `index` - The backend's index in the backend list
    pub fn add_backend(&self, address: &str, index: usize) {
        let mut ring = self.ring.write();
        let mut backends = self.backends.write();

        // Ensure the backends vector is large enough
        if index >= backends.len() {
            backends.resize(index + 1, String::new());
        }
        backends[index] = address.to_string();

        // Add virtual nodes for this backend
        for i in 0..self.virtual_nodes {
            let hash = self.hash_virtual_node(address, i);
            ring.insert(hash, index);
        }
    }

    /// Remove a backend from the hash ring.
    ///
    /// This removes all virtual nodes for the specified backend.
    ///
    /// # Arguments
    ///
    /// * `address` - The backend address to remove
    #[allow(dead_code)]
    pub fn remove_backend(&self, address: &str) {
        let mut ring = self.ring.write();

        // Remove all virtual nodes for this backend
        for i in 0..self.virtual_nodes {
            let hash = self.hash_virtual_node(address, i);
            ring.remove(&hash);
        }
    }

    /// Select a backend based on the given key.
    ///
    /// The key is hashed to find its position on the ring, then we walk
    /// clockwise to find the first healthy backend.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to hash (e.g., request path, session ID)
    /// * `healthy_indices` - List of healthy backend indices
    ///
    /// # Returns
    ///
    /// The index of the selected backend, or `None` if no healthy backends.
    pub fn select_by_key(&self, key: &str, healthy_indices: &[usize]) -> Option<usize> {
        if healthy_indices.is_empty() {
            return None;
        }

        let ring = self.ring.read();
        if ring.is_empty() {
            // No backends in ring, fall back to round-robin
            return self.select_fallback(healthy_indices);
        }

        // Hash the key to find position on ring
        let key_hash = self.hash_key(key);

        // Find the first node at or after this position (clockwise search)
        // This is the core of consistent hashing - walk the ring until we find a healthy backend
        let mut found_backend = None;

        // First, try nodes >= key_hash
        for (&node_hash, &backend_index) in ring.range(key_hash..) {
            if healthy_indices.contains(&backend_index) {
                found_backend = Some(backend_index);
                break;
            }
            // Skip unhealthy backends and continue walking
            let _ = node_hash; // silence unused warning
        }

        // If not found, wrap around and try from the beginning
        if found_backend.is_none() {
            for (&node_hash, &backend_index) in ring.range(..key_hash) {
                if healthy_indices.contains(&backend_index) {
                    found_backend = Some(backend_index);
                    break;
                }
                let _ = node_hash;
            }
        }

        found_backend.or_else(|| self.select_fallback(healthy_indices))
    }

    /// Get the key extractor strategy.
    #[allow(dead_code)]
    pub fn key_extractor(&self) -> &KeyExtractor {
        &self.key_extractor
    }

    /// Get the number of virtual nodes per backend.
    #[allow(dead_code)]
    pub fn virtual_nodes(&self) -> usize {
        self.virtual_nodes
    }

    /// Get the total number of nodes on the ring.
    #[allow(dead_code)]
    pub fn ring_size(&self) -> usize {
        self.ring.read().len()
    }

    /// Hash a key to a position on the ring.
    fn hash_key(&self, key: &str) -> u64 {
        // Use blake3 for fast, high-quality hashing
        let hash = blake3::hash(key.as_bytes());
        // Take the first 8 bytes as u64
        let bytes: [u8; 8] = hash.as_bytes()[..8].try_into().unwrap();
        u64::from_le_bytes(bytes)
    }

    /// Hash a virtual node to a position on the ring.
    fn hash_virtual_node(&self, address: &str, virtual_index: usize) -> u64 {
        // Create a unique key for each virtual node
        let key = format!("{}#{}", address, virtual_index);
        self.hash_key(&key)
    }

    /// Fallback to round-robin selection when no key or ring is available.
    fn select_fallback(&self, healthy_indices: &[usize]) -> Option<usize> {
        if healthy_indices.is_empty() {
            return None;
        }
        let pos = self.fallback.fetch_add(1, Ordering::Relaxed);
        Some(healthy_indices[pos % healthy_indices.len()])
    }
}

impl Selection for ConsistentHash {
    /// Select using round-robin fallback.
    ///
    /// When using consistent hashing, you typically want to use `select_by_key`
    /// with a specific key. This fallback uses round-robin for when no key
    /// is available (e.g., health checks or internal requests).
    fn select(&self, healthy_indices: &[usize]) -> Option<usize> {
        self.select_fallback(healthy_indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_robin_basic() {
        let rr = RoundRobin::new(3);
        let healthy = vec![0, 1, 2];

        // Should cycle through all backends
        let mut selections = Vec::new();
        for _ in 0..6 {
            selections.push(rr.select(&healthy).unwrap());
        }

        // Each backend should be selected twice in 6 iterations
        assert_eq!(selections.iter().filter(|&&x| x == 0).count(), 2);
        assert_eq!(selections.iter().filter(|&&x| x == 1).count(), 2);
        assert_eq!(selections.iter().filter(|&&x| x == 2).count(), 2);
    }

    #[test]
    fn test_round_robin_with_unhealthy() {
        let rr = RoundRobin::new(3);
        // Only backends 0 and 2 are healthy
        let healthy = vec![0, 2];

        let mut selections = Vec::new();
        for _ in 0..4 {
            selections.push(rr.select(&healthy).unwrap());
        }

        // Should only select from healthy backends
        for &idx in &selections {
            assert!(idx == 0 || idx == 2);
        }
    }

    #[test]
    fn test_round_robin_empty() {
        let rr = RoundRobin::new(3);
        let healthy: Vec<usize> = vec![];

        assert!(rr.select(&healthy).is_none());
    }

    #[test]
    fn test_round_robin_single() {
        let rr = RoundRobin::new(1);
        let healthy = vec![0];

        // Should always return 0
        for _ in 0..5 {
            assert_eq!(rr.select(&healthy), Some(0));
        }
    }

    // ============ WeightedRoundRobin Tests ============

    #[test]
    fn test_weighted_round_robin_basic() {
        // Backend 0: weight 2, Backend 1: weight 1
        // Expected ratio: 2:1 (backend 0 gets 2x traffic)
        let wrr = WeightedRoundRobin::new(vec![2, 1]);
        let healthy = vec![0, 1];

        let mut counts = [0usize; 2];
        // Run enough iterations to see the pattern
        for _ in 0..300 {
            let idx = wrr.select(&healthy).unwrap();
            counts[idx] += 1;
        }

        // Backend 0 should have ~2x the selections of backend 1
        // With weights [2, 1], total weight is 3
        // Backend 0 should get 2/3 of traffic, backend 1 should get 1/3
        // In 300 requests: backend 0 ~200, backend 1 ~100
        assert!(
            counts[0] > counts[1],
            "Backend 0 (weight 2) should have more selections than backend 1 (weight 1): {:?}",
            counts
        );

        // Allow some variance but the ratio should be roughly 2:1
        let ratio = counts[0] as f64 / counts[1] as f64;
        assert!(
            (1.5..2.5).contains(&ratio),
            "Expected ratio ~2.0, got {}: {:?}",
            ratio,
            counts
        );
    }

    #[test]
    fn test_weighted_round_robin_equal_weights() {
        // All backends have equal weight - should behave like regular round-robin
        let wrr = WeightedRoundRobin::new(vec![1, 1, 1]);
        let healthy = vec![0, 1, 2];

        let mut counts = [0usize; 3];
        for _ in 0..300 {
            let idx = wrr.select(&healthy).unwrap();
            counts[idx] += 1;
        }

        // Each backend should get roughly equal traffic
        for (i, &count) in counts.iter().enumerate() {
            assert!(
                (80..=120).contains(&count),
                "Backend {} should have ~100 selections, got {}",
                i,
                count
            );
        }
    }

    #[test]
    fn test_weighted_round_robin_with_unhealthy() {
        // Backend 0: weight 3, Backend 1: weight 2, Backend 2: weight 1
        let wrr = WeightedRoundRobin::new(vec![3, 2, 1]);

        // Only backends 0 and 2 are healthy
        let healthy = vec![0, 2];

        let mut counts = [0usize; 3];
        for _ in 0..400 {
            let idx = wrr.select(&healthy).unwrap();
            counts[idx] += 1;
        }

        // Backend 1 should have 0 selections (unhealthy)
        assert_eq!(counts[1], 0, "Unhealthy backend should not be selected");

        // Backend 0 (weight 3) should have 3x the selections of backend 2 (weight 1)
        let ratio = counts[0] as f64 / counts[2] as f64;
        assert!(
            (2.5..3.5).contains(&ratio),
            "Expected ratio ~3.0, got {}: {:?}",
            ratio,
            counts
        );
    }

    #[test]
    fn test_weighted_round_robin_empty() {
        let wrr = WeightedRoundRobin::new(vec![1, 2, 3]);
        let healthy: Vec<usize> = vec![];

        assert!(wrr.select(&healthy).is_none());
    }

    #[test]
    fn test_weighted_round_robin_single() {
        let wrr = WeightedRoundRobin::new(vec![5]);
        let healthy = vec![0];

        // Should always return 0
        for _ in 0..10 {
            assert_eq!(wrr.select(&healthy), Some(0));
        }
    }

    #[test]
    fn test_weighted_round_robin_zero_weight_treated_as_one() {
        // Weight of 0 should be treated as 1
        let wrr = WeightedRoundRobin::new(vec![0, 1]);

        assert_eq!(wrr.weight(0), 1); // 0 becomes 1
        assert_eq!(wrr.weight(1), 1);
        assert_eq!(wrr.total_weight(), 2);
    }

    #[test]
    fn test_weighted_round_robin_high_weights() {
        // Test with higher weights
        // Backend 0: weight 10, Backend 1: weight 1
        let wrr = WeightedRoundRobin::new(vec![10, 1]);
        let healthy = vec![0, 1];

        let mut counts = [0usize; 2];
        for _ in 0..1100 {
            let idx = wrr.select(&healthy).unwrap();
            counts[idx] += 1;
        }

        // Backend 0 should get ~10x traffic
        let ratio = counts[0] as f64 / counts[1] as f64;
        assert!(
            (8.0..12.0).contains(&ratio),
            "Expected ratio ~10.0, got {}: {:?}",
            ratio,
            counts
        );
    }

    #[test]
    fn test_weighted_round_robin_three_backends() {
        // Backend 0: weight 1, Backend 1: weight 2, Backend 2: weight 3
        // Total weight: 6
        // Expected distribution: 1/6, 2/6, 3/6
        let wrr = WeightedRoundRobin::new(vec![1, 2, 3]);
        let healthy = vec![0, 1, 2];

        let mut counts = [0usize; 3];
        for _ in 0..600 {
            let idx = wrr.select(&healthy).unwrap();
            counts[idx] += 1;
        }

        // Backend 0: ~100, Backend 1: ~200, Backend 2: ~300
        assert!(
            counts[0] < counts[1] && counts[1] < counts[2],
            "Counts should increase with weights: {:?}",
            counts
        );

        // Check approximate ratios
        let ratio_1_0 = counts[1] as f64 / counts[0] as f64;
        let ratio_2_0 = counts[2] as f64 / counts[0] as f64;

        assert!(
            (1.5..2.5).contains(&ratio_1_0),
            "Expected ratio 1:0 ~2.0, got {}",
            ratio_1_0
        );
        assert!(
            (2.5..3.5).contains(&ratio_2_0),
            "Expected ratio 2:0 ~3.0, got {}",
            ratio_2_0
        );
    }

    #[test]
    fn test_weighted_round_robin_weight_accessor() {
        let wrr = WeightedRoundRobin::new(vec![5, 3, 7]);

        assert_eq!(wrr.weight(0), 5);
        assert_eq!(wrr.weight(1), 3);
        assert_eq!(wrr.weight(2), 7);
        assert_eq!(wrr.weight(99), 1); // Out of bounds returns default 1
        assert_eq!(wrr.total_weight(), 15);
    }

    // ============ ConsistentHash Tests ============

    #[test]
    fn test_consistent_hash_new() {
        let ch = ConsistentHash::new(150);
        assert_eq!(ch.virtual_nodes(), 150);
        assert_eq!(ch.ring_size(), 0);
    }

    #[test]
    fn test_consistent_hash_add_backend() {
        let ch = ConsistentHash::new(100);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);

        // Each backend should add 100 virtual nodes
        assert_eq!(ch.ring_size(), 200);
    }

    #[test]
    fn test_consistent_hash_remove_backend() {
        let ch = ConsistentHash::new(100);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);
        assert_eq!(ch.ring_size(), 200);

        ch.remove_backend("127.0.0.1:3001");
        assert_eq!(ch.ring_size(), 100);

        ch.remove_backend("127.0.0.1:3002");
        assert_eq!(ch.ring_size(), 0);
    }

    #[test]
    fn test_consistent_hash_same_key_same_backend() {
        // Core property: same key should always map to the same backend (when healthy)
        let ch = ConsistentHash::new(150);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);
        ch.add_backend("127.0.0.1:3003", 2);

        let healthy = vec![0, 1, 2];
        let key = "/api/users/123";

        // Same key should always return the same backend
        let first_selection = ch.select_by_key(key, &healthy).unwrap();
        for _ in 0..100 {
            let selection = ch.select_by_key(key, &healthy).unwrap();
            assert_eq!(
                selection, first_selection,
                "Same key should always map to the same backend"
            );
        }
    }

    #[test]
    fn test_consistent_hash_different_keys_distribute() {
        // Keys should distribute across backends
        let ch = ConsistentHash::new(150);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);
        ch.add_backend("127.0.0.1:3003", 2);

        let healthy = vec![0, 1, 2];
        let mut counts = [0usize; 3];

        // Generate many different keys
        for i in 0..3000 {
            let key = format!("/api/resource/{}", i);
            let idx = ch.select_by_key(&key, &healthy).unwrap();
            counts[idx] += 1;
        }

        // Each backend should get a reasonable share (roughly 1000 each)
        // Allow for some variance: 600-1400 range
        for (i, &count) in counts.iter().enumerate() {
            assert!(
                (600..=1400).contains(&count),
                "Backend {} should have roughly even distribution, got {}: {:?}",
                i,
                count,
                counts
            );
        }
    }

    #[test]
    fn test_consistent_hash_graceful_failover() {
        // When a backend becomes unhealthy, requests should go to the next healthy backend
        let ch = ConsistentHash::new(150);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);
        ch.add_backend("127.0.0.1:3003", 2);

        let key = "/api/session/abc";

        // All healthy - get initial selection
        let all_healthy = vec![0, 1, 2];
        let original = ch.select_by_key(key, &all_healthy).unwrap();

        // Make the original backend unhealthy
        let healthy_without_original: Vec<usize> = all_healthy
            .iter()
            .copied()
            .filter(|&x| x != original)
            .collect();

        // Should failover to a different backend
        let failover = ch.select_by_key(key, &healthy_without_original).unwrap();
        assert_ne!(
            failover, original,
            "Should failover to a different backend"
        );
        assert!(
            healthy_without_original.contains(&failover),
            "Failover target should be healthy"
        );

        // Same key should consistently go to the failover backend
        for _ in 0..10 {
            let selection = ch.select_by_key(key, &healthy_without_original).unwrap();
            assert_eq!(
                selection, failover,
                "Should consistently use the failover backend"
            );
        }
    }

    #[test]
    fn test_consistent_hash_recovery() {
        // When a backend recovers, it should receive its original keys again
        let ch = ConsistentHash::new(150);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);
        ch.add_backend("127.0.0.1:3003", 2);

        let key = "/api/data/xyz";
        let all_healthy = vec![0, 1, 2];

        // Get original selection
        let original = ch.select_by_key(key, &all_healthy).unwrap();

        // Simulate backend failure
        let healthy_without_original: Vec<usize> = all_healthy
            .iter()
            .copied()
            .filter(|&x| x != original)
            .collect();
        let _failover = ch.select_by_key(key, &healthy_without_original).unwrap();

        // Simulate recovery - should return to original backend
        let after_recovery = ch.select_by_key(key, &all_healthy).unwrap();
        assert_eq!(
            after_recovery, original,
            "After recovery, key should map back to original backend"
        );
    }

    #[test]
    fn test_consistent_hash_minimal_redistribution_on_add() {
        // Adding a backend should only affect a portion of keys
        let ch = ConsistentHash::new(150);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);

        let healthy_2 = vec![0, 1];

        // Record initial mappings for many keys
        let mut initial_mappings = Vec::new();
        for i in 0..1000 {
            let key = format!("/key/{}", i);
            let backend = ch.select_by_key(&key, &healthy_2).unwrap();
            initial_mappings.push((key, backend));
        }

        // Add a third backend
        ch.add_backend("127.0.0.1:3003", 2);
        let healthy_3 = vec![0, 1, 2];

        // Count how many keys moved
        let mut moved = 0;
        for (key, original_backend) in &initial_mappings {
            let new_backend = ch.select_by_key(key, &healthy_3).unwrap();
            if *original_backend != new_backend {
                moved += 1;
            }
        }

        // Ideally, only about 1/3 of keys should move (the new backend's share)
        // Allow some variance: 200-500 moved (20%-50%)
        assert!(
            (200..=500).contains(&moved),
            "Adding a backend should cause minimal redistribution. Moved: {} out of 1000",
            moved
        );
    }

    #[test]
    fn test_consistent_hash_minimal_redistribution_on_remove() {
        // Removing a backend should only redistribute its keys
        let ch = ConsistentHash::new(150);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);
        ch.add_backend("127.0.0.1:3003", 2);

        let healthy_3 = vec![0, 1, 2];

        // Record initial mappings and count per backend
        let mut mappings: Vec<(String, usize)> = Vec::new();
        let mut counts = [0usize; 3];
        for i in 0..1000 {
            let key = format!("/resource/{}", i);
            let backend = ch.select_by_key(&key, &healthy_3).unwrap();
            mappings.push((key, backend));
            counts[backend] += 1;
        }

        // Remove backend 1
        ch.remove_backend("127.0.0.1:3002");
        let healthy_2 = vec![0, 2];

        // Count how many keys that were NOT on backend 1 changed
        let mut stable_keys_that_moved = 0;
        for (key, original_backend) in &mappings {
            if *original_backend != 1 {
                let new_backend = ch.select_by_key(key, &healthy_2).unwrap();
                if *original_backend != new_backend {
                    stable_keys_that_moved += 1;
                }
            }
        }

        // Keys that were not on the removed backend should mostly stay put
        // Allow for some small movement due to ring dynamics
        let keys_not_on_removed = 1000 - counts[1];
        let movement_ratio = stable_keys_that_moved as f64 / keys_not_on_removed as f64;
        assert!(
            movement_ratio < 0.1,
            "Keys not on removed backend should mostly stay stable. Movement ratio: {:.2}%",
            movement_ratio * 100.0
        );
    }

    #[test]
    fn test_consistent_hash_empty_healthy() {
        let ch = ConsistentHash::new(150);
        ch.add_backend("127.0.0.1:3001", 0);

        let healthy: Vec<usize> = vec![];
        assert!(ch.select_by_key("/test", &healthy).is_none());
    }

    #[test]
    fn test_consistent_hash_single_backend() {
        let ch = ConsistentHash::new(150);
        ch.add_backend("127.0.0.1:3001", 0);

        let healthy = vec![0];

        // All keys should go to the only backend
        for i in 0..100 {
            let key = format!("/path/{}", i);
            assert_eq!(ch.select_by_key(&key, &healthy), Some(0));
        }
    }

    #[test]
    fn test_consistent_hash_fallback_selection() {
        // The Selection trait's select() should use round-robin fallback
        let ch = ConsistentHash::new(150);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);

        let healthy = vec![0, 1];

        // Should cycle through backends
        let mut counts = [0usize; 2];
        for _ in 0..100 {
            let idx = ch.select(&healthy).unwrap();
            counts[idx] += 1;
        }

        // Should be roughly even
        assert!(
            (40..=60).contains(&counts[0]) && (40..=60).contains(&counts[1]),
            "Fallback should distribute evenly: {:?}",
            counts
        );
    }

    #[test]
    fn test_consistent_hash_key_extractor_default() {
        let ch = ConsistentHash::new(150);
        assert!(matches!(ch.key_extractor(), KeyExtractor::Path));
    }

    #[test]
    fn test_consistent_hash_with_header_extractor() {
        let ch = ConsistentHash::with_key_extractor(
            150,
            KeyExtractor::Header("X-Session-ID".to_string()),
        );
        assert!(matches!(ch.key_extractor(), KeyExtractor::Header(_)));

        if let KeyExtractor::Header(header) = ch.key_extractor() {
            assert_eq!(header, "X-Session-ID");
        }
    }

    #[test]
    fn test_consistent_hash_with_client_ip_extractor() {
        let ch = ConsistentHash::with_key_extractor(150, KeyExtractor::ClientIp);
        assert!(matches!(ch.key_extractor(), KeyExtractor::ClientIp));
    }

    #[test]
    fn test_consistent_hash_virtual_nodes_minimum() {
        // Virtual nodes should be at least 1
        let ch = ConsistentHash::new(0);
        assert_eq!(ch.virtual_nodes(), 1);
    }

    #[test]
    fn test_consistent_hash_empty_ring_fallback() {
        // When ring is empty, should fall back to round-robin
        let ch = ConsistentHash::new(150);
        // Don't add any backends to the ring

        let healthy = vec![0, 1, 2];

        // Should still work using fallback
        let mut counts = [0usize; 3];
        for _ in 0..300 {
            let idx = ch.select_by_key("/some/path", &healthy).unwrap();
            counts[idx] += 1;
        }

        // Should cycle through healthy indices
        for (i, &count) in counts.iter().enumerate() {
            assert!(
                (80..=120).contains(&count),
                "Fallback should distribute evenly, backend {} got {}",
                i,
                count
            );
        }
    }

    #[test]
    fn test_consistent_hash_high_virtual_nodes() {
        // Test with high virtual node count
        let ch = ConsistentHash::new(500);
        ch.add_backend("127.0.0.1:3001", 0);
        ch.add_backend("127.0.0.1:3002", 1);

        assert_eq!(ch.ring_size(), 1000);

        let healthy = vec![0, 1];
        let mut counts = [0usize; 2];

        for i in 0..2000 {
            let key = format!("/high/{}", i);
            let idx = ch.select_by_key(&key, &healthy).unwrap();
            counts[idx] += 1;
        }

        // Should be well distributed
        assert!(
            (800..=1200).contains(&counts[0]) && (800..=1200).contains(&counts[1]),
            "High virtual nodes should provide good distribution: {:?}",
            counts
        );
    }

    #[test]
    fn test_consistent_hash_deterministic_across_instances() {
        // Two separate instances with same config should produce same results
        let ch1 = ConsistentHash::new(150);
        ch1.add_backend("127.0.0.1:3001", 0);
        ch1.add_backend("127.0.0.1:3002", 1);

        let ch2 = ConsistentHash::new(150);
        ch2.add_backend("127.0.0.1:3001", 0);
        ch2.add_backend("127.0.0.1:3002", 1);

        let healthy = vec![0, 1];

        // Same keys should map to same backends
        for i in 0..100 {
            let key = format!("/deterministic/{}", i);
            let result1 = ch1.select_by_key(&key, &healthy).unwrap();
            let result2 = ch2.select_by_key(&key, &healthy).unwrap();
            assert_eq!(
                result1, result2,
                "Key '{}' should map identically across instances",
                key
            );
        }
    }
}
