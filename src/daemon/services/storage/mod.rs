//! Embedded S3-like object storage service with pluggable backends.
//!
//! Provides filesystem-based or in-memory object storage with metadata tracking.
//! Objects are stored with full CRUD operations and prefix-based listing.
//!
//! # Example
//!
//! ```ignore
//! use mik::daemon::services::storage::StorageService;
//!
//! // In-memory (testing/embedding)
//! let storage = StorageService::memory();
//!
//! // Persistent (production)
//! let storage = StorageService::file("~/.mik/storage")?;
//!
//! // Store an object
//! let meta = storage.put_object("images/logo.png", &image_bytes, Some("image/png")).await?;
//!
//! // List objects
//! let images = storage.list_objects(Some("images/")).await?;
//! ```
//!
//! # Custom Backends
//!
//! Implement the `StorageBackend` trait to use custom storage:
//!
//! ```ignore
//! use mik::daemon::services::storage::{StorageBackend, StorageService};
//!
//! struct S3Backend { /* ... */ }
//! impl StorageBackend for S3Backend { /* ... */ }
//!
//! let storage = StorageService::custom(S3Backend::new());
//! ```
//!
//! # Security
//!
//! All object paths are normalized and validated to prevent directory
//! traversal attacks. Paths containing `..`, absolute paths, or other
//! suspicious components are rejected.

mod backend;
mod filesystem;
mod memory;
mod metadata;
mod service;
mod types;
mod validation;

// Re-export the public API
pub use backend::StorageBackend;
pub use filesystem::FilesystemBackend;
pub use memory::MemoryStorageBackend;
pub use service::StorageService;
pub use types::ObjectMeta;
