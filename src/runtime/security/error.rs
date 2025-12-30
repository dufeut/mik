//! Error types for security validation.
//!
//! This module defines error types for path traversal and module name validation
//! failures. These errors are used to communicate specific security violations
//! detected during input sanitization.

use std::error::Error;
use std::fmt;

/// Error type for path traversal attempts.
///
/// This error is returned when a path fails security validation due to
/// potential directory traversal attacks, reserved names, or other
/// path-based security issues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathTraversalError {
    /// Path contains null bytes.
    NullByte,
    /// Path is empty.
    EmptyPath,
    /// Path is absolute (starts with `/` or drive letter).
    AbsolutePath,
    /// Path tries to escape the base directory using `..`.
    EscapesBaseDirectory,
    /// Path contains a reserved Windows device name (CON, PRN, NUL, etc.).
    ReservedWindowsName,
    /// Path is a Windows UNC path (\\server\share).
    UncPath,
    /// Path contains Windows alternate data stream syntax (`file:stream`).
    AlternateDataStream,
}

impl fmt::Display for PathTraversalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NullByte => write!(f, "Path contains null bytes"),
            Self::EmptyPath => write!(f, "Path is empty"),
            Self::AbsolutePath => write!(f, "Absolute paths are not allowed"),
            Self::EscapesBaseDirectory => {
                write!(f, "Path attempts to escape base directory using '..'")
            },
            Self::ReservedWindowsName => {
                write!(f, "Path contains reserved Windows device name")
            },
            Self::UncPath => write!(f, "UNC paths are not allowed"),
            Self::AlternateDataStream => {
                write!(f, "Alternate data streams are not allowed")
            },
        }
    }
}

impl Error for PathTraversalError {}

/// Error type for invalid module names.
///
/// This error is returned when a module name fails validation due to
/// security concerns such as path separators, control characters, or
/// special directory names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleNameError {
    /// Module name is empty.
    EmptyName,
    /// Module name contains null bytes.
    NullByte,
    /// Module name contains path separators.
    PathSeparator,
    /// Module name is a special directory (`.` or `..`).
    SpecialDirectory,
    /// Module name is too long (> 255 characters).
    TooLong,
    /// Module name contains control characters.
    ControlCharacter,
}

impl fmt::Display for ModuleNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "Module name is empty"),
            Self::NullByte => write!(f, "Module name contains null bytes"),
            Self::PathSeparator => write!(f, "Module name contains path separators"),
            Self::SpecialDirectory => write!(f, "Module name cannot be '.' or '..'"),
            Self::TooLong => write!(f, "Module name is too long (max 255 characters)"),
            Self::ControlCharacter => write!(f, "Module name contains control characters"),
        }
    }
}

impl Error for ModuleNameError {}
