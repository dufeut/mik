//! Embedded queue service with optional persistence.
//!
//! Provides in-memory queue operations with optional redb persistence
//! and simple pub/sub functionality for inter-instance communication.
//!
//! # Examples
//!
//! ## Basic Queue Operations
//!
//! ```rust
//! use mik::daemon::services::queue::{QueueService, QueueConfig};
//!
//! # fn main() -> anyhow::Result<()> {
//! // Create an in-memory queue service
//! let service = QueueService::new(QueueConfig::default())?;
//!
//! // Push messages
//! let msg_id = service.push("tasks", b"process this")?;
//! service.push("tasks", b"then process this")?;
//!
//! // Pop messages (FIFO order)
//! let msg = service.pop("tasks")?.unwrap();
//! assert_eq!(msg.data, b"process this");
//!
//! // Peek without removing
//! let next = service.peek("tasks")?.unwrap();
//! assert_eq!(next.data, b"then process this");
//! # Ok(())
//! # }
//! ```
//!
//! ## Pub/Sub Pattern
//!
//! ```rust
//! use mik::daemon::services::queue::{QueueService, QueueConfig};
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let service = QueueService::new(QueueConfig::default())?;
//!
//! // Subscribe to a topic
//! let mut subscriber1 = service.subscribe("events");
//! let mut subscriber2 = service.subscribe("events");
//!
//! // Publish a message
//! let count = service.publish("events", b"something happened")?;
//! assert_eq!(count, 2); // 2 active subscribers
//!
//! // Both subscribers receive it
//! let msg1 = subscriber1.recv().await.unwrap();
//! let msg2 = subscriber2.recv().await.unwrap();
//! assert_eq!(msg1.data, msg2.data);
//! # Ok(())
//! # }
//! ```
//!
//! ## Persistent Queues
//!
//! ```rust
//! use mik::daemon::services::queue::{QueueService, QueueConfig};
//! use std::path::PathBuf;
//!
//! # fn main() -> anyhow::Result<()> {
//! # let temp_dir = tempfile::tempdir()?;
//! let config = QueueConfig {
//!     persist: true,
//!     db_path: Some(temp_dir.path().join("queues.db")),
//!     max_queue_size: None,
//! };
//!
//! let service = QueueService::new(config)?;
//! service.push("durable_queue", b"important message")?;
//!
//! // Messages are persisted to disk automatically
//! # Ok(())
//! # }
//! ```

#![allow(dead_code)] // Queue service for future sidecar integration
#![allow(clippy::unnecessary_wraps)]

mod persistence;
mod pubsub;
mod service;
mod types;

// Re-export public API
pub use service::QueueService;
pub use types::{QueueConfig, QueueMessage};

#[cfg(test)]
mod tests;
