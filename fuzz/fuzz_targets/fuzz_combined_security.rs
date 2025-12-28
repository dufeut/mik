//! Combined fuzz target for security functions.
//!
//! This fuzzer tests combinations of security functions to ensure
//! they work correctly together and maintain invariants.
//!
//! Run with: `cargo +nightly fuzz run fuzz_combined_security`

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mik::security::{sanitize_file_path, sanitize_module_name, validate_windows_path};

/// Combined security input for testing multiple functions.
#[derive(Arbitrary, Debug)]
struct SecurityInput {
    /// Input string to test
    input: String,
    /// Type of security test to perform
    test_type: SecurityTestType,
}

#[derive(Arbitrary, Debug)]
enum SecurityTestType {
    /// Test file path sanitization
    FilePath,
    /// Test module name sanitization
    ModuleName,
    /// Test Windows path validation
    WindowsPath,
    /// Test all functions with same input
    All,
    /// Test with adversarial patterns
    Adversarial(AdversarialPattern),
}

#[derive(Arbitrary, Debug)]
enum AdversarialPattern {
    /// Double encoding (%252e%252e)
    DoubleEncoding,
    /// Unicode normalization attacks
    UnicodeNormalization,
    /// Case variation (CoN vs CON)
    CaseVariation,
    /// Null byte injection
    NullByteInjection,
    /// Very long path
    LongPath,
    /// Many path components
    ManyComponents,
    /// Mixed separators (/ and \)
    MixedSeparators,
    /// Trailing/leading dots
    DotManipulation,
    /// Space padding
    SpacePadding,
}

impl SecurityInput {
    /// Generate adversarial input based on pattern.
    fn adversarial_input(&self, pattern: &AdversarialPattern) -> String {
        match pattern {
            AdversarialPattern::DoubleEncoding => {
                // %2e%2e -> .. after one decode, but our functions work on decoded input
                format!("%2e%2e/{}", self.input)
            }
            AdversarialPattern::UnicodeNormalization => {
                // Try Unicode lookalikes for . and /
                // U+2024 ONE DOT LEADER, U+2215 DIVISION SLASH
                format!("\u{2024}\u{2024}\u{2215}{}", self.input)
            }
            AdversarialPattern::CaseVariation => {
                // Mixed case for reserved names
                format!("CoN.{}", self.input)
            }
            AdversarialPattern::NullByteInjection => {
                format!("{}\0.txt", self.input)
            }
            AdversarialPattern::LongPath => {
                // Very long path segment
                let segment = "a".repeat(300);
                format!("{}/{}", segment, self.input)
            }
            AdversarialPattern::ManyComponents => {
                // Many nested directories
                let components = (0..50).map(|i| format!("d{i}")).collect::<Vec<_>>().join("/");
                format!("{}/{}", components, self.input)
            }
            AdversarialPattern::MixedSeparators => {
                format!("a/b\\c/{}", self.input)
            }
            AdversarialPattern::DotManipulation => {
                format!(".../{}/...", self.input)
            }
            AdversarialPattern::SpacePadding => {
                format!("  {}  ", self.input)
            }
        }
    }
}

fuzz_target!(|data: SecurityInput| {
    match data.test_type {
        SecurityTestType::FilePath => {
            test_file_path(&data.input);
        }
        SecurityTestType::ModuleName => {
            test_module_name(&data.input);
        }
        SecurityTestType::WindowsPath => {
            test_windows_path(&data.input);
        }
        SecurityTestType::All => {
            test_file_path(&data.input);
            test_module_name(&data.input);
            test_windows_path(&data.input);
        }
        SecurityTestType::Adversarial(ref pattern) => {
            let adversarial = data.adversarial_input(pattern);
            test_file_path(&adversarial);
            test_module_name(&adversarial);
            test_windows_path(&adversarial);
        }
    }
});

/// Test sanitize_file_path with invariant checks.
fn test_file_path(input: &str) {
    let result = sanitize_file_path(input);

    if let Ok(ref path) = result {
        // INVARIANT: No path traversal in output
        let path_str = path.to_string_lossy();
        assert!(
            !path_str.starts_with(".."),
            "sanitized path starts with ..: {:?}",
            path
        );

        // INVARIANT: Not absolute
        assert!(!path.is_absolute(), "sanitized path is absolute: {:?}", path);

        // INVARIANT: No null bytes
        assert!(
            !path_str.contains('\0'),
            "sanitized path has null: {:?}",
            path
        );
    }
}

/// Test sanitize_module_name with invariant checks.
fn test_module_name(input: &str) {
    let result = sanitize_module_name(input);

    if let Ok(ref name) = result {
        // INVARIANT: No path separators
        assert!(
            !name.contains('/') && !name.contains('\\'),
            "module name has separator: {:?}",
            name
        );

        // INVARIANT: Not empty
        assert!(!name.is_empty(), "module name is empty");

        // INVARIANT: Not special directory
        assert!(name != "." && name != "..", "module name is special: {:?}", name);

        // INVARIANT: No control characters
        assert!(
            !name.chars().any(char::is_control),
            "module name has control char: {:?}",
            name
        );

        // INVARIANT: Reasonable length
        assert!(name.len() <= 255, "module name too long: {}", name.len());
    }
}

/// Test validate_windows_path with invariant checks.
fn test_windows_path(input: &str) {
    let result = validate_windows_path(input);

    if result.is_ok() {
        // INVARIANT: No UNC paths accepted
        assert!(
            !input.starts_with("\\\\") && !input.starts_with("//"),
            "accepted UNC path: {:?}",
            input
        );

        // INVARIANT: No reserved names in components
        const RESERVED: &[&str] = &[
            "CON", "PRN", "AUX", "NUL",
            "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
            "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        for component in input.split(['/', '\\']) {
            if component.is_empty() {
                continue;
            }
            let stem = component.split('.').next().unwrap_or(component);
            assert!(
                !RESERVED.contains(&stem.to_uppercase().as_str()),
                "accepted reserved name: {:?} in {:?}",
                component,
                input
            );
        }

        // INVARIANT: No alternate data streams (colon only at position 1)
        if let Some(pos) = input.find(':') {
            assert_eq!(
                pos, 1,
                "accepted alternate data stream: colon at {} in {:?}",
                pos, input
            );
        }
    }
}
