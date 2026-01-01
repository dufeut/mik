//! Embedded SQL service with pluggable backends.
//!
//! Provides a SQL interface backed by SQLite or custom backends for WASM
//! instances to persist structured data.
//!
//! # Example
//!
//! ```ignore
//! use mik::daemon::services::sql::{SqlService, Value};
//!
//! // In-memory (testing/embedding)
//! let service = SqlService::memory()?;
//!
//! // Persistent (production)
//! let service = SqlService::file("~/.mik/data.db")?;
//!
//! // Create table and insert data
//! service.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)").await?;
//! service.execute(
//!     "INSERT INTO users (name) VALUES (?)",
//!     &[Value::Text("Alice".to_string())]
//! ).await?;
//!
//! // Query data
//! let rows = service.query("SELECT * FROM users WHERE id = ?", &[Value::Integer(1)]).await?;
//! ```
//!
//! # Custom Backends
//!
//! Implement the `SqlBackend` trait to use custom storage:
//!
//! ```ignore
//! use mik::daemon::services::sql::{SqlBackend, SqlService};
//!
//! struct PostgresBackend { /* ... */ }
//! impl SqlBackend for PostgresBackend { /* ... */ }
//!
//! let service = SqlService::custom(PostgresBackend::new());
//! ```

mod backend;
mod memory;
mod service;
mod sqlite;
mod types;

// Re-export the public API
pub use backend::SqlBackend;
pub use memory::MemorySqlBackend;
pub use service::SqlService;
pub use sqlite::SqliteBackend;
pub use types::{Row, Value};
