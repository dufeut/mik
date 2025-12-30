//! Instance storage operations for the state store.
//!
//! Provides CRUD operations for WASM instance metadata with ACID guarantees.

use anyhow::{Context, Result};
use redb::{ReadableDatabase, ReadableTable};

use super::types::Instance;
use super::{INSTANCES_TABLE, StateStore};

impl StateStore {
    /// Persists an instance to the database.
    ///
    /// Overwrites existing instance with same name. Serializes to JSON
    /// for compatibility with debugging tools and future schema evolution.
    pub fn save_instance(&self, instance: &Instance) -> Result<()> {
        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;

        {
            let mut table = write_txn
                .open_table(INSTANCES_TABLE)
                .context("Failed to open instances table")?;

            let json =
                serde_json::to_vec(instance).context("Failed to serialize instance to JSON")?;

            table
                .insert(instance.name.as_str(), json.as_slice())
                .with_context(|| format!("Failed to insert instance '{}'", instance.name))?;
        }

        write_txn
            .commit()
            .context("Failed to commit instance save transaction")?;

        Ok(())
    }

    /// Retrieves an instance by name.
    ///
    /// Returns None if instance doesn't exist. Deserializes from JSON
    /// and validates structure matches current Instance schema.
    pub fn get_instance(&self, name: &str) -> Result<Option<Instance>> {
        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        let table = read_txn
            .open_table(INSTANCES_TABLE)
            .context("Failed to open instances table")?;

        let result = table
            .get(name)
            .with_context(|| format!("Failed to read instance '{name}'"))?;

        match result {
            Some(guard) => {
                let json = guard.value();
                let instance = serde_json::from_slice(json)
                    .with_context(|| format!("Failed to deserialize instance '{name}'"))?;
                Ok(Some(instance))
            },
            None => Ok(None),
        }
    }

    /// Lists all instances in the database.
    ///
    /// Returns empty vec if no instances exist. Skips instances that fail
    /// deserialization to prevent corruption from blocking reads.
    pub fn list_instances(&self) -> Result<Vec<Instance>> {
        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        let table = read_txn
            .open_table(INSTANCES_TABLE)
            .context("Failed to open instances table")?;

        let mut instances = Vec::new();

        for item in table.iter().context("Failed to iterate instances table")? {
            let (_, value) = item.context("Failed to read instance entry")?;

            // Skip corrupted entries instead of failing the entire list operation
            if let Ok(instance) = serde_json::from_slice::<Instance>(value.value()) {
                instances.push(instance);
            }
        }

        Ok(instances)
    }

    /// Removes an instance from the database.
    ///
    /// Returns Ok(true) if instance existed and was removed, Ok(false) if
    /// it didn't exist. Idempotent - safe to call multiple times.
    pub fn remove_instance(&self, name: &str) -> Result<bool> {
        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;

        let removed = {
            let mut table = write_txn
                .open_table(INSTANCES_TABLE)
                .context("Failed to open instances table")?;

            table
                .remove(name)
                .with_context(|| format!("Failed to remove instance '{name}'"))?
                .is_some()
        };

        write_txn
            .commit()
            .context("Failed to commit instance removal transaction")?;

        Ok(removed)
    }

    /// Persists an instance to the database asynchronously.
    ///
    /// Async version of `save_instance` that uses `spawn_blocking`.
    #[allow(dead_code)]
    pub async fn save_instance_async(&self, instance: Instance) -> Result<()> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.save_instance(&instance))
            .await
            .context("Task join error")?
    }

    /// Retrieves an instance by name asynchronously.
    ///
    /// Async version of `get_instance` that uses `spawn_blocking`.
    #[allow(dead_code)]
    pub async fn get_instance_async(&self, name: String) -> Result<Option<Instance>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.get_instance(&name))
            .await
            .context("Task join error")?
    }

    /// Lists all instances asynchronously.
    ///
    /// Async version of `list_instances` that uses `spawn_blocking`.
    #[allow(dead_code)]
    pub async fn list_instances_async(&self) -> Result<Vec<Instance>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.list_instances())
            .await
            .context("Task join error")?
    }

    /// Removes an instance asynchronously.
    ///
    /// Async version of `remove_instance` that uses `spawn_blocking`.
    #[allow(dead_code)]
    pub async fn remove_instance_async(&self, name: String) -> Result<bool> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.remove_instance(&name))
            .await
            .context("Task join error")?
    }
}
