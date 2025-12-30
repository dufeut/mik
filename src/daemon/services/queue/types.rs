//! Core types for the queue service.
//!
//! Contains message types, configuration, and internal queue data structures.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// Maximum number of subscribers per topic.
pub(crate) const MAX_SUBSCRIBERS_PER_TOPIC: usize = 1024;

/// Serde helper for `DateTime<Utc>` as RFC3339 string (backward compatible).
pub(crate) mod datetime_rfc3339 {
    use chrono::{DateTime, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.to_rfc3339())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(serde::de::Error::custom)
    }
}

/// Message in a queue with metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueueMessage {
    /// Unique message identifier (UUID v4).
    pub id: String,
    /// Message payload.
    pub data: Vec<u8>,
    /// Timestamp when the message was created.
    #[serde(with = "datetime_rfc3339")]
    pub created_at: DateTime<Utc>,
}

/// A single queue with its messages (internal).
#[derive(Debug)]
pub(crate) struct Queue {
    pub(crate) messages: VecDeque<QueueMessage>,
    max_size: Option<usize>,
}

impl Queue {
    /// Create a new queue with optional size limit.
    pub(crate) fn new(max_size: Option<usize>) -> Self {
        Self {
            messages: VecDeque::new(),
            max_size,
        }
    }

    /// Push a message to the queue.
    pub(crate) fn push(&mut self, message: QueueMessage) -> anyhow::Result<()> {
        if let Some(max) = self.max_size
            && self.messages.len() >= max
        {
            anyhow::bail!("Queue is full (max size: {max})");
        }
        self.messages.push_back(message);
        Ok(())
    }

    /// Pop a message from the front of the queue.
    pub(crate) fn pop(&mut self) -> Option<QueueMessage> {
        self.messages.pop_front()
    }

    /// Peek at the front message without removing it.
    pub(crate) fn peek(&self) -> Option<&QueueMessage> {
        self.messages.front()
    }

    /// Get the number of messages in the queue.
    pub(crate) fn len(&self) -> usize {
        self.messages.len()
    }

    /// Clear all messages from the queue, returning the count.
    pub(crate) fn clear(&mut self) -> usize {
        let count = self.messages.len();
        self.messages.clear();
        count
    }
}

/// Configuration for the queue service.
#[derive(Debug, Clone, Default)]
pub struct QueueConfig {
    /// Enable persistence to disk.
    pub persist: bool,
    /// Path to redb database file (if persistence enabled).
    pub db_path: Option<std::path::PathBuf>,
    /// Maximum size per queue (None = unlimited).
    pub max_queue_size: Option<usize>,
}
