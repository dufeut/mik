//! Key-value store service with pluggable backends.
//!
//! Provides a simple, embedded KV store with TTL support for WASM instances.
//! Supports multiple backends:
//!
//! - **RedbBackend**: Persistent storage with ACID guarantees (default for CLI)
//! - **MemoryBackend**: Fast, non-persistent storage (ideal for testing/embedding)
//!
//! # Example
//!
//! ```ignore
//! use mik::daemon::services::kv::{KvStore, MemoryBackend, RedbBackend};
//!
//! // In-memory (testing/embedding)
//! let store = KvStore::memory();
//! store.set("key", b"value", None).await?;
//!
//! // Persistent (production)
//! let store = KvStore::file("~/.mik/kv.redb")?;
//! store.set("key", b"value", None).await?;
//! ```
//!
//! # Custom Backends
//!
//! Implement the `KvBackend` trait to use custom storage:
//!
//! ```ignore
//! use mik::daemon::services::kv::{KvBackend, KvStore};
//!
//! struct RedisBackend { /* ... */ }
//! impl KvBackend for RedisBackend { /* ... */ }
//!
//! let store = KvStore::custom(RedisBackend::new());
//! ```

mod backend;
mod memory;
mod redb;
mod store;
mod types;

// TODO: Re-enable property tests after converting to async-compatible format
// #[cfg(test)]
// mod property_tests;
#[cfg(test)]
mod tests;

// Re-export the public API
pub use backend::KvBackend;
pub use memory::MemoryBackend;
pub use redb::RedbBackend;
pub use store::KvStore;
