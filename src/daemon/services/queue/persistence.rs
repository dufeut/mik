//! Persistence layer for queue service.
//!
//! Handles reading and writing queue data to redb database.

use super::types::{Queue, QueueMessage};
use anyhow::{Context, Result};
use parking_lot::RwLock;
use redb::{ReadableDatabase, ReadableTable};
use std::collections::{HashMap, VecDeque};

/// Table definition for queue storage.
const TABLE: redb::TableDefinition<'static, &'static str, &'static [u8]> =
    redb::TableDefinition::new("queues");

/// Persist a single queue to disk.
pub(crate) fn persist_queue(
    db_lock: &RwLock<redb::Database>,
    queues: &RwLock<HashMap<String, Queue>>,
    queue_name: &str,
) -> Result<()> {
    let db = db_lock.read();
    let write_txn = db.begin_write()?;

    {
        let mut table = write_txn.open_table(TABLE)?;

        // Serialize queue messages
        let queues = queues.read();
        if let Some(queue) = queues.get(queue_name) {
            let serialized =
                serde_json::to_vec(&queue.messages).context("Failed to serialize queue")?;
            table.insert(queue_name, serialized.as_slice())?;
        } else {
            // Queue was deleted, remove from db
            table.remove(queue_name)?;
        }
    }

    write_txn.commit()?;
    Ok(())
}

/// Delete a queue from disk.
pub(crate) fn delete_queue_from_disk(
    db_lock: &RwLock<redb::Database>,
    queue_name: &str,
) -> Result<()> {
    let db = db_lock.read();
    let write_txn = db.begin_write()?;

    {
        let mut table = write_txn.open_table(TABLE)?;
        table.remove(queue_name)?;
    }

    write_txn.commit()?;
    Ok(())
}

/// Load all queues from disk.
pub(crate) fn load_from_disk(
    db_lock: &RwLock<redb::Database>,
    queues: &RwLock<HashMap<String, Queue>>,
    max_queue_size: Option<usize>,
) -> Result<()> {
    let db = db_lock.read();
    let read_txn = db.begin_read()?;

    // Check if table exists
    let table = match read_txn.open_table(TABLE) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => {
            // Table doesn't exist yet, nothing to load
            return Ok(());
        },
        Err(e) => return Err(e.into()),
    };

    let mut queues = queues.write();

    for result in table.iter()? {
        let (name, data) = result?;
        let name_str = name.value();
        let messages: VecDeque<QueueMessage> = serde_json::from_slice(data.value())
            .with_context(|| format!("Failed to deserialize queue '{name_str}'"))?;

        let mut queue = Queue::new(max_queue_size);
        queue.messages = messages;
        queues.insert(name_str.to_string(), queue);
    }

    Ok(())
}
