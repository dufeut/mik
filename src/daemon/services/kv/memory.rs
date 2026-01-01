//! In-memory KV storage backend.
//!
//! Provides a fast, non-persistent key-value store using DashMap for
//! concurrent access. Ideal for testing, development, and embedded use cases.

use super::backend::KvBackend;
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use std::time::{Duration, Instant};

/// Entry stored in the memory backend with optional expiration.
#[derive(Clone)]
struct MemoryEntry {
    value: Vec<u8>,
    expires_at: Option<Instant>,
}

impl MemoryEntry {
    fn new(value: Vec<u8>, ttl: Option<Duration>) -> Self {
        Self {
            value,
            expires_at: ttl.map(|d| Instant::now() + d),
        }
    }

    fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| Instant::now() >= exp)
    }
}

/// In-memory key-value storage backend using DashMap.
///
/// Provides fast, concurrent access without persistence. All data is lost
/// when the process exits. Ideal for:
/// - Testing and development
/// - Embedded applications (Tauri, etc.)
/// - Temporary caching
///
/// # Thread Safety
///
/// `MemoryBackend` is `Clone` and uses `DashMap` internally for
/// lock-free concurrent access.
///
/// # Example
///
/// ```ignore
/// use mik::daemon::services::kv::MemoryBackend;
///
/// let backend = MemoryBackend::new();
/// backend.set("key", b"value".to_vec(), None).await?;
/// ```
#[derive(Clone, Default)]
pub struct MemoryBackend {
    data: DashMap<String, MemoryEntry>,
}

impl MemoryBackend {
    /// Creates a new empty in-memory backend.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of entries in the store (including expired).
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the store is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Removes all expired entries from the store.
    ///
    /// Call this periodically if you want proactive cleanup of expired
    /// entries. Otherwise, expired entries are cleaned up lazily on access.
    #[allow(dead_code)]
    pub fn cleanup_expired(&self) {
        self.data.retain(|_, entry| !entry.is_expired());
    }

    /// Clears all entries from the store.
    #[allow(dead_code)]
    pub fn clear(&self) {
        self.data.clear();
    }
}

#[async_trait]
impl KvBackend for MemoryBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        if let Some(entry) = self.data.get(key) {
            if entry.is_expired() {
                drop(entry);
                self.data.remove(key);
                Ok(None)
            } else {
                Ok(Some(entry.value.clone()))
            }
        } else {
            Ok(None)
        }
    }

    async fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<()> {
        let entry = MemoryEntry::new(value, ttl);
        self.data.insert(key.to_string(), entry);
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool> {
        Ok(self.data.remove(key).is_some())
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        let mut expired_keys = Vec::new();

        for entry in &self.data {
            let key = entry.key();

            // Filter by prefix if provided
            if let Some(prefix) = prefix
                && !key.starts_with(prefix)
            {
                continue;
            }

            if entry.value().is_expired() {
                expired_keys.push(key.clone());
            } else {
                keys.push(key.clone());
            }
        }

        // Clean up expired entries
        for key in expired_keys {
            self.data.remove(&key);
        }

        Ok(keys)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        if let Some(entry) = self.data.get(key) {
            if entry.is_expired() {
                drop(entry);
                self.data.remove(key);
                Ok(false)
            } else {
                Ok(true)
            }
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_get_set() {
        let backend = MemoryBackend::new();

        backend.set("key1", b"value1".to_vec(), None).await.unwrap();
        let value = backend.get("key1").await.unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let backend = MemoryBackend::new();
        let value = backend.get("nonexistent").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_delete() {
        let backend = MemoryBackend::new();

        backend.set("key1", b"value1".to_vec(), None).await.unwrap();
        let deleted = backend.delete("key1").await.unwrap();
        assert!(deleted);

        let value = backend.get("key1").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let backend = MemoryBackend::new();
        let deleted = backend.delete("nonexistent").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_list() {
        let backend = MemoryBackend::new();

        backend.set("a/1", b"v".to_vec(), None).await.unwrap();
        backend.set("a/2", b"v".to_vec(), None).await.unwrap();
        backend.set("b/1", b"v".to_vec(), None).await.unwrap();

        let mut all = backend.list(None).await.unwrap();
        all.sort();
        assert_eq!(all, vec!["a/1", "a/2", "b/1"]);

        let mut prefix_a = backend.list(Some("a/")).await.unwrap();
        prefix_a.sort();
        assert_eq!(prefix_a, vec!["a/1", "a/2"]);
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let backend = MemoryBackend::new();

        // Set with very short TTL
        backend
            .set(
                "expiring",
                b"value".to_vec(),
                Some(Duration::from_millis(10)),
            )
            .await
            .unwrap();

        // Should exist immediately
        let value = backend.get("expiring").await.unwrap();
        assert!(value.is_some());

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Should be gone now
        let value = backend.get("expiring").await.unwrap();
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn test_exists() {
        let backend = MemoryBackend::new();

        backend.set("key", b"value".to_vec(), None).await.unwrap();
        assert!(backend.exists("key").await.unwrap());
        assert!(!backend.exists("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_overwrite() {
        let backend = MemoryBackend::new();

        backend.set("key", b"value1".to_vec(), None).await.unwrap();
        backend.set("key", b"value2".to_vec(), None).await.unwrap();

        let value = backend.get("key").await.unwrap();
        assert_eq!(value, Some(b"value2".to_vec()));
    }
}
