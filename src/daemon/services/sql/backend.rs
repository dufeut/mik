//! Backend trait for the SQL service.
//!
//! Defines the interface that all SQL storage backends must implement,
//! enabling pluggable storage (SQLite, in-memory, PostgreSQL, etc.).

use super::types::{Row, Value};
use anyhow::Result;
use async_trait::async_trait;

/// Backend trait for SQL storage.
///
/// All backends must be thread-safe (`Send + Sync`) for use with tokio.
/// Implementations should handle their own concurrency and provide
/// appropriate transaction guarantees where applicable.
///
/// # Example
///
/// ```ignore
/// use mik::daemon::services::sql::{SqlBackend, MemorySqlBackend};
///
/// let backend = MemorySqlBackend::new();
/// backend.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)", &[]).await?;
/// backend.execute("INSERT INTO users (name) VALUES (?)", &[Value::Text("Alice".into())]).await?;
/// let rows = backend.query("SELECT * FROM users", &[]).await?;
/// ```
#[async_trait]
pub trait SqlBackend: Send + Sync + 'static {
    /// Executes a SELECT query and returns matching rows.
    ///
    /// Accepts parameterized queries to prevent SQL injection. Parameters
    /// are bound in order using ? placeholders.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Query preparation fails (SQL syntax error)
    /// - Query execution or result fetching fails
    async fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>>;

    /// Executes an INSERT, UPDATE, or DELETE statement.
    ///
    /// Returns the number of rows affected. Accepts parameterized queries
    /// to prevent SQL injection.
    ///
    /// # Errors
    ///
    /// Returns an error if the statement execution fails (constraint violation, SQL error).
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<usize>;

    /// Executes multiple SQL statements in a batch.
    ///
    /// Useful for creating tables or running migration scripts. Statements
    /// are separated by semicolons. Does not support parameterization -
    /// use only with trusted SQL (e.g., schema definitions).
    ///
    /// # Errors
    ///
    /// Returns an error if any statement in the batch fails to execute.
    async fn execute_batch(&self, sql: &str) -> Result<()>;

    /// Executes multiple statements atomically in a transaction.
    ///
    /// All statements succeed or all are rolled back. Returns the number of
    /// rows affected by each statement in order.
    ///
    /// # Errors
    ///
    /// Returns an error if any statement fails. On error, all changes are rolled back.
    async fn execute_batch_atomic(
        &self,
        statements: Vec<(String, Vec<Value>)>,
    ) -> Result<Vec<usize>>;
}
