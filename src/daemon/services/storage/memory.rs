//! In-memory storage backend.
//!
//! Provides a fast, non-persistent object store using DashMap for
//! concurrent access. Ideal for testing, development, and embedded use cases.

use super::backend::StorageBackend;
use super::types::ObjectMeta;
use super::validation::validate_path;
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;

/// Entry stored in the memory backend.
#[derive(Clone)]
struct MemoryObject {
    data: Vec<u8>,
    meta: ObjectMeta,
}

/// In-memory object storage backend using DashMap.
///
/// Provides fast, concurrent access without persistence. All data is lost
/// when the process exits. Ideal for:
/// - Testing and development
/// - Embedded applications (Tauri, etc.)
/// - Temporary storage
///
/// # Thread Safety
///
/// `MemoryStorageBackend` is `Clone` and uses `DashMap` internally for
/// lock-free concurrent access.
///
/// # Example
///
/// ```ignore
/// use mik::daemon::services::storage::MemoryStorageBackend;
///
/// let backend = MemoryStorageBackend::new();
/// backend.put("images/logo.png", &image_bytes, Some("image/png")).await?;
/// ```
#[derive(Clone, Default)]
pub struct MemoryStorageBackend {
    data: DashMap<String, MemoryObject>,
}

impl MemoryStorageBackend {
    /// Creates a new empty in-memory backend.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of objects in the store.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the store is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clears all objects from the store.
    #[allow(dead_code)]
    pub fn clear(&self) {
        self.data.clear();
    }
}

#[async_trait]
impl StorageBackend for MemoryStorageBackend {
    async fn put(&self, path: &str, data: &[u8], content_type: Option<&str>) -> Result<ObjectMeta> {
        // Validate and normalize path
        let normalized = validate_path(path)?;
        // Use forward slashes consistently (for cross-platform compatibility)
        let normalized_str = normalized.to_string_lossy().replace('\\', "/");

        // Determine content type
        let content_type = content_type
            .map(std::string::ToString::to_string)
            .or_else(|| {
                mime_guess::from_path(&normalized)
                    .first()
                    .map(|mime| mime.to_string())
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());

        // Create metadata
        let now = Utc::now();
        let meta = ObjectMeta {
            path: normalized_str.clone(),
            size: data.len() as u64,
            content_type,
            created_at: now,
            modified_at: now,
        };

        // Store object
        let obj = MemoryObject {
            data: data.to_vec(),
            meta: meta.clone(),
        };
        self.data.insert(normalized_str, obj);

        Ok(meta)
    }

    async fn get(&self, path: &str) -> Result<Option<(Vec<u8>, ObjectMeta)>> {
        let normalized = validate_path(path)?;
        let normalized_str = normalized.to_string_lossy().replace('\\', "/");

        Ok(self.data.get(&normalized_str).map(|entry| {
            let obj = entry.value();
            (obj.data.clone(), obj.meta.clone())
        }))
    }

    async fn delete(&self, path: &str) -> Result<bool> {
        let normalized = validate_path(path)?;
        let normalized_str = normalized.to_string_lossy().replace('\\', "/");

        Ok(self.data.remove(&normalized_str).is_some())
    }

    async fn head(&self, path: &str) -> Result<Option<ObjectMeta>> {
        let normalized = validate_path(path)?;
        let normalized_str = normalized.to_string_lossy().replace('\\', "/");

        Ok(self
            .data
            .get(&normalized_str)
            .map(|entry| entry.value().meta.clone()))
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<ObjectMeta>> {
        let mut objects: Vec<ObjectMeta> = self
            .data
            .iter()
            .filter(|entry| {
                if let Some(prefix) = prefix {
                    entry.key().starts_with(prefix)
                } else {
                    true
                }
            })
            .map(|entry| entry.value().meta.clone())
            .collect();

        // Sort by path for consistent ordering
        objects.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(objects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_put_and_get() {
        let backend = MemoryStorageBackend::new();

        let data = b"Hello, World!";
        let meta = backend
            .put("test.txt", data, Some("text/plain"))
            .await
            .unwrap();

        assert_eq!(meta.path, "test.txt");
        assert_eq!(meta.size, 13);
        assert_eq!(meta.content_type, "text/plain");

        let (retrieved_data, retrieved_meta) = backend.get("test.txt").await.unwrap().unwrap();
        assert_eq!(retrieved_data, data);
        assert_eq!(retrieved_meta.path, "test.txt");
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let backend = MemoryStorageBackend::new();
        let result = backend.get("nonexistent.txt").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete() {
        let backend = MemoryStorageBackend::new();

        backend.put("test.txt", b"data", None).await.unwrap();

        let deleted = backend.delete("test.txt").await.unwrap();
        assert!(deleted);

        let result = backend.get("test.txt").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let backend = MemoryStorageBackend::new();
        let deleted = backend.delete("nonexistent.txt").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_head() {
        let backend = MemoryStorageBackend::new();

        backend
            .put("test.txt", b"Hello", Some("text/plain"))
            .await
            .unwrap();

        let meta = backend.head("test.txt").await.unwrap().unwrap();
        assert_eq!(meta.path, "test.txt");
        assert_eq!(meta.size, 5);
        assert_eq!(meta.content_type, "text/plain");
    }

    #[tokio::test]
    async fn test_list() {
        let backend = MemoryStorageBackend::new();

        backend
            .put("images/logo.png", b"png", Some("image/png"))
            .await
            .unwrap();
        backend
            .put("images/banner.jpg", b"jpg", Some("image/jpeg"))
            .await
            .unwrap();
        backend
            .put("docs/readme.md", b"md", Some("text/markdown"))
            .await
            .unwrap();

        let all_objects = backend.list(None).await.unwrap();
        assert_eq!(all_objects.len(), 3);

        let images = backend.list(Some("images/")).await.unwrap();
        assert_eq!(images.len(), 2);
        assert!(images.iter().all(|m| m.path.starts_with("images/")));
    }

    #[tokio::test]
    async fn test_path_traversal_prevention() {
        let backend = MemoryStorageBackend::new();

        let attack_paths = ["../etc/passwd", "../../etc/passwd", "/etc/passwd"];

        for path in &attack_paths {
            let result = backend.put(path, b"attack", None).await;
            assert!(result.is_err(), "Path traversal not prevented for: {path}");
        }
    }

    #[tokio::test]
    async fn test_content_type_auto_detection() {
        let backend = MemoryStorageBackend::new();

        backend.put("test.json", b"{}", None).await.unwrap();
        backend.put("test.html", b"<html>", None).await.unwrap();

        let json_meta = backend.head("test.json").await.unwrap().unwrap();
        assert_eq!(json_meta.content_type, "application/json");

        let html_meta = backend.head("test.html").await.unwrap().unwrap();
        assert_eq!(html_meta.content_type, "text/html");
    }

    #[tokio::test]
    async fn test_overwrite() {
        let backend = MemoryStorageBackend::new();

        backend
            .put("test.txt", b"original", Some("text/plain"))
            .await
            .unwrap();
        backend
            .put("test.txt", b"updated", Some("text/plain"))
            .await
            .unwrap();

        let (data, meta) = backend.get("test.txt").await.unwrap().unwrap();
        assert_eq!(data, b"updated");
        assert_eq!(meta.size, 7);
    }
}
