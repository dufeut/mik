//! Async wrappers for storage operations.
//!
//! These methods wrap the synchronous operations in `spawn_blocking` to
//! avoid blocking the async runtime. Use these when calling from async
//! contexts (HTTP handlers, etc.).

use anyhow::{Context, Result};

use super::StorageService;
use super::types::ObjectMeta;

impl StorageService {
    /// Stores an object asynchronously.
    ///
    /// Async version of `put_object` that uses `spawn_blocking`.
    pub async fn put_object_async(
        &self,
        path: String,
        data: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<ObjectMeta> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            service.put_object(&path, &data, content_type.as_deref())
        })
        .await
        .context("Task join error")?
    }

    /// Retrieves an object asynchronously.
    ///
    /// Async version of `get_object` that uses `spawn_blocking`.
    pub async fn get_object_async(&self, path: String) -> Result<Option<(Vec<u8>, ObjectMeta)>> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || service.get_object(&path))
            .await
            .context("Task join error")?
    }

    /// Deletes an object asynchronously.
    ///
    /// Async version of `delete_object` that uses `spawn_blocking`.
    pub async fn delete_object_async(&self, path: String) -> Result<bool> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || service.delete_object(&path))
            .await
            .context("Task join error")?
    }

    /// Retrieves object metadata asynchronously.
    ///
    /// Async version of `head_object` that uses `spawn_blocking`.
    pub async fn head_object_async(&self, path: String) -> Result<Option<ObjectMeta>> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || service.head_object(&path))
            .await
            .context("Task join error")?
    }

    /// Lists objects asynchronously.
    ///
    /// Async version of `list_objects` that uses `spawn_blocking`.
    pub async fn list_objects_async(&self, prefix: Option<String>) -> Result<Vec<ObjectMeta>> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || service.list_objects(prefix.as_deref()))
            .await
            .context("Task join error")?
    }
}
