//! Redb-backed KV storage backend.
//!
//! Provides persistent key-value storage using redb with ACID guarantees.

use super::backend::KvBackend;
use super::types::KvEntry;
use anyhow::{Context, Result};
use async_trait::async_trait;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Table name for key-value pairs with expiration metadata
pub(crate) const KV_TABLE: TableDefinition<'static, &'static str, &'static [u8]> =
    TableDefinition::new("kv");

/// Redb-backed key-value storage backend.
///
/// Provides persistent storage with ACID guarantees. Suitable for
/// production use where durability is required.
///
/// # Thread Safety
///
/// `RedbBackend` is `Clone` and can be shared across threads. The underlying
/// database handles concurrent access safely.
#[derive(Clone)]
pub struct RedbBackend {
    db: Arc<Database>,
}

impl RedbBackend {
    /// Opens or creates a redb database at the given path.
    ///
    /// Creates parent directories if needed. Uses redb's ACID guarantees
    /// to prevent corruption on crashes or unclean shutdowns.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Parent directory cannot be created
    /// - Database file cannot be opened or created (permissions, disk full, etc.)
    /// - Initialization transaction fails to begin or commit
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Ensure parent directory exists before opening database
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create KV directory: {}", parent.display()))?;
        }

        let db = Database::create(path)
            .with_context(|| format!("Failed to open KV database: {}", path.display()))?;

        // Initialize table on first open to ensure it exists for reads
        let write_txn = db
            .begin_write()
            .context("Failed to begin initialization transaction")?;
        {
            let _table = write_txn
                .open_table(KV_TABLE)
                .context("Failed to initialize KV table")?;
        }
        write_txn
            .commit()
            .context("Failed to commit initialization transaction")?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Internal helper to get a value synchronously.
    fn get_sync(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        let table = read_txn
            .open_table(KV_TABLE)
            .context("Failed to open KV table")?;

        let result = table
            .get(key)
            .with_context(|| format!("Failed to read key '{key}'"))?;

        match result {
            Some(guard) => {
                let json = guard.value();
                let entry: KvEntry = serde_json::from_slice(json)
                    .with_context(|| format!("Failed to deserialize entry for key '{key}'"))?;

                // Check expiration
                if entry.is_expired()? {
                    // Drop read transaction before starting write
                    drop(table);
                    drop(read_txn);

                    // Remove expired entry
                    self.delete_sync(key)?;
                    Ok(None)
                } else {
                    Ok(Some(entry.value))
                }
            },
            None => Ok(None),
        }
    }

    /// Internal helper to set a value synchronously.
    fn set_sync(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<()> {
        let entry = if let Some(ttl) = ttl {
            KvEntry::with_ttl(value, ttl.as_secs())?
        } else {
            KvEntry::new(value)
        };

        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;

        {
            let mut table = write_txn
                .open_table(KV_TABLE)
                .context("Failed to open KV table")?;

            let json = serde_json::to_vec(&entry).context("Failed to serialize entry to JSON")?;

            table
                .insert(key, json.as_slice())
                .with_context(|| format!("Failed to insert key '{key}'"))?;
        }

        write_txn
            .commit()
            .context("Failed to commit set transaction")?;

        Ok(())
    }

    /// Internal helper to delete a value synchronously.
    fn delete_sync(&self, key: &str) -> Result<bool> {
        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;

        let removed = {
            let mut table = write_txn
                .open_table(KV_TABLE)
                .context("Failed to open KV table")?;

            table
                .remove(key)
                .with_context(|| format!("Failed to remove key '{key}'"))?
                .is_some()
        };

        write_txn
            .commit()
            .context("Failed to commit delete transaction")?;

        Ok(removed)
    }

    /// Internal helper to list keys synchronously.
    fn list_sync(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        let table = read_txn
            .open_table(KV_TABLE)
            .context("Failed to open KV table")?;

        let mut keys = Vec::new();
        let mut expired_keys = Vec::new();

        for item in table.iter().context("Failed to iterate KV table")? {
            let (key, value) = item.context("Failed to read KV entry")?;
            let key_str = key.value();

            // Filter by prefix if provided
            if let Some(prefix) = prefix
                && !key_str.starts_with(prefix)
            {
                continue;
            }

            // Check expiration
            if let Ok(entry) = serde_json::from_slice::<KvEntry>(value.value()) {
                match entry.is_expired() {
                    Ok(true) => {
                        // Mark for deletion
                        expired_keys.push(key_str.to_string());
                    },
                    Ok(false) => {
                        // Valid entry
                        keys.push(key_str.to_string());
                    },
                    Err(_) => {
                        // Skip entries with time errors
                    },
                }
            }
        }

        // Clean up expired keys (drop read transaction first)
        drop(table);
        drop(read_txn);

        for key in expired_keys {
            let _ = self.delete_sync(&key); // Ignore errors during cleanup
        }

        Ok(keys)
    }
}

#[async_trait]
impl KvBackend for RedbBackend {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let backend = self.clone();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || backend.get_sync(&key))
            .await
            .context("Task join error")?
    }

    async fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<()> {
        let backend = self.clone();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || backend.set_sync(&key, value, ttl))
            .await
            .context("Task join error")?
    }

    async fn delete(&self, key: &str) -> Result<bool> {
        let backend = self.clone();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || backend.delete_sync(&key))
            .await
            .context("Task join error")?
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let backend = self.clone();
        let prefix = prefix.map(std::string::ToString::to_string);
        tokio::task::spawn_blocking(move || backend.list_sync(prefix.as_deref()))
            .await
            .context("Task join error")?
    }
}
