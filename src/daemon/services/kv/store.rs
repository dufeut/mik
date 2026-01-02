//! High-level `KvStore` wrapper over backend implementations.
//!
//! Provides a convenient API that wraps any `KvBackend` implementation.

use super::backend::KvBackend;
use super::memory::MemoryBackend;
use super::redb::RedbBackend;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// High-level key-value store interface.
///
/// Wraps a `KvBackend` implementation and provides a consistent API
/// regardless of the underlying storage mechanism.
///
/// # Thread Safety
///
/// `KvStore` is `Clone` and can be shared across threads. The underlying
/// backend handles concurrent access safely.
///
/// # Example
///
/// ```ignore
/// use mik::daemon::services::kv::KvStore;
/// use std::time::Duration;
///
/// // Create an in-memory store
/// let store = KvStore::memory();
///
/// // Set a value with TTL
/// store.set("session:123", b"user_data", Some(Duration::from_secs(3600))).await?;
///
/// // Get the value
/// if let Some(data) = store.get("session:123").await? {
///     println!("Found: {} bytes", data.len());
/// }
/// ```
#[derive(Clone)]
pub struct KvStore {
    backend: Arc<dyn KvBackend>,
}

impl KvStore {
    /// Creates a new `KvStore` backed by a file-based redb database.
    ///
    /// This is the default for CLI usage where persistence is required.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or created.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let store = KvStore::file("~/.mik/kv.redb")?;
    /// ```
    pub fn file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let backend = RedbBackend::open(path)?;
        Ok(Self {
            backend: Arc::new(backend),
        })
    }

    /// Creates a new `KvStore` backed by an in-memory store.
    ///
    /// Ideal for testing, development, and embedded applications.
    /// All data is lost when the process exits.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let store = KvStore::memory();
    /// ```
    pub fn memory() -> Self {
        Self {
            backend: Arc::new(MemoryBackend::new()),
        }
    }

    /// Creates a new `KvStore` with a custom backend.
    ///
    /// Use this to integrate custom storage backends like Redis, PostgreSQL, etc.
    ///
    /// # Example
    ///
    /// ```ignore
    /// struct RedisBackend { /* ... */ }
    /// impl KvBackend for RedisBackend { /* ... */ }
    ///
    /// let store = KvStore::custom(RedisBackend::new());
    /// ```
    pub fn custom<B: KvBackend>(backend: B) -> Self {
        Self {
            backend: Arc::new(backend),
        }
    }

    /// Creates a new `KvStore` from a boxed backend.
    ///
    /// Useful when working with trait objects directly.
    pub fn from_boxed(backend: Box<dyn KvBackend>) -> Self {
        Self {
            backend: Arc::from(backend),
        }
    }

    /// Retrieves a value by key.
    ///
    /// Returns `Ok(None)` if the key doesn't exist or has expired.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.backend.get(key).await
    }

    /// Stores a key-value pair with an optional TTL.
    ///
    /// If `ttl` is `Some(duration)`, the entry will expire after the
    /// specified duration. If `None`, the entry never expires.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    pub async fn set(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<()> {
        self.backend.set(key, value.to_vec(), ttl).await
    }

    /// Deletes a key-value pair.
    ///
    /// Returns `Ok(true)` if the key existed, `Ok(false)` otherwise.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    pub async fn delete(&self, key: &str) -> Result<bool> {
        self.backend.delete(key).await
    }

    /// Lists all keys matching an optional prefix.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    pub async fn list_keys(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        self.backend.list(prefix).await
    }

    /// Checks if a key exists and has not expired.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying storage operation fails.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.backend.exists(key).await
    }
}
