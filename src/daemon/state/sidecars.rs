//! Sidecar service discovery operations for the state store.
//!
//! Provides CRUD operations for registered sidecar services.

use anyhow::{Context, Result};
use chrono::Utc;
use redb::{ReadableDatabase, ReadableTable};

use super::types::{ServiceType, Sidecar};
use super::{SIDECARS_TABLE, StateStore};

impl StateStore {
    /// Registers a sidecar service.
    ///
    /// Overwrites existing sidecar with same name.
    pub fn save_sidecar(&self, sidecar: &Sidecar) -> Result<()> {
        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;

        {
            let mut table = write_txn
                .open_table(SIDECARS_TABLE)
                .context("Failed to open sidecars table")?;

            let json =
                serde_json::to_vec(sidecar).context("Failed to serialize sidecar to JSON")?;

            table
                .insert(sidecar.name.as_str(), json.as_slice())
                .with_context(|| format!("Failed to insert sidecar '{}'", sidecar.name))?;
        }

        write_txn
            .commit()
            .context("Failed to commit sidecar save transaction")?;

        Ok(())
    }

    /// Retrieves a sidecar by name.
    pub fn get_sidecar(&self, name: &str) -> Result<Option<Sidecar>> {
        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        let table = read_txn
            .open_table(SIDECARS_TABLE)
            .context("Failed to open sidecars table")?;

        let result = table
            .get(name)
            .with_context(|| format!("Failed to read sidecar '{name}'"))?;

        match result {
            Some(guard) => {
                let json = guard.value();
                let sidecar = serde_json::from_slice(json)
                    .with_context(|| format!("Failed to deserialize sidecar '{name}'"))?;
                Ok(Some(sidecar))
            },
            None => Ok(None),
        }
    }

    /// Lists all registered sidecars.
    pub fn list_sidecars(&self) -> Result<Vec<Sidecar>> {
        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        let table = read_txn
            .open_table(SIDECARS_TABLE)
            .context("Failed to open sidecars table")?;

        let mut sidecars = Vec::new();

        for item in table.iter().context("Failed to iterate sidecars table")? {
            let (_, value) = item.context("Failed to read sidecar entry")?;

            if let Ok(sidecar) = serde_json::from_slice::<Sidecar>(value.value()) {
                sidecars.push(sidecar);
            }
        }

        Ok(sidecars)
    }

    /// Lists sidecars by service type.
    pub fn list_sidecars_by_type(&self, service_type: &ServiceType) -> Result<Vec<Sidecar>> {
        let all = self.list_sidecars()?;
        Ok(all
            .into_iter()
            .filter(|s| &s.service_type == service_type)
            .collect())
    }

    /// Removes a sidecar from the registry.
    pub fn remove_sidecar(&self, name: &str) -> Result<bool> {
        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;

        let removed = {
            let mut table = write_txn
                .open_table(SIDECARS_TABLE)
                .context("Failed to open sidecars table")?;

            table
                .remove(name)
                .with_context(|| format!("Failed to remove sidecar '{name}'"))?
                .is_some()
        };

        write_txn
            .commit()
            .context("Failed to commit sidecar removal transaction")?;

        Ok(removed)
    }

    /// Updates the heartbeat timestamp for a sidecar.
    pub fn update_sidecar_heartbeat(&self, name: &str, healthy: bool) -> Result<bool> {
        if let Some(mut sidecar) = self.get_sidecar(name)? {
            sidecar.last_heartbeat = Utc::now();
            sidecar.healthy = healthy;
            self.save_sidecar(&sidecar)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Registers a sidecar service asynchronously.
    pub async fn save_sidecar_async(&self, sidecar: Sidecar) -> Result<()> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.save_sidecar(&sidecar))
            .await
            .context("Task join error")?
    }

    /// Retrieves a sidecar by name asynchronously.
    pub async fn get_sidecar_async(&self, name: String) -> Result<Option<Sidecar>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.get_sidecar(&name))
            .await
            .context("Task join error")?
    }

    /// Lists all registered sidecars asynchronously.
    pub async fn list_sidecars_async(&self) -> Result<Vec<Sidecar>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.list_sidecars())
            .await
            .context("Task join error")?
    }

    /// Lists sidecars by service type asynchronously.
    pub async fn list_sidecars_by_type_async(
        &self,
        service_type: ServiceType,
    ) -> Result<Vec<Sidecar>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.list_sidecars_by_type(&service_type))
            .await
            .context("Task join error")?
    }

    /// Removes a sidecar asynchronously.
    pub async fn remove_sidecar_async(&self, name: String) -> Result<bool> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.remove_sidecar(&name))
            .await
            .context("Task join error")?
    }

    /// Updates the heartbeat timestamp for a sidecar asynchronously.
    pub async fn update_sidecar_heartbeat_async(
        &self,
        name: String,
        healthy: bool,
    ) -> Result<bool> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.update_sidecar_heartbeat(&name, healthy))
            .await
            .context("Task join error")?
    }
}
