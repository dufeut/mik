//! Pub/sub functionality for the queue service.
//!
//! Provides topic-based publish/subscribe pattern using broadcast channels.

use super::types::{MAX_SUBSCRIBERS_PER_TOPIC, QueueMessage};
use anyhow::Result;
use chrono::Utc;
use parking_lot::RwLock;
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Manages pub/sub topics with broadcast channels.
pub(crate) struct PubSubManager {
    /// Topics with their broadcast senders.
    topics: RwLock<HashMap<String, broadcast::Sender<QueueMessage>>>,
}

impl PubSubManager {
    /// Create a new pub/sub manager.
    pub(crate) fn new() -> Self {
        Self {
            topics: RwLock::new(HashMap::new()),
        }
    }

    /// Publish a message to all subscribers of a topic.
    ///
    /// Returns the number of subscribers that received the message.
    pub(crate) fn publish(&self, topic: &str, message: &[u8]) -> Result<usize> {
        let msg = QueueMessage {
            id: Uuid::new_v4().to_string(),
            data: message.to_vec(),
            created_at: Utc::now(),
        };

        let topics = self.topics.read();
        if let Some(sender) = topics.get(topic) {
            // send() returns the number of active receivers
            let count = sender.send(msg).unwrap_or(0);
            Ok(count)
        } else {
            // No subscribers yet
            Ok(0)
        }
    }

    /// Subscribe to a topic.
    ///
    /// Returns a receiver that will receive all messages published to the topic.
    pub(crate) fn subscribe(&self, topic: &str) -> broadcast::Receiver<QueueMessage> {
        let mut topics = self.topics.write();
        let sender = topics
            .entry(topic.to_string())
            .or_insert_with(|| broadcast::channel(MAX_SUBSCRIBERS_PER_TOPIC).0);
        sender.subscribe()
    }

    /// Get the number of active subscribers for a topic.
    pub(crate) fn subscriber_count(&self, topic: &str) -> usize {
        let topics = self.topics.read();
        topics
            .get(topic)
            .map_or(0, tokio::sync::broadcast::Sender::receiver_count)
    }

    /// List all topic names.
    pub(crate) fn list_topics(&self) -> Vec<String> {
        let topics = self.topics.read();
        topics.keys().cloned().collect()
    }
}
