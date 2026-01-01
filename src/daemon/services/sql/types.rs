//! Type definitions for the SQL service.
//!
//! Contains the value types and row structures used across all SQL backends.

use rusqlite::types::ValueRef;
use serde::{Deserialize, Serialize};

/// SQL value types that can be stored in databases.
///
/// Mirrors SQLite's type system for seamless conversion. All types
/// are JSON-serializable for API responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    /// SQL NULL value
    Null,
    /// 64-bit signed integer
    Integer(i64),
    /// 64-bit floating point number
    Real(f64),
    /// UTF-8 text string
    Text(String),
    /// Binary blob data
    Blob(Vec<u8>),
}

impl From<ValueRef<'_>> for Value {
    fn from(value_ref: ValueRef<'_>) -> Self {
        match value_ref {
            ValueRef::Null => Self::Null,
            ValueRef::Integer(i) => Self::Integer(i),
            ValueRef::Real(r) => Self::Real(r),
            ValueRef::Text(t) => Self::Text(String::from_utf8_lossy(t).to_string()),
            ValueRef::Blob(b) => Self::Blob(b.to_vec()),
        }
    }
}

impl Value {
    /// Converts to a rusqlite Value for parameter binding.
    pub fn to_rusqlite(&self) -> rusqlite::types::Value {
        match self {
            Self::Null => rusqlite::types::Value::Null,
            Self::Integer(i) => rusqlite::types::Value::Integer(*i),
            Self::Real(r) => rusqlite::types::Value::Real(*r),
            Self::Text(s) => rusqlite::types::Value::Text(s.clone()),
            Self::Blob(b) => rusqlite::types::Value::Blob(b.clone()),
        }
    }
}

/// A single row returned from a SQL query.
///
/// Contains column names and their corresponding values in order.
/// JSON-serializable for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    /// Column names in order
    pub columns: Vec<String>,
    /// Values in same order as columns
    pub values: Vec<Value>,
}

impl Row {
    /// Creates a new row with the given columns and values.
    ///
    /// # Panics
    ///
    /// Panics if `columns.len()` != `values.len()`. This validation runs in
    /// both debug and release builds to prevent data corruption.
    pub fn new(columns: Vec<String>, values: Vec<Value>) -> Self {
        assert_eq!(
            columns.len(),
            values.len(),
            "Column count ({}) must match value count ({})",
            columns.len(),
            values.len()
        );
        Self { columns, values }
    }

    /// Gets a value by column name, returning None if not found.
    #[allow(dead_code)]
    pub fn get(&self, column: &str) -> Option<&Value> {
        self.columns
            .iter()
            .position(|c| c == column)
            .and_then(|idx| self.values.get(idx))
    }
}
