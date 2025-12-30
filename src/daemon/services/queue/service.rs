//! Main queue service implementation.
//!
//! Provides the public `QueueService` API that integrates queue operations,
//! persistence, and pub/sub functionality.

use super::persistence;
use super::pubsub::PubSubManager;
use super::types::{Queue, QueueConfig, QueueMessage};
use anyhow::{Context, Result};
use chrono::Utc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Internal state for the queue service.
pub(crate) struct QueueServiceInner {
    /// All queues indexed by name.
    pub(crate) queues: RwLock<HashMap<String, Queue>>,
    /// Pub/sub manager for topic-based messaging.
    pub(crate) pubsub: PubSubManager,
    /// Configuration.
    pub(crate) config: QueueConfig,
    /// Optional persistence layer.
    pub(crate) db: Option<RwLock<redb::Database>>,
}

/// Embedded queue service.
///
/// Provides in-memory queue operations with optional redb persistence
/// and simple pub/sub functionality for inter-instance communication.
#[derive(Clone)]
pub struct QueueService {
    pub(crate) inner: Arc<QueueServiceInner>,
}

impl QueueService {
    /// Create a new queue service.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Persistence is enabled but `db_path` is not provided
    /// - Database directory cannot be created
    /// - Database file cannot be opened or created
    /// - Persisted queue data cannot be loaded (corruption)
    pub fn new(config: QueueConfig) -> Result<Self> {
        let db = if config.persist {
            let path = config
                .db_path
                .as_ref()
                .context("db_path required when persist is enabled")?;

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).context("Failed to create database directory")?;
            }

            let database =
                redb::Database::create(path).context("Failed to create redb database")?;
            Some(RwLock::new(database))
        } else {
            None
        };

        let service = Self {
            inner: Arc::new(QueueServiceInner {
                queues: RwLock::new(HashMap::new()),
                pubsub: PubSubManager::new(),
                config: config.clone(),
                db,
            }),
        };

        // Load persisted queues if persistence is enabled
        if let Some(ref db_lock) = service.inner.db {
            persistence::load_from_disk(db_lock, &service.inner.queues, config.max_queue_size)?;
        }

        Ok(service)
    }

    /// Push a message to a queue.
    ///
    /// # Errors
    ///
    /// Returns an error if the queue is full (when `max_queue_size` is set)
    /// or if persistence fails.
    pub fn push(&self, queue_name: &str, message: &[u8]) -> Result<String> {
        let msg = QueueMessage {
            id: Uuid::new_v4().to_string(),
            data: message.to_vec(),
            created_at: Utc::now(),
        };

        let msg_id = msg.id.clone();

        // Add to in-memory queue
        {
            let mut queues = self.inner.queues.write();
            let queue = queues
                .entry(queue_name.to_string())
                .or_insert_with(|| Queue::new(self.inner.config.max_queue_size));
            queue.push(msg)?;
        }

        // Persist if enabled
        if let Some(ref db_lock) = self.inner.db {
            persistence::persist_queue(db_lock, &self.inner.queues, queue_name)?;
        }

        Ok(msg_id)
    }

    /// Pop a message from a queue.
    ///
    /// # Errors
    ///
    /// Returns an error if persistence is enabled and the database write fails.
    pub fn pop(&self, queue_name: &str) -> Result<Option<QueueMessage>> {
        let msg = {
            let mut queues = self.inner.queues.write();
            queues.get_mut(queue_name).and_then(Queue::pop)
        };

        // Persist if enabled and message was popped
        if msg.is_some() {
            if let Some(ref db_lock) = self.inner.db {
                persistence::persist_queue(db_lock, &self.inner.queues, queue_name)?;
            }
        }

        Ok(msg)
    }

    /// Peek at the next message without removing it.
    ///
    /// # Errors
    ///
    /// This method is infallible in practice but returns `Result` for API consistency.
    pub fn peek(&self, queue_name: &str) -> Result<Option<QueueMessage>> {
        let queues = self.inner.queues.read();
        Ok(queues.get(queue_name).and_then(|q| q.peek()).cloned())
    }

    /// Get the length of a queue.
    ///
    /// # Errors
    ///
    /// This method is infallible in practice but returns `Result` for API consistency.
    pub fn len(&self, queue_name: &str) -> Result<usize> {
        let queues = self.inner.queues.read();
        Ok(queues.get(queue_name).map_or(0, Queue::len))
    }

    /// Check if a queue is empty.
    ///
    /// # Errors
    ///
    /// This method is infallible in practice but returns `Result` for API consistency.
    pub fn is_empty(&self, queue_name: &str) -> Result<bool> {
        Ok(self.len(queue_name)? == 0)
    }

    /// Clear all messages from a queue.
    ///
    /// # Errors
    ///
    /// Returns an error if persistence is enabled and the database write fails.
    pub fn clear(&self, queue_name: &str) -> Result<usize> {
        let count = {
            let mut queues = self.inner.queues.write();
            queues.get_mut(queue_name).map_or(0, Queue::clear)
        };

        // Persist if enabled
        if count > 0 {
            if let Some(ref db_lock) = self.inner.db {
                persistence::persist_queue(db_lock, &self.inner.queues, queue_name)?;
            }
        }

        Ok(count)
    }

    /// Delete a queue entirely.
    ///
    /// # Errors
    ///
    /// Returns an error if persistence is enabled and the database write fails.
    pub fn delete_queue(&self, queue_name: &str) -> Result<bool> {
        let existed = {
            let mut queues = self.inner.queues.write();
            queues.remove(queue_name).is_some()
        };

        // Remove from disk if enabled
        if existed {
            if let Some(ref db_lock) = self.inner.db {
                persistence::delete_queue_from_disk(db_lock, queue_name)?;
            }
        }

        Ok(existed)
    }

    /// List all queue names.
    pub fn list_queues(&self) -> Vec<String> {
        let queues = self.inner.queues.read();
        queues.keys().cloned().collect()
    }

    /// Publish a message to all subscribers of a topic.
    ///
    /// # Errors
    ///
    /// This method is infallible in practice but returns `Result` for API consistency.
    pub fn publish(&self, topic: &str, message: &[u8]) -> Result<usize> {
        self.inner.pubsub.publish(topic, message)
    }

    /// Subscribe to a topic.
    pub fn subscribe(&self, topic: &str) -> broadcast::Receiver<QueueMessage> {
        self.inner.pubsub.subscribe(topic)
    }

    /// Get the number of active subscribers for a topic.
    pub fn subscriber_count(&self, topic: &str) -> usize {
        self.inner.pubsub.subscriber_count(topic)
    }

    /// List all topic names.
    pub fn list_topics(&self) -> Vec<String> {
        self.inner.pubsub.list_topics()
    }
}
