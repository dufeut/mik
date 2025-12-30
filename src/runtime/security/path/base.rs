//! Base directory validation with symlink protection.
//!
//! This module provides validation to ensure paths stay within a base directory
//! even after symlink resolution, preventing TOCTOU (Time-of-Check-Time-of-Use)
//! attacks.
//!
//! # Security Model
//!
//! Symlinks can be used to escape a sandbox directory:
//! 1. Attacker creates a symlink inside the allowed directory
//! 2. Symlink points to a file/directory outside the allowed directory
//! 3. Application follows the symlink and accesses unauthorized content
//!
//! This module prevents such attacks by canonicalizing paths (which resolves
//! all symlinks) and verifying the result stays within the base directory.
//!
//! # Platform Differences
//!
//! - **Unix**: Both file and directory symlinks use the same mechanism
//! - **Windows**: File symlinks (`symlink_file`) and directory symlinks
//!   (`symlink_dir`) are distinct. Creating symlinks typically requires
//!   administrator privileges or Developer Mode enabled.

use std::path::{Path, PathBuf};

use crate::runtime::security::error::PathTraversalError;

/// Validate that a path stays within a base directory after canonicalization.
///
/// This prevents symlink-based path traversal attacks (TOCTOU).
///
/// # Arguments
/// * `base_dir` - The base directory (will be canonicalized)
/// * `file_path` - The path to validate (relative to `base_dir`)
///
/// # Returns
/// * `Ok(canonical_path)` - The canonicalized full path if it's within `base_dir`
/// * `Err(PathTraversalError::EscapesBaseDirectory)` - If the path escapes via symlink
///
/// # Security Notes
///
/// This function:
/// 1. Joins `base_dir` and `file_path` to create the full path
/// 2. Canonicalizes the full path (resolving all symlinks)
/// 3. Canonicalizes the base directory
/// 4. Verifies the canonical full path starts with the canonical base
///
/// For non-existent files, it canonicalizes the parent directory instead.
///
/// # Example
///
/// ```ignore
/// use std::path::Path;
/// use mik::security::validate_path_within_base;
///
/// let base = std::fs::canonicalize("/var/www/static")?;
/// let path = validate_path_within_base(&base, Path::new("myproject/file.txt"))?;
/// // path is guaranteed to be under /var/www/static
/// ```
pub fn validate_path_within_base(
    base_dir: &Path,
    file_path: &Path,
) -> Result<PathBuf, PathTraversalError> {
    let full_path = base_dir.join(file_path);

    // Try to canonicalize - this resolves symlinks
    // If the file doesn't exist, we check the parent directory
    let canonical = if full_path.exists() {
        full_path
            .canonicalize()
            .map_err(|_| PathTraversalError::EscapesBaseDirectory)?
    } else {
        // For non-existent files, canonicalize the parent and append the filename
        let parent = full_path.parent().ok_or(PathTraversalError::EmptyPath)?;
        let filename = full_path.file_name().ok_or(PathTraversalError::EmptyPath)?;

        if parent.exists() {
            let canonical_parent = parent
                .canonicalize()
                .map_err(|_| PathTraversalError::EscapesBaseDirectory)?;
            canonical_parent.join(filename)
        } else {
            // Parent doesn't exist - just use the joined path
            // The file read will fail anyway
            full_path
        }
    };

    // Verify the canonical path starts with the base directory
    // Also canonicalize base_dir to handle symlinks in the base path
    let canonical_base = base_dir
        .canonicalize()
        .map_err(|_| PathTraversalError::EscapesBaseDirectory)?;

    if !canonical.starts_with(&canonical_base) {
        return Err(PathTraversalError::EscapesBaseDirectory);
    }

    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // =========================================================================
    // BASIC VALIDATION TESTS
    // =========================================================================

    #[test]
    fn test_validate_path_within_base_valid() {
        let base = tempdir().unwrap();
        let file_path = base.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        // Valid file within base
        let result = validate_path_within_base(base.path(), Path::new("test.txt"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_within_base_nonexistent() {
        let base = tempdir().unwrap();

        // Non-existent file should still return Ok (let file read handle 404)
        let result = validate_path_within_base(base.path(), Path::new("nonexistent.txt"));
        // Result depends on whether parent exists - base exists so this should work
        assert!(result.is_ok());
    }

    // =========================================================================
    // UNIX SYMLINK TESTS
    // =========================================================================

    #[cfg(unix)]
    #[test]
    fn test_validate_path_within_base_symlink_escape() {
        use std::os::unix::fs::symlink;

        let base = tempdir().unwrap();
        let evil_target = tempdir().unwrap();

        // Create evil file outside base
        let evil_file = evil_target.path().join("secret.txt");
        fs::write(&evil_file, "secret data").unwrap();

        // Create symlink inside base pointing to evil file
        let symlink_path = base.path().join("evil_link");
        symlink(&evil_file, &symlink_path).unwrap();

        // Should reject the symlink that escapes base
        let result = validate_path_within_base(base.path(), Path::new("evil_link"));
        assert_eq!(result, Err(PathTraversalError::EscapesBaseDirectory));
    }

    /// Test nested symlink chain traversal (Unix).
    ///
    /// Multiple levels of symlinks should all be resolved and validated.
    /// A symlink chain that eventually escapes should be blocked.
    #[cfg(unix)]
    #[test]
    fn test_validate_path_within_base_nested_symlink_escape() {
        use std::os::unix::fs::symlink;

        let base = tempdir().unwrap();
        let evil_target = tempdir().unwrap();

        // Create evil file outside base
        let evil_file = evil_target.path().join("secret.txt");
        fs::write(&evil_file, "secret data").unwrap();

        // Create a chain: link1 -> link2 -> evil_file
        let subdir = base.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();

        let link2 = subdir.join("link2");
        symlink(&evil_file, &link2).unwrap();

        let link1 = base.path().join("link1");
        symlink(&link2, &link1).unwrap();

        // Following the chain should detect the escape
        let result = validate_path_within_base(base.path(), Path::new("link1"));
        assert_eq!(result, Err(PathTraversalError::EscapesBaseDirectory));
    }

    /// Test safe symlink that stays within base (Unix).
    ///
    /// Symlinks within the base directory pointing to other files within
    /// the base directory should be allowed.
    #[cfg(unix)]
    #[test]
    fn test_validate_path_within_base_safe_internal_symlink() {
        use std::os::unix::fs::symlink;

        let base = tempdir().unwrap();

        // Create a file in base
        let real_file = base.path().join("real_file.txt");
        fs::write(&real_file, "real data").unwrap();

        // Create a symlink to that file, also in base
        let link_file = base.path().join("link_file.txt");
        symlink(&real_file, &link_file).unwrap();

        // This should be allowed - stays within base
        let result = validate_path_within_base(base.path(), Path::new("link_file.txt"));
        assert!(result.is_ok());
    }

    // =========================================================================
    // WINDOWS SYMLINK TESTS
    // =========================================================================

    /// Test Windows symlink-based path traversal (directory symlink).
    ///
    /// Windows supports both file and directory symlinks. This test verifies
    /// that directory symlinks pointing outside the base directory are rejected.
    #[cfg(windows)]
    #[test]
    fn test_validate_path_within_base_windows_dir_symlink_escape() {
        use std::os::windows::fs::symlink_dir;

        let base = tempdir().unwrap();
        let evil_target = tempdir().unwrap();

        // Create evil directory outside base
        let evil_dir = evil_target.path().join("secrets");
        fs::create_dir_all(&evil_dir).unwrap();
        fs::write(evil_dir.join("secret.txt"), "secret data").unwrap();

        // Create directory symlink inside base pointing to evil directory
        let symlink_path = base.path().join("evil_link");
        if symlink_dir(&evil_dir, &symlink_path).is_ok() {
            // Should reject the symlink that escapes base
            let result = validate_path_within_base(base.path(), Path::new("evil_link/secret.txt"));
            assert_eq!(result, Err(PathTraversalError::EscapesBaseDirectory));

            // Direct symlink reference should also be rejected
            let result = validate_path_within_base(base.path(), Path::new("evil_link"));
            assert_eq!(result, Err(PathTraversalError::EscapesBaseDirectory));
        }
        // If symlink creation fails (e.g., not running as admin), skip the test
    }

    /// Test Windows file symlink-based path traversal.
    ///
    /// Windows file symlinks (as opposed to directory symlinks) pointing
    /// outside the base directory should be rejected.
    #[cfg(windows)]
    #[test]
    fn test_validate_path_within_base_windows_file_symlink_escape() {
        use std::os::windows::fs::symlink_file;

        let base = tempdir().unwrap();
        let evil_target = tempdir().unwrap();

        // Create evil file outside base
        let evil_file = evil_target.path().join("secret.txt");
        fs::write(&evil_file, "secret data").unwrap();

        // Create file symlink inside base pointing to evil file
        let symlink_path = base.path().join("evil_link.txt");
        if symlink_file(&evil_file, &symlink_path).is_ok() {
            // Should reject the symlink that escapes base
            let result = validate_path_within_base(base.path(), Path::new("evil_link.txt"));
            assert_eq!(result, Err(PathTraversalError::EscapesBaseDirectory));
        }
        // If symlink creation fails (e.g., not running as admin), skip the test
    }

    /// Test safe symlink that stays within base (Windows).
    #[cfg(windows)]
    #[test]
    fn test_validate_path_within_base_windows_safe_internal_symlink() {
        use std::os::windows::fs::symlink_file;

        let base = tempdir().unwrap();

        // Create a file in base
        let real_file = base.path().join("real_file.txt");
        fs::write(&real_file, "real data").unwrap();

        // Create a symlink to that file, also in base
        let link_file = base.path().join("link_file.txt");
        if symlink_file(&real_file, &link_file).is_ok() {
            // This should be allowed - stays within base
            let result = validate_path_within_base(base.path(), Path::new("link_file.txt"));
            assert!(result.is_ok());
        }
    }
}
