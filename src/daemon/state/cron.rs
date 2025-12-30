//! Cron job storage operations for the state store.
//!
//! Provides CRUD operations for scheduled job configurations.

use anyhow::{Context, Result};
use redb::{ReadableDatabase, ReadableTable};

use crate::daemon::cron::ScheduleConfig;

use super::{CRON_JOBS_TABLE, StateStore};

impl StateStore {
    /// Persists a cron job to the database.
    ///
    /// Overwrites existing job with same name.
    pub fn save_cron_job(&self, config: &ScheduleConfig) -> Result<()> {
        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;

        {
            let mut table = write_txn
                .open_table(CRON_JOBS_TABLE)
                .context("Failed to open cron_jobs table")?;

            let json =
                serde_json::to_vec(config).context("Failed to serialize cron job to JSON")?;

            table
                .insert(config.name.as_str(), json.as_slice())
                .with_context(|| format!("Failed to insert cron job '{}'", config.name))?;
        }

        write_txn
            .commit()
            .context("Failed to commit cron job save transaction")?;

        Ok(())
    }

    /// Retrieves a cron job by name.
    #[allow(dead_code)]
    pub fn get_cron_job(&self, name: &str) -> Result<Option<ScheduleConfig>> {
        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        let table = read_txn
            .open_table(CRON_JOBS_TABLE)
            .context("Failed to open cron_jobs table")?;

        let result = table
            .get(name)
            .with_context(|| format!("Failed to read cron job '{name}'"))?;

        match result {
            Some(guard) => {
                let json = guard.value();
                let config = serde_json::from_slice(json)
                    .with_context(|| format!("Failed to deserialize cron job '{name}'"))?;
                Ok(Some(config))
            },
            None => Ok(None),
        }
    }

    /// Lists all cron jobs in the database.
    pub fn list_cron_jobs(&self) -> Result<Vec<ScheduleConfig>> {
        let read_txn = self
            .db
            .begin_read()
            .context("Failed to begin read transaction")?;

        let table = read_txn
            .open_table(CRON_JOBS_TABLE)
            .context("Failed to open cron_jobs table")?;

        let mut jobs = Vec::new();

        for item in table.iter().context("Failed to iterate cron_jobs table")? {
            let (_, value) = item.context("Failed to read cron job entry")?;

            if let Ok(config) = serde_json::from_slice::<ScheduleConfig>(value.value()) {
                jobs.push(config);
            }
        }

        Ok(jobs)
    }

    /// Removes a cron job from the database.
    pub fn remove_cron_job(&self, name: &str) -> Result<bool> {
        let write_txn = self
            .db
            .begin_write()
            .context("Failed to begin write transaction")?;

        let removed = {
            let mut table = write_txn
                .open_table(CRON_JOBS_TABLE)
                .context("Failed to open cron_jobs table")?;

            table
                .remove(name)
                .with_context(|| format!("Failed to remove cron job '{name}'"))?
                .is_some()
        };

        write_txn
            .commit()
            .context("Failed to commit cron job remove transaction")?;

        Ok(removed)
    }

    /// Persists a cron job asynchronously.
    pub async fn save_cron_job_async(&self, config: ScheduleConfig) -> Result<()> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.save_cron_job(&config))
            .await
            .context("Task join error")?
    }

    /// Lists all cron jobs asynchronously.
    pub async fn list_cron_jobs_async(&self) -> Result<Vec<ScheduleConfig>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.list_cron_jobs())
            .await
            .context("Task join error")?
    }

    /// Removes a cron job asynchronously.
    pub async fn remove_cron_job_async(&self, name: String) -> Result<bool> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.remove_cron_job(&name))
            .await
            .context("Task join error")?
    }
}
