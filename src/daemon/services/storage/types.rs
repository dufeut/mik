//! Types and constants for the storage service.

use chrono::{DateTime, Utc};
use redb::TableDefinition;
use serde::{Deserialize, Serialize};

/// Table for object metadata storage
pub(crate) const OBJECTS_TABLE: TableDefinition<'static, &'static str, &'static [u8]> =
    TableDefinition::new("objects");

/// Metadata for a stored object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectMeta {
    /// Virtual path of the object (e.g., "images/logo.png")
    pub path: String,
    /// Size in bytes
    pub size: u64,
    /// MIME content type (e.g., "image/png", "application/json")
    pub content_type: String,
    /// Timestamp when object was created
    pub created_at: DateTime<Utc>,
    /// Timestamp when object was last modified
    pub modified_at: DateTime<Utc>,
}
