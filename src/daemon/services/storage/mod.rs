//! Embedded S3-like object storage service for the mik daemon.
//!
//! Provides filesystem-based object storage with metadata tracking using redb.
//! Objects are stored in `~/.mik/storage/` with metadata in a companion database.
//!
//! Security features:
//! - Path traversal protection (prevents escaping storage directory)
//! - Atomic operations with ACID guarantees via redb
//! - Content-type validation and storage
//!
//! # Async Usage
//!
//! All database operations are blocking. When using from async contexts,
//! use the async methods (`get_object_async`, `put_object_async`, etc.)
//! which automatically wrap operations in `spawn_blocking` to avoid blocking
//! the async runtime.

mod async_ops;
mod metadata;
mod operations;
mod types;
mod validation;

use anyhow::{Context, Result};
use redb::Database;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// Re-export public types
pub use types::ObjectMeta;

use metadata::reconcile;
use operations::{delete_object, get_object, head_object, list_objects, put_object};
use types::OBJECTS_TABLE;

/// Embedded object storage service.
///
/// Stores objects on the filesystem with metadata tracked in redb for
/// fast queries and listing operations.
///
/// # Thread Safety
///
/// `StorageService` is `Clone` and can be shared across threads. The underlying
/// database handles concurrent access safely.
#[derive(Clone)]
pub struct StorageService {
    /// Base directory for object storage (e.g., ~/.mik/storage)
    base_dir: PathBuf,
    /// Metadata database
    db: Arc<Database>,
}

impl StorageService {
    /// Creates or opens the storage service at the given base directory.
    ///
    /// # Arguments
    /// * `base_dir` - Root directory for object storage (e.g., ~/.mik/storage)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Storage directory cannot be created
    /// - Metadata database cannot be opened or initialized
    /// - Metadata reconciliation fails (filesystem scan errors)
    ///
    /// # Security
    /// All object paths are normalized and validated to prevent directory
    /// traversal attacks. Paths containing `..`, absolute paths, or other
    /// suspicious components are rejected.
    pub fn open<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();

        // Create base directory if it doesn't exist
        fs::create_dir_all(&base_dir).with_context(|| {
            format!("Failed to create storage directory: {}", base_dir.display())
        })?;

        // Create metadata database in base directory
        let db_path = base_dir.join("metadata.redb");
        let db = Database::create(&db_path).with_context(|| {
            format!(
                "Failed to open storage metadata database: {}",
                db_path.display()
            )
        })?;

        // Initialize metadata table
        let write_txn = db
            .begin_write()
            .context("Failed to begin initialization transaction")?;
        {
            let _table = write_txn
                .open_table(OBJECTS_TABLE)
                .context("Failed to initialize objects table")?;
        }
        write_txn
            .commit()
            .context("Failed to commit initialization transaction")?;

        let service = Self {
            base_dir,
            db: Arc::new(db),
        };

        // Reconcile metadata with filesystem on startup
        service.reconcile()?;

        Ok(service)
    }

    /// Reconciles metadata database with actual filesystem state.
    ///
    /// This method is called automatically on startup to handle:
    /// - Files deleted outside the service (removes orphaned metadata)
    /// - Files added outside the service (creates missing metadata)
    /// - Files modified outside the service (updates stale metadata)
    ///
    /// # Errors
    ///
    /// Returns an error if directory scanning fails or database operations fail.
    ///
    /// # Performance
    /// This scans the entire storage directory and metadata table, so it
    /// may take time for large storage systems. Consider calling only
    /// on startup or periodically during low-traffic windows.
    pub fn reconcile(&self) -> Result<()> {
        reconcile(&self.db, &self.base_dir)
    }

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
    /// - Parent directories cannot be created
    /// - File cannot be written (permissions, disk full)
    /// - Metadata cannot be saved to database
    ///
    /// # Example
    /// ```no_run
    /// let storage = StorageService::open("~/.mik/storage")?;
    /// let meta = storage.put_object(
    ///     "images/logo.png",
    ///     &image_bytes,
    ///     Some("image/png")
    /// )?;
    /// println!("Stored {} bytes", meta.size);
    /// ```
    pub fn put_object(
        &self,
        path: &str,
        data: &[u8],
        content_type: Option<&str>,
    ) -> Result<ObjectMeta> {
        put_object(&self.db, &self.base_dir, path, data, content_type)
    }

    /// Retrieves an object and its metadata.
    ///
    /// # Returns
    /// * `Ok(Some((data, meta)))` - Object found
    /// * `Ok(None)` - Object not found
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or the file cannot be read.
    ///
    /// # Example
    /// ```no_run
    /// if let Some((data, meta)) = storage.get_object("images/logo.png")? {
    ///     println!("Content-Type: {}", meta.content_type);
    ///     println!("Size: {} bytes", data.len());
    /// }
    /// ```
    pub fn get_object(&self, path: &str) -> Result<Option<(Vec<u8>, ObjectMeta)>> {
        get_object(&self.db, &self.base_dir, path)
    }

    /// Deletes an object and its metadata.
    ///
    /// # Returns
    /// * `Ok(true)` - Object existed and was deleted
    /// * `Ok(false)` - Object did not exist
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid, file deletion fails, or
    /// metadata cannot be removed from the database.
    ///
    /// # Example
    /// ```no_run
    /// if storage.delete_object("images/old-logo.png")? {
    ///     println!("Object deleted");
    /// } else {
    ///     println!("Object not found");
    /// }
    /// ```
    pub fn delete_object(&self, path: &str) -> Result<bool> {
        delete_object(&self.db, &self.base_dir, path)
    }

    /// Retrieves object metadata without downloading the object.
    ///
    /// # Returns
    /// * `Ok(Some(meta))` - Object found
    /// * `Ok(None)` - Object not found
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid or filesystem metadata cannot be read.
    ///
    /// # Example
    /// ```no_run
    /// if let Some(meta) = storage.head_object("images/logo.png")? {
    ///     println!("Size: {} bytes", meta.size);
    ///     println!("Modified: {}", meta.modified_at);
    /// }
    /// ```
    pub fn head_object(&self, path: &str) -> Result<Option<ObjectMeta>> {
        head_object(&self.db, &self.base_dir, path)
    }

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
    /// Returns an error if the database read transaction fails or iteration errors occur.
    ///
    /// # Example
    /// ```no_run
    /// // List all objects
    /// let all_objects = storage.list_objects(None)?;
    ///
    /// // List only images
    /// let images = storage.list_objects(Some("images/"))?;
    /// ```
    pub fn list_objects(&self, prefix: Option<&str>) -> Result<Vec<ObjectMeta>> {
        list_objects(&self.db, prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn create_storage() -> (StorageService, TempDir) {
        let tmp = TempDir::new().unwrap();
        let storage = StorageService::open(tmp.path()).unwrap();
        (storage, tmp)
    }

    #[test]
    fn test_put_and_get_object() {
        let (storage, _tmp) = create_storage();

        let data = b"Hello, World!";
        let meta = storage
            .put_object("test.txt", data, Some("text/plain"))
            .unwrap();

        assert_eq!(meta.path, "test.txt");
        assert_eq!(meta.size, 13);
        assert_eq!(meta.content_type, "text/plain");

        let (retrieved_data, retrieved_meta) = storage.get_object("test.txt").unwrap().unwrap();
        assert_eq!(retrieved_data, data);
        assert_eq!(retrieved_meta.path, "test.txt");
        assert_eq!(retrieved_meta.size, 13);
    }

    #[test]
    fn test_get_nonexistent_object() {
        let (storage, _tmp) = create_storage();

        let result = storage.get_object("nonexistent.txt").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_object() {
        let (storage, _tmp) = create_storage();

        storage.put_object("test.txt", b"data", None).unwrap();

        let deleted = storage.delete_object("test.txt").unwrap();
        assert!(deleted);

        let result = storage.get_object("test.txt").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_nonexistent_object() {
        let (storage, _tmp) = create_storage();

        let deleted = storage.delete_object("nonexistent.txt").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_head_object() {
        let (storage, _tmp) = create_storage();

        storage
            .put_object("test.txt", b"Hello", Some("text/plain"))
            .unwrap();

        let meta = storage.head_object("test.txt").unwrap().unwrap();
        assert_eq!(meta.path, "test.txt");
        assert_eq!(meta.size, 5);
        assert_eq!(meta.content_type, "text/plain");
    }

    #[test]
    fn test_head_nonexistent_object() {
        let (storage, _tmp) = create_storage();

        let result = storage.head_object("nonexistent.txt").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_objects() {
        let (storage, _tmp) = create_storage();

        storage
            .put_object("images/logo.png", b"png", Some("image/png"))
            .unwrap();
        storage
            .put_object("images/banner.jpg", b"jpg", Some("image/jpeg"))
            .unwrap();
        storage
            .put_object("docs/readme.md", b"md", Some("text/markdown"))
            .unwrap();

        let all_objects = storage.list_objects(None).unwrap();
        assert_eq!(all_objects.len(), 3);

        let images = storage.list_objects(Some("images/")).unwrap();
        assert_eq!(images.len(), 2);
        assert!(images.iter().all(|m| m.path.starts_with("images/")));

        let docs = storage.list_objects(Some("docs/")).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path, "docs/readme.md");
    }

    #[test]
    fn test_list_empty_storage() {
        let (storage, _tmp) = create_storage();

        let objects = storage.list_objects(None).unwrap();
        assert_eq!(objects.len(), 0);
    }

    #[test]
    fn test_path_traversal_prevention() {
        let (storage, _tmp) = create_storage();

        // Test various path traversal attempts
        let attack_paths = [
            "../etc/passwd",
            "../../etc/passwd",
            "test/../../../etc/passwd",
            "/etc/passwd",
            "test/../../etc/passwd",
        ];

        for path in &attack_paths {
            let result = storage.put_object(path, b"attack", None);
            assert!(result.is_err(), "Path traversal not prevented for: {path}");
        }
    }

    #[test]
    fn test_empty_path_rejection() {
        let (storage, _tmp) = create_storage();

        let result = storage.put_object("", b"data", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_nested_paths() {
        let (storage, _tmp) = create_storage();

        let data = b"nested data";
        storage.put_object("a/b/c/d/file.txt", data, None).unwrap();

        let (retrieved, _) = storage.get_object("a/b/c/d/file.txt").unwrap().unwrap();
        assert_eq!(retrieved, data);
    }

    #[test]
    fn test_content_type_auto_detection() {
        let (storage, _tmp) = create_storage();

        storage.put_object("test.json", b"{}", None).unwrap();
        storage.put_object("test.html", b"<html>", None).unwrap();
        storage.put_object("test.png", b"png", None).unwrap();

        let json_meta = storage.head_object("test.json").unwrap().unwrap();
        assert_eq!(json_meta.content_type, "application/json");

        let html_meta = storage.head_object("test.html").unwrap().unwrap();
        assert_eq!(html_meta.content_type, "text/html");

        let png_meta = storage.head_object("test.png").unwrap().unwrap();
        assert_eq!(png_meta.content_type, "image/png");
    }

    #[test]
    fn test_overwrite_object() {
        let (storage, _tmp) = create_storage();

        storage
            .put_object("test.txt", b"original", Some("text/plain"))
            .unwrap();
        storage
            .put_object("test.txt", b"updated", Some("text/plain"))
            .unwrap();

        let (data, meta) = storage.get_object("test.txt").unwrap().unwrap();
        assert_eq!(data, b"updated");
        assert_eq!(meta.size, 7);
    }

    #[test]
    fn test_normalize_current_dir() {
        let (storage, _tmp) = create_storage();

        storage.put_object("./test.txt", b"data", None).unwrap();

        // Should be stored as "test.txt" without "./"
        let result = storage.get_object("test.txt").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_list_sorting() {
        let (storage, _tmp) = create_storage();

        storage.put_object("z.txt", b"z", None).unwrap();
        storage.put_object("a.txt", b"a", None).unwrap();
        storage.put_object("m.txt", b"m", None).unwrap();

        let objects = storage.list_objects(None).unwrap();
        assert_eq!(objects[0].path, "a.txt");
        assert_eq!(objects[1].path, "m.txt");
        assert_eq!(objects[2].path, "z.txt");
    }

    #[test]
    fn test_metadata_timestamps() {
        let (storage, _tmp) = create_storage();

        let before = Utc::now();
        let meta = storage.put_object("test.txt", b"data", None).unwrap();
        let after = Utc::now();

        assert!(meta.created_at >= before);
        assert!(meta.created_at <= after);
        assert_eq!(meta.created_at, meta.modified_at);
    }
}
