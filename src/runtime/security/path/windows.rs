//! Windows-specific path validation.
//!
//! This module provides validation for Windows-specific path security issues:
//! - Reserved device names (CON, PRN, NUL, COM1-9, LPT1-9)
//! - UNC paths (`\\server\share`)
//! - Alternate Data Streams (`file.txt:stream`)
//!
//! # Defense in Depth
//!
//! This validation is performed on **all platforms** because:
//! - The server might run on Windows
//! - Files might be accessed from Windows clients
//! - Defense in depth against path confusion attacks
//!
//! # Examples
//!
//! ```
//! use mik::security::validate_windows_path;
//!
//! // Valid paths
//! assert!(validate_windows_path("normal_file.txt").is_ok());
//! assert!(validate_windows_path("my_folder/file.txt").is_ok());
//!
//! // Invalid: reserved device names
//! assert!(validate_windows_path("CON").is_err());
//! assert!(validate_windows_path("con.txt").is_err());
//!
//! // Invalid: UNC paths
//! assert!(validate_windows_path("\\\\server\\share").is_err());
//!
//! // Invalid: alternate data streams
//! assert!(validate_windows_path("file.txt:hidden").is_err());
//! ```

use tracing::warn;

use crate::runtime::security::error::PathTraversalError;

/// Reserved Windows device names that can cause issues even with extensions.
///
/// Windows treats these as device names regardless of extension:
/// - `CON`, `PRN`, `AUX`, `NUL`
/// - `COM1` through `COM9`
/// - `LPT1` through `LPT9`
pub const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validates a path for Windows-specific security issues.
///
/// This function checks for:
/// 1. Reserved Windows device names (CON, PRN, NUL, COM1-9, LPT1-9)
/// 2. UNC paths (`\\server\share`)
/// 3. Alternate data streams (`file.txt:stream`)
///
/// This validation is performed on all platforms because:
/// - The server might run on Windows
/// - Files might be accessed from Windows clients
/// - Defense in depth against path confusion attacks
///
/// # Errors
///
/// Returns [`PathTraversalError`] if the path contains:
/// - [`ReservedWindowsName`](PathTraversalError::ReservedWindowsName) - Reserved device name
/// - [`UncPath`](PathTraversalError::UncPath) - UNC path format
/// - [`AlternateDataStream`](PathTraversalError::AlternateDataStream) - NTFS ADS syntax
///
/// # Examples
///
/// ```
/// use mik::security::validate_windows_path;
///
/// // Valid paths
/// assert!(validate_windows_path("normal_file.txt").is_ok());
/// assert!(validate_windows_path("my_folder/file.txt").is_ok());
///
/// // Invalid: reserved device names
/// assert!(validate_windows_path("CON").is_err());
/// assert!(validate_windows_path("con.txt").is_err());
/// assert!(validate_windows_path("folder/NUL").is_err());
///
/// // Invalid: UNC paths
/// assert!(validate_windows_path("\\\\server\\share").is_err());
///
/// // Invalid: alternate data streams
/// assert!(validate_windows_path("file.txt:hidden").is_err());
/// ```
pub fn validate_windows_path(path: &str) -> Result<(), PathTraversalError> {
    // Check for UNC paths (\\server\share or //server/share)
    if path.starts_with("\\\\") || path.starts_with("//") {
        warn!(
            security_event = "windows_path_attack",
            path = %path,
            reason = "unc_path",
            "Blocked UNC path"
        );
        return Err(PathTraversalError::UncPath);
    }

    // Check for alternate data streams (colon in path, except for drive letters)
    // Drive letters like C: are handled by the absolute path check elsewhere
    // But we need to catch things like "file.txt:stream"
    if let Some(colon_pos) = path.find(':') {
        // If colon is not at position 1 (drive letter like C:), it's suspicious
        if colon_pos != 1 {
            warn!(
                security_event = "windows_path_attack",
                path = %path,
                reason = "alternate_data_stream",
                "Blocked path with alternate data stream"
            );
            return Err(PathTraversalError::AlternateDataStream);
        }
    }

    // Check each path component for reserved device names
    for component in path.split(['/', '\\']) {
        if component.is_empty() {
            continue;
        }

        // Get the stem (filename without extension)
        // "CON.txt" -> "CON", "NUL" -> "NUL"
        let stem = component
            .split('.')
            .next()
            .unwrap_or(component)
            .to_uppercase();

        if WINDOWS_RESERVED_NAMES.contains(&stem.as_str()) {
            warn!(
                security_event = "windows_path_attack",
                path = %path,
                component = %component,
                reason = "reserved_device_name",
                "Blocked path with Windows reserved device name"
            );
            return Err(PathTraversalError::ReservedWindowsName);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // RESERVED DEVICE NAME TESTS
    // =========================================================================

    #[test]
    fn test_validate_windows_path_reserved_names() {
        // Reserved device names should be rejected
        assert_eq!(
            validate_windows_path("CON"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("con"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("PRN"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("NUL"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("COM1"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("LPT9"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        // Reserved names with extensions should also be rejected
        assert_eq!(
            validate_windows_path("CON.txt"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("nul.anything"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        // Reserved names in subdirectories
        assert_eq!(
            validate_windows_path("folder/CON"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("folder\\NUL.txt"),
            Err(PathTraversalError::ReservedWindowsName)
        );
    }

    #[test]
    fn test_windows_reserved_names_comprehensive() {
        // All reserved device names
        for name in &["CON", "PRN", "AUX", "NUL"] {
            assert_eq!(
                validate_windows_path(name),
                Err(PathTraversalError::ReservedWindowsName),
                "Should reject {name}"
            );
        }

        // COM ports 1-9
        for i in 1..=9 {
            let name = format!("COM{i}");
            assert_eq!(
                validate_windows_path(&name),
                Err(PathTraversalError::ReservedWindowsName),
                "Should reject {name}"
            );
        }

        // LPT ports 1-9
        for i in 1..=9 {
            let name = format!("LPT{i}");
            assert_eq!(
                validate_windows_path(&name),
                Err(PathTraversalError::ReservedWindowsName),
                "Should reject {name}"
            );
        }
    }

    #[test]
    fn test_windows_reserved_names_case_insensitive() {
        // Case variations
        assert_eq!(
            validate_windows_path("con"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("CoN"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("cON"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("nUl"),
            Err(PathTraversalError::ReservedWindowsName)
        );
    }

    #[test]
    fn test_windows_reserved_with_extensions() {
        // Reserved names with extensions are still dangerous
        assert_eq!(
            validate_windows_path("CON.txt"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("NUL.exe"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("COM1.log"),
            Err(PathTraversalError::ReservedWindowsName)
        );
        assert_eq!(
            validate_windows_path("aux.anything.here"),
            Err(PathTraversalError::ReservedWindowsName)
        );
    }

    // =========================================================================
    // UNC PATH TESTS
    // =========================================================================

    #[test]
    fn test_validate_windows_path_unc() {
        // UNC paths should be rejected
        assert_eq!(
            validate_windows_path("\\\\server\\share"),
            Err(PathTraversalError::UncPath)
        );
        assert_eq!(
            validate_windows_path("//server/share"),
            Err(PathTraversalError::UncPath)
        );
        assert_eq!(
            validate_windows_path("\\\\192.168.1.1\\c$"),
            Err(PathTraversalError::UncPath)
        );
    }

    #[test]
    fn test_windows_unc_paths() {
        // UNC path variations
        assert_eq!(
            validate_windows_path("\\\\server\\share\\file.txt"),
            Err(PathTraversalError::UncPath)
        );
        assert_eq!(
            validate_windows_path("//server/share/file.txt"),
            Err(PathTraversalError::UncPath)
        );
        assert_eq!(
            validate_windows_path("\\\\?\\C:\\path"),
            Err(PathTraversalError::UncPath)
        );
        assert_eq!(
            validate_windows_path("\\\\localhost\\c$"),
            Err(PathTraversalError::UncPath)
        );
    }

    // =========================================================================
    // ALTERNATE DATA STREAM TESTS
    // =========================================================================

    #[test]
    fn test_validate_windows_path_alternate_data_stream() {
        // Alternate data streams should be rejected
        assert_eq!(
            validate_windows_path("file.txt:hidden"),
            Err(PathTraversalError::AlternateDataStream)
        );
        assert_eq!(
            validate_windows_path("file.txt:$DATA"),
            Err(PathTraversalError::AlternateDataStream)
        );
        assert_eq!(
            validate_windows_path("folder/file:stream"),
            Err(PathTraversalError::AlternateDataStream)
        );
    }

    #[test]
    fn test_windows_alternate_data_streams() {
        // ADS syntax
        assert_eq!(
            validate_windows_path("file.txt:Zone.Identifier"),
            Err(PathTraversalError::AlternateDataStream)
        );
        assert_eq!(
            validate_windows_path("file:$DATA"),
            Err(PathTraversalError::AlternateDataStream)
        );
        assert_eq!(
            validate_windows_path("file.txt::$DATA"),
            Err(PathTraversalError::AlternateDataStream)
        );
    }

    // =========================================================================
    // VALID PATHS TESTS
    // =========================================================================

    #[test]
    fn test_validate_windows_path_valid() {
        // Normal paths should be allowed
        assert!(validate_windows_path("normal_file.txt").is_ok());
        assert!(validate_windows_path("my_folder/file.txt").is_ok());
        assert!(validate_windows_path("assets\\images\\logo.png").is_ok());
        // Names that look like reserved but aren't
        assert!(validate_windows_path("CONSOLE.txt").is_ok());
        assert!(validate_windows_path("PRNT.txt").is_ok());
        assert!(validate_windows_path("COM10.txt").is_ok()); // Only COM1-9 are reserved
    }

    #[test]
    fn test_windows_safe_names_similar_to_reserved() {
        // Names that look similar but aren't reserved
        assert!(validate_windows_path("CONN.txt").is_ok()); // Extra N
        assert!(validate_windows_path("PRNT.txt").is_ok()); // Extra T
        assert!(validate_windows_path("AUXILLARY.txt").is_ok()); // Extra chars
        assert!(validate_windows_path("NULLIFY.txt").is_ok()); // Contains NUL
        assert!(validate_windows_path("COM10.txt").is_ok()); // COM10+ not reserved
        assert!(validate_windows_path("COM0.txt").is_ok()); // COM0 not reserved
        assert!(validate_windows_path("LPT0.txt").is_ok()); // LPT0 not reserved
        assert!(validate_windows_path("LPT10.txt").is_ok()); // LPT10+ not reserved
    }
}
