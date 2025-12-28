//! Fuzz target for `sanitize_module_name` - module name validation.
//!
//! This fuzzer tests that:
//! 1. No input causes a panic
//! 2. Valid module names are safe for filesystem use
//! 3. Rejected inputs contain dangerous patterns
//!
//! Run with: `cargo +nightly fuzz run fuzz_sanitize_module_name`

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mik::security::{sanitize_module_name, ModuleNameError};

/// Structured input for more targeted fuzzing.
#[derive(Arbitrary, Debug)]
struct ModuleNameInput {
    /// Base module name
    name: String,
    /// Whether to inject path separators
    inject_separator: bool,
    /// Type of separator to inject
    separator_type: SeparatorType,
    /// Whether to inject control characters
    inject_control: bool,
    /// Control character to inject (will be mapped to actual control char)
    control_char_seed: u8,
}

#[derive(Arbitrary, Debug)]
enum SeparatorType {
    ForwardSlash,
    BackSlash,
    Both,
}

impl ModuleNameInput {
    fn build(&self) -> String {
        let mut result = self.name.clone();

        if self.inject_separator {
            let sep = match self.separator_type {
                SeparatorType::ForwardSlash => "/",
                SeparatorType::BackSlash => "\\",
                SeparatorType::Both => "/\\",
            };
            let pos = result.len() / 2;
            result.insert_str(pos.min(result.len()), sep);
        }

        if self.inject_control {
            // Map seed to control characters (0x00-0x1F)
            let control = (self.control_char_seed % 32) as char;
            result.push(control);
        }

        result
    }
}

fuzz_target!(|data: ModuleNameInput| {
    let name = data.build();

    // The function must never panic
    let result = sanitize_module_name(&name);

    match result {
        Ok(sanitized) => {
            // INVARIANT 1: Output equals input (no transformation, just validation)
            assert_eq!(
                sanitized, name,
                "sanitize_module_name transformed input: {:?} -> {:?}",
                name, sanitized
            );

            // INVARIANT 2: Valid name must not be empty
            assert!(
                !sanitized.is_empty(),
                "sanitize_module_name returned empty string"
            );

            // INVARIANT 3: Valid name must not contain path separators
            assert!(
                !sanitized.contains('/') && !sanitized.contains('\\'),
                "sanitized name contains path separator: {:?}",
                sanitized
            );

            // INVARIANT 4: Valid name must not be "." or ".."
            assert!(
                sanitized != "." && sanitized != "..",
                "sanitized name is special directory: {:?}",
                sanitized
            );

            // INVARIANT 5: Valid name must not contain null bytes
            assert!(
                !sanitized.contains('\0'),
                "sanitized name contains null byte: {:?}",
                sanitized
            );

            // INVARIANT 6: Valid name must not contain control characters
            assert!(
                !sanitized.chars().any(char::is_control),
                "sanitized name contains control character: {:?}",
                sanitized
            );

            // INVARIANT 7: Valid name must not exceed 255 characters
            assert!(
                sanitized.len() <= 255,
                "sanitized name exceeds 255 chars: {}",
                sanitized.len()
            );

            // INVARIANT 8: Valid name should be safe for use as filename
            // (This is implicit from the other checks)
        }
        Err(e) => {
            // Errors are expected - verify they match the input
            match e {
                ModuleNameError::EmptyName => {
                    assert!(
                        name.is_empty(),
                        "EmptyName error but name is not empty: {:?}",
                        name
                    );
                }
                ModuleNameError::NullByte => {
                    assert!(
                        name.contains('\0'),
                        "NullByte error but no null in name: {:?}",
                        name
                    );
                }
                ModuleNameError::PathSeparator => {
                    assert!(
                        name.contains('/') || name.contains('\\'),
                        "PathSeparator error but no separator in name: {:?}",
                        name
                    );
                }
                ModuleNameError::SpecialDirectory => {
                    assert!(
                        name == "." || name == "..",
                        "SpecialDirectory error but name is not . or ..: {:?}",
                        name
                    );
                }
                ModuleNameError::TooLong => {
                    assert!(
                        name.len() > 255,
                        "TooLong error but name is {} chars: {:?}",
                        name.len(),
                        name
                    );
                }
                ModuleNameError::ControlCharacter => {
                    assert!(
                        name.chars().any(char::is_control),
                        "ControlCharacter error but no control chars: {:?}",
                        name
                    );
                }
            }
        }
    }
});
