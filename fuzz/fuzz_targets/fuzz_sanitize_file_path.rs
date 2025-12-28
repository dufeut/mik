//! Fuzz target for `sanitize_file_path` - path traversal prevention.
//!
//! This fuzzer tests that:
//! 1. No input causes a panic
//! 2. Valid outputs are always safe (no path traversal)
//! 3. Rejected inputs contain dangerous patterns
//!
//! Run with: `cargo +nightly fuzz run fuzz_sanitize_file_path`

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mik::security::{sanitize_file_path, PathTraversalError};
use std::path::Path;

/// Structured input for more targeted fuzzing.
#[derive(Arbitrary, Debug)]
struct PathInput {
    /// Raw path string to test
    path: String,
    /// Whether to inject null bytes
    inject_null: bool,
    /// Whether to inject path traversal patterns
    inject_traversal: bool,
    /// Number of `..` components to inject
    traversal_depth: u8,
}

impl PathInput {
    /// Build the final path string for testing.
    fn build(&self) -> String {
        let mut result = self.path.clone();

        if self.inject_null {
            // Inject null byte at various positions
            let pos = result.len() / 2;
            result.insert(pos.min(result.len()), '\0');
        }

        if self.inject_traversal {
            let traversal = "../".repeat(self.traversal_depth as usize);
            result = format!("{traversal}{result}");
        }

        result
    }
}

fuzz_target!(|data: PathInput| {
    let path = data.build();

    // The function must never panic
    let result = sanitize_file_path(&path);

    match result {
        Ok(sanitized) => {
            // INVARIANT 1: Valid output must not be empty
            assert!(
                !sanitized.as_os_str().is_empty(),
                "sanitize_file_path returned empty path for input: {:?}",
                path
            );

            // INVARIANT 2: Valid output must not contain ".." components that escape
            // We verify this by checking the normalized path doesn't start with ".."
            let sanitized_str = sanitized.to_string_lossy();
            assert!(
                !sanitized_str.starts_with(".."),
                "sanitized path starts with '..': {:?} from input {:?}",
                sanitized_str,
                path
            );

            // INVARIANT 3: Valid output must not be absolute
            assert!(
                !sanitized.is_absolute(),
                "sanitize_file_path returned absolute path: {:?}",
                sanitized
            );

            // INVARIANT 4: Valid output must not contain null bytes
            assert!(
                !sanitized_str.contains('\0'),
                "sanitized path contains null byte: {:?}",
                sanitized
            );

            // INVARIANT 5: Number of components should be reasonable
            let component_count = sanitized.components().count();
            assert!(
                component_count <= path.len(),
                "component count explosion: {} components from {} char input",
                component_count,
                path.len()
            );
        }
        Err(e) => {
            // Errors are expected for malicious input - verify they make sense
            match e {
                PathTraversalError::NullByte => {
                    // If NullByte error, input should contain null
                    assert!(
                        path.contains('\0'),
                        "NullByte error but no null in input: {:?}",
                        path
                    );
                }
                PathTraversalError::EmptyPath => {
                    // Empty path error - original might be empty or only "."
                    // (which normalizes to empty)
                }
                PathTraversalError::AbsolutePath => {
                    // Should only happen for absolute paths
                    let p = Path::new(&path);
                    // Note: Windows paths like C:\ are also absolute
                    let looks_absolute = p.is_absolute()
                        || path.starts_with('/')
                        || (path.len() >= 2 && path.chars().nth(1) == Some(':'));
                    assert!(
                        looks_absolute,
                        "AbsolutePath error but path doesn't look absolute: {:?}",
                        path
                    );
                }
                PathTraversalError::EscapesBaseDirectory => {
                    // Should contain ".." that escapes
                    assert!(
                        path.contains(".."),
                        "EscapesBaseDirectory but no '..' in input: {:?}",
                        path
                    );
                }
                PathTraversalError::ReservedWindowsName => {
                    // Should contain a reserved name (CON, PRN, NUL, etc.)
                    let upper = path.to_uppercase();
                    let has_reserved = ["CON", "PRN", "AUX", "NUL"]
                        .iter()
                        .chain(&["COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9"])
                        .chain(&["LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"])
                        .any(|reserved| {
                            upper.contains(reserved)
                        });
                    assert!(
                        has_reserved,
                        "ReservedWindowsName but no reserved name in input: {:?}",
                        path
                    );
                }
                PathTraversalError::UncPath => {
                    // Should start with \\ or //
                    assert!(
                        path.starts_with("\\\\") || path.starts_with("//"),
                        "UncPath error but path doesn't start with UNC prefix: {:?}",
                        path
                    );
                }
                PathTraversalError::AlternateDataStream => {
                    // Should contain : (not at position 1 for drive letter)
                    let colon_pos = path.find(':');
                    assert!(
                        colon_pos.is_some() && colon_pos != Some(1),
                        "AlternateDataStream but no suspicious colon: {:?}",
                        path
                    );
                }
            }
        }
    }
});
