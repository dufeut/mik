//! Module name sanitization and validation.
//!
//! This module provides functions for validating and sanitizing module names
//! to prevent directory traversal and injection attacks.

use tracing::warn;

use super::error::ModuleNameError;

/// Sanitize a module name to prevent directory traversal and injection attacks.
///
/// Valid module names:
/// - Must not be empty
/// - Must not contain path separators (`/`, `\`)
/// - Must not contain null bytes
/// - Must not be `.` or `..`
/// - Should be a valid filename
///
/// # Errors
///
/// Returns [`ModuleNameError`] if the name is invalid:
/// - [`EmptyName`](ModuleNameError::EmptyName) - Name is empty
/// - [`NullByte`](ModuleNameError::NullByte) - Name contains null bytes
/// - [`PathSeparator`](ModuleNameError::PathSeparator) - Name contains `/` or `\`
/// - [`SpecialDirectory`](ModuleNameError::SpecialDirectory) - Name is `.` or `..`
/// - [`TooLong`](ModuleNameError::TooLong) - Name exceeds 255 characters
/// - [`ControlCharacter`](ModuleNameError::ControlCharacter) - Name contains control characters
///
/// # Examples
///
/// ```
/// use mik::security::sanitize_module_name;
///
/// // Valid names
/// assert!(sanitize_module_name("api").is_ok());
/// assert!(sanitize_module_name("user-service").is_ok());
/// assert!(sanitize_module_name("my_module").is_ok());
///
/// // Invalid names
/// assert!(sanitize_module_name("../etc/passwd").is_err());
/// assert!(sanitize_module_name("api/users").is_err());
/// assert!(sanitize_module_name("..").is_err());
/// ```
pub fn sanitize_module_name(name: &str) -> Result<String, ModuleNameError> {
    // Check for empty name
    if name.is_empty() {
        return Err(ModuleNameError::EmptyName);
    }

    // Check for null bytes
    if name.contains('\0') {
        warn!(
            security_event = "module_injection_attempt",
            module = %name.replace('\0', "\\0"),
            reason = "null_byte",
            "Blocked module name with null byte"
        );
        return Err(ModuleNameError::NullByte);
    }

    // Check for path separators (Unix and Windows)
    if name.contains('/') || name.contains('\\') {
        warn!(
            security_event = "module_injection_attempt",
            module = %name,
            reason = "path_separator",
            "Blocked module name with path separator"
        );
        return Err(ModuleNameError::PathSeparator);
    }

    // Reject special directory names
    if name == "." || name == ".." {
        warn!(
            security_event = "module_injection_attempt",
            module = %name,
            reason = "special_directory",
            "Blocked special directory as module name"
        );
        return Err(ModuleNameError::SpecialDirectory);
    }

    // Check length (reasonable limit for filenames)
    if name.len() > 255 {
        warn!(
            security_event = "module_injection_attempt",
            module_len = name.len(),
            reason = "too_long",
            "Blocked excessively long module name"
        );
        return Err(ModuleNameError::TooLong);
    }

    // Check for control characters
    if name.chars().any(char::is_control) {
        warn!(
            security_event = "module_injection_attempt",
            module = %name.chars().filter(|c| !c.is_control()).collect::<String>(),
            reason = "control_character",
            "Blocked module name with control characters"
        );
        return Err(ModuleNameError::ControlCharacter);
    }

    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_module_name_valid() {
        assert!(sanitize_module_name("api").is_ok());
        assert!(sanitize_module_name("user-service").is_ok());
        assert!(sanitize_module_name("my_module").is_ok());
        assert!(sanitize_module_name("service123").is_ok());
    }

    #[test]
    fn test_sanitize_module_name_path_separator() {
        assert_eq!(
            sanitize_module_name("api/users"),
            Err(ModuleNameError::PathSeparator)
        );
        assert_eq!(
            sanitize_module_name("..\\..\\etc\\passwd"),
            Err(ModuleNameError::PathSeparator)
        );
    }

    #[test]
    fn test_sanitize_module_name_special_directory() {
        assert_eq!(
            sanitize_module_name(".."),
            Err(ModuleNameError::SpecialDirectory)
        );
        assert_eq!(
            sanitize_module_name("."),
            Err(ModuleNameError::SpecialDirectory)
        );
    }

    #[test]
    fn test_sanitize_module_name_empty() {
        assert_eq!(sanitize_module_name(""), Err(ModuleNameError::EmptyName));
    }

    #[test]
    fn test_sanitize_module_name_null_byte() {
        assert_eq!(
            sanitize_module_name("api\0"),
            Err(ModuleNameError::NullByte)
        );
    }

    #[test]
    fn test_sanitize_module_name_too_long() {
        let long_name = "a".repeat(256);
        assert_eq!(
            sanitize_module_name(&long_name),
            Err(ModuleNameError::TooLong)
        );
    }

    #[test]
    fn test_sanitize_module_name_control_chars() {
        assert_eq!(
            sanitize_module_name("api\x01"),
            Err(ModuleNameError::ControlCharacter)
        );
        assert_eq!(
            sanitize_module_name("api\n"),
            Err(ModuleNameError::ControlCharacter)
        );
    }

    #[test]
    fn test_null_byte_in_module_name() {
        assert_eq!(
            sanitize_module_name("valid\0evil"),
            Err(ModuleNameError::NullByte)
        );
        assert_eq!(sanitize_module_name("\0"), Err(ModuleNameError::NullByte));
        assert_eq!(
            sanitize_module_name("module\0"),
            Err(ModuleNameError::NullByte)
        );
    }

    // =========================================================================
    // MODULE NAME VALIDATION TESTS
    // =========================================================================

    #[test]
    fn test_module_name_valid() {
        // Various valid module name formats
        assert!(sanitize_module_name("my-module").is_ok());
        assert!(sanitize_module_name("module_v2").is_ok());
        assert!(sanitize_module_name("CamelCase").is_ok());
        assert!(sanitize_module_name("UPPERCASE").is_ok());
        assert!(sanitize_module_name("lowercase").is_ok());
        assert!(sanitize_module_name("mix3d-numb3rs_123").is_ok());
        assert!(sanitize_module_name("a").is_ok()); // Single char
        assert!(sanitize_module_name("module.wasm").is_ok()); // With extension
        assert!(sanitize_module_name("module.service.v2").is_ok()); // Multiple dots
    }

    #[test]
    fn test_module_name_invalid_chars() {
        // Path separators
        assert_eq!(
            sanitize_module_name("../bad"),
            Err(ModuleNameError::PathSeparator)
        );
        assert_eq!(
            sanitize_module_name("bad/module"),
            Err(ModuleNameError::PathSeparator)
        );
        assert_eq!(
            sanitize_module_name("bad\\module"),
            Err(ModuleNameError::PathSeparator)
        );
        assert_eq!(
            sanitize_module_name("/absolute"),
            Err(ModuleNameError::PathSeparator)
        );
        assert_eq!(
            sanitize_module_name("\\backslash"),
            Err(ModuleNameError::PathSeparator)
        );
    }

    #[test]
    fn test_module_name_too_long() {
        // Exactly at limit (255 chars) - should pass
        let at_limit = "a".repeat(255);
        assert!(sanitize_module_name(&at_limit).is_ok());

        // One over limit (256 chars) - should fail
        let over_limit = "a".repeat(256);
        assert_eq!(
            sanitize_module_name(&over_limit),
            Err(ModuleNameError::TooLong)
        );

        // Way over limit
        let way_over = "a".repeat(1000);
        assert_eq!(
            sanitize_module_name(&way_over),
            Err(ModuleNameError::TooLong)
        );
    }

    #[test]
    fn test_module_name_control_characters() {
        // Various control characters
        assert_eq!(
            sanitize_module_name("mod\x00ule"), // Null (also caught by null check)
            Err(ModuleNameError::NullByte)
        );
        assert_eq!(
            sanitize_module_name("mod\x01ule"), // SOH
            Err(ModuleNameError::ControlCharacter)
        );
        assert_eq!(
            sanitize_module_name("mod\x1Bule"), // Escape
            Err(ModuleNameError::ControlCharacter)
        );
        assert_eq!(
            sanitize_module_name("module\r"), // Carriage return
            Err(ModuleNameError::ControlCharacter)
        );
        assert_eq!(
            sanitize_module_name("module\n"), // Newline
            Err(ModuleNameError::ControlCharacter)
        );
        assert_eq!(
            sanitize_module_name("module\t"), // Tab
            Err(ModuleNameError::ControlCharacter)
        );
    }

    #[test]
    fn test_module_name_special_directories() {
        assert_eq!(
            sanitize_module_name("."),
            Err(ModuleNameError::SpecialDirectory)
        );
        assert_eq!(
            sanitize_module_name(".."),
            Err(ModuleNameError::SpecialDirectory)
        );
        // But these should be valid (contain dots but aren't special)
        assert!(sanitize_module_name("...").is_ok());
        assert!(sanitize_module_name(".hidden").is_ok());
        assert!(sanitize_module_name("file.txt").is_ok());
    }

    // =========================================================================
    // ERROR TYPE TESTS
    // =========================================================================

    #[test]
    fn test_module_name_error_display() {
        assert_eq!(
            ModuleNameError::EmptyName.to_string(),
            "Module name is empty"
        );
        assert_eq!(
            ModuleNameError::NullByte.to_string(),
            "Module name contains null bytes"
        );
        assert_eq!(
            ModuleNameError::PathSeparator.to_string(),
            "Module name contains path separators"
        );
        assert_eq!(
            ModuleNameError::SpecialDirectory.to_string(),
            "Module name cannot be '.' or '..'"
        );
        assert_eq!(
            ModuleNameError::TooLong.to_string(),
            "Module name is too long (max 255 characters)"
        );
        assert_eq!(
            ModuleNameError::ControlCharacter.to_string(),
            "Module name contains control characters"
        );
    }
}
