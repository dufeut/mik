//! High-level `SqlService` wrapper over backend implementations.
//!
//! Provides a convenient API that wraps any `SqlBackend` implementation.

use super::backend::SqlBackend;
use super::memory::MemorySqlBackend;
use super::sqlite::SqliteBackend;
use super::types::{Row, Value};
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

/// High-level SQL service interface.
///
/// Wraps a `SqlBackend` implementation and provides a consistent API
/// regardless of the underlying storage mechanism.
///
/// # Thread Safety
///
/// `SqlService` is `Clone` and can be shared across threads. The underlying
/// backend handles concurrent access safely.
///
/// # Example
///
/// ```ignore
/// use mik::daemon::services::sql::SqlService;
///
/// // Create an in-memory database
/// let service = SqlService::memory()?;
///
/// // Create a table
/// service.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)").await?;
///
/// // Insert data
/// service.execute(
///     "INSERT INTO users (name) VALUES (?)",
///     &[Value::Text("Alice".into())]
/// ).await?;
///
/// // Query data
/// let rows = service.query("SELECT * FROM users", &[]).await?;
/// ```
#[derive(Clone)]
pub struct SqlService {
    backend: Arc<dyn SqlBackend>,
}

impl SqlService {
    /// Creates a new `SqlService` backed by a file-based SQLite database.
    ///
    /// This is the default for CLI usage where persistence is required.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or created.
    pub fn file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let backend = SqliteBackend::open(path)?;
        Ok(Self {
            backend: Arc::new(backend),
        })
    }

    /// Creates a new `SqlService` backed by an in-memory SQLite database.
    ///
    /// Ideal for testing, development, and embedded applications.
    /// All data is lost when the process exits.
    ///
    /// # Errors
    ///
    /// Returns an error if the in-memory database cannot be created.
    pub fn memory() -> Result<Self> {
        let backend = MemorySqlBackend::new()?;
        Ok(Self {
            backend: Arc::new(backend),
        })
    }

    /// Creates a new `SqlService` with a custom backend.
    ///
    /// Use this to integrate custom storage backends like PostgreSQL, etc.
    pub fn custom<B: SqlBackend>(backend: B) -> Self {
        Self {
            backend: Arc::new(backend),
        }
    }

    /// Creates a new `SqlService` from a boxed backend.
    ///
    /// Useful when working with trait objects directly.
    pub fn from_boxed(backend: Box<dyn SqlBackend>) -> Self {
        Self {
            backend: Arc::from(backend),
        }
    }

    /// Executes a SELECT query and returns matching rows.
    ///
    /// Accepts parameterized queries to prevent SQL injection.
    ///
    /// # Errors
    ///
    /// Returns an error if query execution fails.
    pub async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>> {
        self.backend.query(sql, params).await
    }

    /// Executes an INSERT, UPDATE, or DELETE statement.
    ///
    /// Returns the number of rows affected.
    ///
    /// # Errors
    ///
    /// Returns an error if statement execution fails.
    pub async fn execute(&self, sql: &str, params: &[Value]) -> Result<usize> {
        self.backend.execute(sql, params).await
    }

    /// Executes multiple SQL statements in a batch.
    ///
    /// Useful for creating tables or running migrations.
    ///
    /// # Errors
    ///
    /// Returns an error if any statement fails.
    pub async fn execute_batch(&self, sql: &str) -> Result<()> {
        self.backend.execute_batch(sql).await
    }

    /// Executes multiple statements atomically in a transaction.
    ///
    /// All statements succeed or all are rolled back.
    ///
    /// # Errors
    ///
    /// Returns an error if any statement fails. On error, all changes are rolled back.
    pub async fn execute_batch_atomic(
        &self,
        statements: Vec<(String, Vec<Value>)>,
    ) -> Result<Vec<usize>> {
        self.backend.execute_batch_atomic(statements).await
    }
}
