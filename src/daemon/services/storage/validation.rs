//! Path validation and security checks for the storage service.
//!
//! Provides protection against directory traversal attacks and ensures
//! all object paths are safe and normalized.

use anyhow::{Result, bail};
use std::path::{Component, Path, PathBuf};

/// Validates and normalizes an object path to prevent directory traversal.
///
/// # Security
/// Rejects paths that:
/// - Are absolute (start with `/` or drive letter)
/// - Contain `..` components
/// - Contain special components like root or prefix
/// - Are empty
///
/// # Examples
/// ```
/// // Valid paths
/// validate_path("images/logo.png")     // Ok("images/logo.png")
/// validate_path("./data/file.json")    // Ok("data/file.json")
///
/// // Invalid paths
/// validate_path("../etc/passwd")       // Error: path traversal
/// validate_path("/etc/passwd")         // Error: absolute path
/// validate_path("")                    // Error: empty path
/// ```
pub(crate) fn validate_path(path: &str) -> Result<PathBuf> {
    if path.is_empty() {
        bail!("Object path cannot be empty");
    }

    let path = Path::new(path);

    // Reject absolute paths
    if path.is_absolute() {
        bail!("Object path cannot be absolute: {}", path.display());
    }

    // Normalize and check for path traversal attempts
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => normalized.push(name),
            Component::CurDir => {}, // Skip "." components
            Component::ParentDir => {
                bail!("Object path cannot contain '..': {}", path.display())
            },
            Component::RootDir | Component::Prefix(_) => {
                bail!(
                    "Object path cannot contain root or prefix: {}",
                    path.display()
                )
            },
        }
    }

    if normalized.as_os_str().is_empty() {
        bail!("Object path normalized to empty path");
    }

    Ok(normalized)
}

/// Returns the filesystem path for an object given a base directory and object path.
pub(crate) fn object_path(base_dir: &Path, path: &str) -> Result<PathBuf> {
    let normalized = validate_path(path)?;
    Ok(base_dir.join(normalized))
}
