//! Metadata database operations for the storage service.
//!
//! Handles saving, loading, and removing object metadata from the redb database,
//! as well as reconciliation between the filesystem and metadata database.

use anyhow::{Context, Result};
use chrono::Utc;
use redb::{Database, ReadableDatabase, ReadableTable};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::types::{OBJECTS_TABLE, ObjectMeta};

/// Saves object metadata to the database.
pub(crate) fn save_metadata(db: &Database, meta: &ObjectMeta) -> Result<()> {
    let write_txn = db
        .begin_write()
        .context("Failed to begin write transaction")?;

    {
        let mut table = write_txn
            .open_table(OBJECTS_TABLE)
            .context("Failed to open objects table")?;

        let json = serde_json::to_vec(meta).context("Failed to serialize object metadata")?;

        table
            .insert(meta.path.as_str(), json.as_slice())
            .with_context(|| format!("Failed to insert object metadata: {}", meta.path))?;
    }

    write_txn
        .commit()
        .context("Failed to commit metadata save transaction")?;

    Ok(())
}

/// Loads object metadata from the database.
pub(crate) fn load_metadata(db: &Database, path: &str) -> Result<Option<ObjectMeta>> {
    let read_txn = db
        .begin_read()
        .context("Failed to begin read transaction")?;

    let table = read_txn
        .open_table(OBJECTS_TABLE)
        .context("Failed to open objects table")?;

    let result = table
        .get(path)
        .with_context(|| format!("Failed to read object metadata: {path}"))?;

    match result {
        Some(guard) => {
            let meta = serde_json::from_slice(guard.value())
                .with_context(|| format!("Failed to deserialize object metadata: {path}"))?;
            Ok(Some(meta))
        },
        None => Ok(None),
    }
}

/// Removes object metadata from the database.
pub(crate) fn remove_metadata(db: &Database, path: &str) -> Result<()> {
    let write_txn = db
        .begin_write()
        .context("Failed to begin write transaction")?;

    {
        let mut table = write_txn
            .open_table(OBJECTS_TABLE)
            .context("Failed to open objects table")?;

        table
            .remove(path)
            .with_context(|| format!("Failed to remove object metadata: {path}"))?;
    }

    write_txn
        .commit()
        .context("Failed to commit metadata removal transaction")?;

    Ok(())
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
pub(crate) fn reconcile(db: &Database, base_dir: &Path) -> Result<()> {
    tracing::debug!(base_dir = %base_dir.display(), "Reconciling storage metadata");

    // Phase 1: Collect all files from filesystem
    let mut fs_files: HashSet<String> = HashSet::new();
    scan_directory(base_dir, base_dir, &mut fs_files)?;

    // Phase 2: Check metadata entries against filesystem
    let mut orphaned_entries: Vec<String> = Vec::new();
    let mut stale_entries: Vec<(String, u64)> = Vec::new(); // (path, actual_size)

    {
        let read_txn = db
            .begin_read()
            .context("Failed to begin read transaction for reconciliation")?;
        let table = read_txn
            .open_table(OBJECTS_TABLE)
            .context("Failed to open objects table for reconciliation")?;

        for item in table.iter().context("Failed to iterate objects table")? {
            let (key, value) = item.context("Failed to read object entry")?;
            let path = key.value().to_string();

            if fs_files.contains(&path) {
                // File exists, check if metadata is stale
                fs_files.remove(&path); // Mark as seen

                if let Ok(meta) = serde_json::from_slice::<ObjectMeta>(value.value())
                    && let Ok(file_meta) = fs::metadata(base_dir.join(&path))
                    && file_meta.len() != meta.size
                {
                    stale_entries.push((path, file_meta.len()));
                }
            } else {
                // Metadata exists but file is gone
                orphaned_entries.push(path);
            }
        }
    }

    // Phase 3: Remove orphaned metadata entries
    if !orphaned_entries.is_empty() {
        tracing::info!(
            count = orphaned_entries.len(),
            "Removing orphaned metadata entries"
        );
        for path in &orphaned_entries {
            remove_metadata(db, path)?;
        }
    }

    // Phase 4: Add missing metadata entries (files without metadata)
    if !fs_files.is_empty() {
        tracing::info!(
            count = fs_files.len(),
            "Creating metadata for untracked files"
        );
        for path in &fs_files {
            let file_path = base_dir.join(path);
            if let Ok(file_meta) = fs::metadata(&file_path) {
                let content_type = mime_guess::from_path(&file_path)
                    .first()
                    .map_or_else(|| "application/octet-stream".to_string(), |m| m.to_string());

                let now = Utc::now();
                let meta = ObjectMeta {
                    path: path.clone(),
                    size: file_meta.len(),
                    content_type,
                    created_at: now,
                    modified_at: now,
                };
                save_metadata(db, &meta)?;
            }
        }
    }

    // Phase 5: Update stale metadata entries
    if !stale_entries.is_empty() {
        tracing::info!(
            count = stale_entries.len(),
            "Updating stale metadata entries"
        );
        for (path, actual_size) in &stale_entries {
            if let Some(mut meta) = load_metadata(db, path)? {
                meta.size = *actual_size;
                meta.modified_at = Utc::now();
                save_metadata(db, &meta)?;
            }
        }
    }

    let total_fixes = orphaned_entries.len() + fs_files.len() + stale_entries.len();
    if total_fixes > 0 {
        tracing::info!(
            orphaned = orphaned_entries.len(),
            untracked = fs_files.len(),
            stale = stale_entries.len(),
            "Storage reconciliation complete"
        );
    } else {
        tracing::debug!("Storage metadata is consistent with filesystem");
    }

    Ok(())
}

/// Recursively scans a directory and collects relative paths to files.
fn scan_directory(base_dir: &Path, dir: &Path, files: &mut HashSet<String>) -> Result<()> {
    if !dir.exists() || !dir.is_dir() {
        return Ok(());
    }

    for entry in
        fs::read_dir(dir).with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        // Skip the metadata database file
        if path.file_name().is_some_and(|n| n == "metadata.redb") {
            continue;
        }
        // Skip redb lock files
        if path.extension().is_some_and(|e| e == "lock") {
            continue;
        }

        if path.is_dir() {
            scan_directory(base_dir, &path, files)?;
        } else if path.is_file() {
            // Convert to relative path from base_dir
            if let Ok(relative) = path.strip_prefix(base_dir) {
                // Normalize path separators for cross-platform consistency
                let relative_str = relative.to_string_lossy().replace('\\', "/");
                files.insert(relative_str);
            }
        }
    }

    Ok(())
}
