//! Core storage operations: put, get, delete, head, and list.

use anyhow::{Context, Result};
use chrono::Utc;
use redb::{Database, ReadableDatabase, ReadableTable};
use std::fs;
use std::path::Path;

use super::metadata::{load_metadata, remove_metadata, save_metadata};
use super::types::{OBJECTS_TABLE, ObjectMeta};
use super::validation::object_path;

/// Stores an object with metadata.
///
/// # Arguments
/// * `db` - The metadata database
/// * `base_dir` - Base directory for object storage
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
pub(crate) fn put_object(
    db: &Database,
    base_dir: &Path,
    path: &str,
    data: &[u8],
    content_type: Option<&str>,
) -> Result<ObjectMeta> {
    let file_path = object_path(base_dir, path)?;

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
            // Auto-detect from file extension
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
    save_metadata(db, &meta)?;

    Ok(meta)
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
pub(crate) fn get_object(
    db: &Database,
    base_dir: &Path,
    path: &str,
) -> Result<Option<(Vec<u8>, ObjectMeta)>> {
    let file_path = object_path(base_dir, path)?;

    // Check if file exists
    if !file_path.exists() {
        return Ok(None);
    }

    // Read object data
    let data = fs::read(&file_path).with_context(|| format!("Failed to read object: {path}"))?;

    // Load metadata
    let meta = if let Some(meta) = load_metadata(db, path)? {
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
pub(crate) fn delete_object(db: &Database, base_dir: &Path, path: &str) -> Result<bool> {
    let file_path = object_path(base_dir, path)?;

    // Check if file exists
    if !file_path.exists() {
        // Also remove metadata if it exists (cleanup orphaned entries)
        remove_metadata(db, path)?;
        return Ok(false);
    }

    // Delete file
    fs::remove_file(&file_path).with_context(|| format!("Failed to delete object: {path}"))?;

    // Delete metadata
    remove_metadata(db, path)?;

    Ok(true)
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
pub(crate) fn head_object(
    db: &Database,
    base_dir: &Path,
    path: &str,
) -> Result<Option<ObjectMeta>> {
    let file_path = object_path(base_dir, path)?;

    // Check if file exists
    if !file_path.exists() {
        return Ok(None);
    }

    // Try to load metadata from database
    if let Some(meta) = load_metadata(db, path)? {
        return Ok(Some(meta));
    }

    // File exists but no metadata - reconstruct from filesystem
    let metadata =
        fs::metadata(&file_path).with_context(|| format!("Failed to get file metadata: {path}"))?;

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

/// Lists all objects, optionally filtered by prefix.
///
/// # Arguments
/// * `db` - The metadata database
/// * `prefix` - Optional path prefix filter (e.g., "images/" lists only images)
///
/// # Returns
/// Vector of object metadata sorted by path
///
/// # Errors
///
/// Returns an error if the database read transaction fails or iteration errors occur.
pub(crate) fn list_objects(db: &Database, prefix: Option<&str>) -> Result<Vec<ObjectMeta>> {
    let read_txn = db
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
