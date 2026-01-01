//! Filesystem-backed storage backend.
//!
//! Provides persistent object storage using the local filesystem with
//! metadata tracked in redb.

use super::backend::StorageBackend;
use super::metadata::{load_metadata, reconcile, remove_metadata, save_metadata};
use super::types::{OBJECTS_TABLE, ObjectMeta};
use super::validation::object_path;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use redb::Database;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Filesystem-backed object storage backend.
///
/// Stores objects on the filesystem with metadata tracked in redb for
/// fast queries and listing operations.
///
/// # Thread Safety
///
/// `FilesystemBackend` is `Clone` and can be shared across threads. The underlying
/// database handles concurrent access safely.
#[derive(Clone)]
pub struct FilesystemBackend {
    base_dir: PathBuf,
    db: Arc<Database>,
}

impl FilesystemBackend {
    /// Creates or opens the storage backend at the given base directory.
    ///
    /// # Arguments
    /// * `base_dir` - Root directory for object storage (e.g., ~/.mik/storage)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Storage directory cannot be created
    /// - Metadata database cannot be opened or initialized
    /// - Metadata reconciliation fails
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

        let backend = Self {
            base_dir,
            db: Arc::new(db),
        };

        // Reconcile metadata with filesystem on startup
        backend.reconcile_sync()?;

        Ok(backend)
    }

    /// Reconciles metadata database with actual filesystem state.
    fn reconcile_sync(&self) -> Result<()> {
        reconcile(&self.db, &self.base_dir)
    }

    /// Internal helper for synchronous put.
    fn put_sync(&self, path: &str, data: &[u8], content_type: Option<&str>) -> Result<ObjectMeta> {
        let file_path = object_path(&self.base_dir, path)?;

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent directories for: {path}"))?;
        }

        // Write object data to filesystem
        fs::write(&file_path, data).with_context(|| format!("Failed to write object: {path}"))?;

        // Determine content type
        let content_type = content_type
            .map(std::string::ToString::to_string)
            .or_else(|| {
                mime_guess::from_path(&file_path)
                    .first()
                    .map(|mime| mime.to_string())
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());

        // Create metadata
        let now = Utc::now();
        let meta = ObjectMeta {
            path: path.to_string(),
            size: data.len() as u64,
            content_type,
            created_at: now,
            modified_at: now,
        };

        // Store metadata in database
        save_metadata(&self.db, &meta)?;

        Ok(meta)
    }

    /// Internal helper for synchronous get.
    fn get_sync(&self, path: &str) -> Result<Option<(Vec<u8>, ObjectMeta)>> {
        let file_path = object_path(&self.base_dir, path)?;

        // Check if file exists
        if !file_path.exists() {
            return Ok(None);
        }

        // Read object data
        let data =
            fs::read(&file_path).with_context(|| format!("Failed to read object: {path}"))?;

        // Load metadata
        let meta = if let Some(meta) = load_metadata(&self.db, path)? {
            meta
        } else {
            // File exists but no metadata - reconstruct from filesystem
            let metadata = fs::metadata(&file_path)
                .with_context(|| format!("Failed to get file metadata: {path}"))?;

            let content_type = mime_guess::from_path(&file_path).first().map_or_else(
                || "application/octet-stream".to_string(),
                |mime| mime.to_string(),
            );

            let now = Utc::now();
            ObjectMeta {
                path: path.to_string(),
                size: metadata.len(),
                content_type,
                created_at: now,
                modified_at: now,
            }
        };

        Ok(Some((data, meta)))
    }

    /// Internal helper for synchronous delete.
    fn delete_sync(&self, path: &str) -> Result<bool> {
        let file_path = object_path(&self.base_dir, path)?;

        // Check if file exists
        if !file_path.exists() {
            // Also remove metadata if it exists (cleanup orphaned entries)
            remove_metadata(&self.db, path)?;
            return Ok(false);
        }

        // Delete file
        fs::remove_file(&file_path).with_context(|| format!("Failed to delete object: {path}"))?;

        // Delete metadata
        remove_metadata(&self.db, path)?;

        Ok(true)
    }

    /// Internal helper for synchronous head.
    fn head_sync(&self, path: &str) -> Result<Option<ObjectMeta>> {
        let file_path = object_path(&self.base_dir, path)?;

        // Check if file exists
        if !file_path.exists() {
            return Ok(None);
        }

        // Try to load metadata from database
        if let Some(meta) = load_metadata(&self.db, path)? {
            return Ok(Some(meta));
        }

        // File exists but no metadata - reconstruct from filesystem
        let metadata = fs::metadata(&file_path)
            .with_context(|| format!("Failed to get file metadata: {path}"))?;

        let content_type = mime_guess::from_path(&file_path).first().map_or_else(
            || "application/octet-stream".to_string(),
            |mime| mime.to_string(),
        );

        let now = Utc::now();
        Ok(Some(ObjectMeta {
            path: path.to_string(),
            size: metadata.len(),
            content_type,
            created_at: now,
            modified_at: now,
        }))
    }

    /// Internal helper for synchronous list.
    fn list_sync(&self, prefix: Option<&str>) -> Result<Vec<ObjectMeta>> {
        use redb::{ReadableDatabase, ReadableTable};

        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        let table = read_txn
            .open_table(OBJECTS_TABLE)
            .context("Failed to open objects table")?;

        let mut objects = Vec::new();

        for item in table.iter().context("Failed to iterate objects table")? {
            let (key, value) = item.context("Failed to read object entry")?;

            // Apply prefix filter
            if let Some(prefix) = prefix
                && !key.value().starts_with(prefix)
            {
                continue;
            }

            // Deserialize metadata
            if let Ok(meta) = serde_json::from_slice::<ObjectMeta>(value.value()) {
                objects.push(meta);
            }
        }

        // Sort by path for consistent ordering
        objects.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(objects)
    }
}

#[async_trait]
impl StorageBackend for FilesystemBackend {
    async fn put(&self, path: &str, data: &[u8], content_type: Option<&str>) -> Result<ObjectMeta> {
        let backend = self.clone();
        let path = path.to_string();
        let data = data.to_vec();
        let content_type = content_type.map(std::string::ToString::to_string);
        tokio::task::spawn_blocking(move || backend.put_sync(&path, &data, content_type.as_deref()))
            .await
            .context("Task join error")?
    }

    async fn get(&self, path: &str) -> Result<Option<(Vec<u8>, ObjectMeta)>> {
        let backend = self.clone();
        let path = path.to_string();
        tokio::task::spawn_blocking(move || backend.get_sync(&path))
            .await
            .context("Task join error")?
    }

    async fn delete(&self, path: &str) -> Result<bool> {
        let backend = self.clone();
        let path = path.to_string();
        tokio::task::spawn_blocking(move || backend.delete_sync(&path))
            .await
            .context("Task join error")?
    }

    async fn head(&self, path: &str) -> Result<Option<ObjectMeta>> {
        let backend = self.clone();
        let path = path.to_string();
        tokio::task::spawn_blocking(move || backend.head_sync(&path))
            .await
            .context("Task join error")?
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<ObjectMeta>> {
        let backend = self.clone();
        let prefix = prefix.map(std::string::ToString::to_string);
        tokio::task::spawn_blocking(move || backend.list_sync(prefix.as_deref()))
            .await
            .context("Task join error")?
    }
}
