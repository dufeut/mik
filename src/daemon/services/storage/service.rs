//! High-level `StorageService` wrapper over backend implementations.
//!
//! Provides a convenient API that wraps any `StorageBackend` implementation.

use super::backend::StorageBackend;
use super::filesystem::FilesystemBackend;
use super::memory::MemoryStorageBackend;
use super::types::ObjectMeta;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

/// High-level object storage service interface.
///
/// Wraps a `StorageBackend` implementation and provides a consistent API
/// regardless of the underlying storage mechanism.
///
/// # Thread Safety
///
/// `StorageService` is `Clone` and can be shared across threads. The underlying
/// backend handles concurrent access safely.
///
/// # Example
///
/// ```ignore
/// use mik::daemon::services::storage::StorageService;
///
/// // Create an in-memory store
/// let storage = StorageService::memory();
///
/// // Store an object
/// let meta = storage.put_object("images/logo.png", &image_bytes, Some("image/png")).await?;
///
/// // Retrieve an object
/// if let Some((data, meta)) = storage.get_object("images/logo.png").await? {
///     println!("Content-Type: {}", meta.content_type);
/// }
/// ```
#[derive(Clone)]
pub struct StorageService {
    backend: Arc<dyn StorageBackend>,
}

impl StorageService {
    /// Creates a new `StorageService` backed by a filesystem directory.
    ///
    /// This is the default for CLI usage where persistence is required.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage directory cannot be created or opened.
    pub fn file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let backend = FilesystemBackend::open(path)?;
        Ok(Self {
            backend: Arc::new(backend),
        })
    }

    /// Creates a new `StorageService` backed by an in-memory store.
    ///
    /// Ideal for testing, development, and embedded applications.
    /// All data is lost when the process exits.
    pub fn memory() -> Self {
        Self {
            backend: Arc::new(MemoryStorageBackend::new()),
        }
    }

    /// Creates a new `StorageService` with a custom backend.
    ///
    /// Use this to integrate custom storage backends like S3, etc.
    pub fn custom<B: StorageBackend>(backend: B) -> Self {
        Self {
            backend: Arc::new(backend),
        }
    }

    /// Creates a new `StorageService` from a boxed backend.
    ///
    /// Useful when working with trait objects directly.
    pub fn from_boxed(backend: Box<dyn StorageBackend>) -> Self {
        Self {
            backend: Arc::from(backend),
        }
    }

    /// Stores an object with metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or storage fails.
    pub async fn put_object(
        &self,
        path: &str,
        data: &[u8],
        content_type: Option<&str>,
    ) -> Result<ObjectMeta> {
        self.backend.put(path, data, content_type).await
    }

    /// Retrieves an object and its metadata.
    ///
    /// Returns `Ok(None)` if the object doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or retrieval fails.
    pub async fn get_object(&self, path: &str) -> Result<Option<(Vec<u8>, ObjectMeta)>> {
        self.backend.get(path).await
    }

    /// Deletes an object.
    ///
    /// Returns `Ok(true)` if the object existed, `Ok(false)` otherwise.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or deletion fails.
    pub async fn delete_object(&self, path: &str) -> Result<bool> {
        self.backend.delete(path).await
    }

    /// Retrieves object metadata without downloading the object.
    ///
    /// Returns `Ok(None)` if the object doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or metadata cannot be read.
    pub async fn head_object(&self, path: &str) -> Result<Option<ObjectMeta>> {
        self.backend.head(path).await
    }

    /// Lists all objects, optionally filtered by prefix.
    ///
    /// # Errors
    ///
    /// Returns an error if listing fails.
    pub async fn list_objects(&self, prefix: Option<&str>) -> Result<Vec<ObjectMeta>> {
        self.backend.list(prefix).await
    }
}
