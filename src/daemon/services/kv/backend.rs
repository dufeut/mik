//! Backend trait for the KV store.
//!
//! Defines the interface that all KV storage backends must implement,
//! enabling pluggable storage (redb, memory, Redis, etc.).

use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

/// Backend trait for key-value storage.
///
/// All backends must be thread-safe (`Send + Sync`) for use with tokio.
/// Implementations should handle their own concurrency and provide
/// appropriate ACID guarantees where applicable.
///
/// # Example
///
/// ```ignore
/// use mik::daemon::services::kv::{KvBackend, MemoryBackend};
///
/// let backend = MemoryBackend::new();
/// backend.set("key", b"value".to_vec(), None).await?;
/// let value = backend.get("key").await?;
/// ```
#[async_trait]
pub trait KvBackend: Send + Sync + 'static {
    /// Retrieves a value by key.
    ///
    /// Returns `Ok(None)` if the key doesn't exist or has expired.
    /// Implementations should automatically remove expired entries.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;

    /// Stores a key-value pair with an optional TTL.
    ///
    /// If `ttl` is `Some(duration)`, the entry will expire after the
    /// specified duration. If `None`, the entry never expires.
    ///
    /// Overwrites existing value if key already exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    async fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<()>;

    /// Deletes a key-value pair.
    ///
    /// Returns `Ok(true)` if the key existed and was removed,
    /// `Ok(false)` if it didn't exist. Idempotent - safe to call multiple times.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    async fn delete(&self, key: &str) -> Result<bool>;

    /// Lists all keys matching an optional prefix.
    ///
    /// If `prefix` is `Some(p)`, only keys starting with `p` are returned.
    /// If `None`, all keys are returned.
    ///
    /// Implementations should automatically skip expired entries.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<String>>;

    /// Checks if a key exists and has not expired.
    ///
    /// Default implementation uses `get()`, but backends may override
    /// for efficiency.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.get(key).await?.is_some())
    }
}
