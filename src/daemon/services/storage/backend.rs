//! Backend trait for the storage service.
//!
//! Defines the interface that all storage backends must implement,
//! enabling pluggable storage (filesystem, memory, S3, etc.).

use super::types::ObjectMeta;
use anyhow::Result;
use async_trait::async_trait;

/// Backend trait for object storage.
///
/// All backends must be thread-safe (`Send + Sync`) for use with tokio.
/// Implementations should handle their own concurrency and provide
/// appropriate consistency guarantees where applicable.
///
/// # Example
///
/// ```ignore
/// use mik::daemon::services::storage::{StorageBackend, MemoryStorageBackend};
///
/// let backend = MemoryStorageBackend::new();
/// backend.put("images/logo.png", &image_bytes, Some("image/png")).await?;
/// let (data, meta) = backend.get("images/logo.png").await?.unwrap();
/// ```
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    /// Stores an object with metadata.
    ///
    /// # Arguments
    /// * `path` - Virtual path for the object (e.g., "images/logo.png")
    /// * `data` - Object data bytes
    /// * `content_type` - Optional MIME type (auto-detected if None)
    ///
    /// # Returns
    /// Metadata of the stored object
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Path is invalid (empty, absolute, or contains `..`)
    /// - Storage operation fails
    async fn put(&self, path: &str, data: &[u8], content_type: Option<&str>) -> Result<ObjectMeta>;

    /// Retrieves an object and its metadata.
    ///
    /// # Returns
    /// * `Ok(Some((data, meta)))` - Object found
    /// * `Ok(None)` - Object not found
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or the read operation fails.
    async fn get(&self, path: &str) -> Result<Option<(Vec<u8>, ObjectMeta)>>;

    /// Deletes an object.
    ///
    /// # Returns
    /// * `Ok(true)` - Object existed and was deleted
    /// * `Ok(false)` - Object did not exist
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or deletion fails.
    async fn delete(&self, path: &str) -> Result<bool>;

    /// Retrieves object metadata without downloading the object.
    ///
    /// # Returns
    /// * `Ok(Some(meta))` - Object found
    /// * `Ok(None)` - Object not found
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or metadata cannot be read.
    async fn head(&self, path: &str) -> Result<Option<ObjectMeta>>;

    /// Lists all objects, optionally filtered by prefix.
    ///
    /// # Arguments
    /// * `prefix` - Optional path prefix filter (e.g., "images/" lists only images)
    ///
    /// # Returns
    /// Vector of object metadata sorted by path
    ///
    /// # Errors
    ///
    /// Returns an error if listing fails.
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<ObjectMeta>>;
}
