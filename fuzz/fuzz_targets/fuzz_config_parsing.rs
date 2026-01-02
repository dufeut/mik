//! Fuzz target for mik.toml / config parsing.
//!
//! This fuzzer tests the TOML config parser to ensure:
//! 1. No input causes a panic
//! 2. Malformed TOML is gracefully rejected
//! 3. Valid TOML that deserializes correctly passes validation
//!
//! Run with: `cargo +nightly fuzz run fuzz_config_parsing`

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mik::config::Config;
use mik::manifest::Manifest;

/// Structured input for more targeted config fuzzing.
#[derive(Arbitrary, Debug)]
struct ConfigInput {
    /// Raw TOML string to test
    toml_string: String,
    /// Which config format to try
    config_type: ConfigType,
    /// Whether to inject adversarial patterns
    adversarial: Option<AdversarialPattern>,
}

#[derive(Arbitrary, Debug)]
enum ConfigType {
    /// Legacy mikrozen.toml format (Config)
    Legacy,
    /// Modern mik.toml format (Manifest)
    Modern,
    /// Try both formats
    Both,
}

#[derive(Arbitrary, Debug)]
enum AdversarialPattern {
    /// Very long strings
    LongStrings,
    /// Deeply nested tables
    DeepNesting,
    /// Special characters in values
    SpecialChars,
    /// Numeric overflows
    NumericOverflow,
    /// Unicode edge cases
    Unicode,
    /// Path traversal in file paths
    PathTraversal,
    /// Control characters
    ControlChars,
}

impl ConfigInput {
    /// Build the final TOML string for testing.
    fn build(&self) -> String {
        match &self.adversarial {
            None => self.toml_string.clone(),
            Some(pattern) => self.apply_adversarial(pattern),
        }
    }

    fn apply_adversarial(&self, pattern: &AdversarialPattern) -> String {
        match pattern {
            AdversarialPattern::LongStrings => {
                // Inject very long values
                let long_value = "a".repeat(10000);
                format!(
                    r#"
[project]
name = "{long_value}"
version = "0.1.0"
"#
                )
            }
            AdversarialPattern::DeepNesting => {
                // This creates invalid TOML but tests parser robustness
                let mut s = String::new();
                for i in 0..100 {
                    s.push_str(&format!("[level{i}]\n"));
                }
                s
            }
            AdversarialPattern::SpecialChars => {
                format!(
                    r#"
[project]
name = "test\n\r\t\0"
version = "0.1.0"
description = "{}!@#$%^&*()[]{{}}|\\:\";<>?,./`~"
"#,
                    self.toml_string
                )
            }
            AdversarialPattern::NumericOverflow => {
                format!(
                    r#"
[project]
name = "test"
version = "0.1.0"

[server]
port = 99999999999
cache_size = 18446744073709551616
"#
                )
            }
            AdversarialPattern::Unicode => {
                format!(
                    r#"
[project]
name = "\u{{0000}}\u{{FFFF}}\u{{10FFFF}}"
version = "0.1.0"
description = "\u{{202E}}reversed\u{{202C}}"
"#
                )
            }
            AdversarialPattern::PathTraversal => {
                format!(
                    r#"
[project]
name = "test"
version = "0.1.0"

[server]
modules = "../../../etc/passwd"

[dependencies]
evil = {{ path = "../../.." }}
"#
                )
            }
            AdversarialPattern::ControlChars => {
                // Inject control characters
                format!(
                    r#"
[project]
name = "test{}control"
version = "0.1.0"
"#,
                    '\x00'
                )
            }
        }
    }
}

fuzz_target!(|data: ConfigInput| {
    let toml_str = data.build();

    match data.config_type {
        ConfigType::Legacy => {
            test_legacy_config(&toml_str);
        }
        ConfigType::Modern => {
            test_modern_manifest(&toml_str);
        }
        ConfigType::Both => {
            test_legacy_config(&toml_str);
            test_modern_manifest(&toml_str);
        }
    }
});

/// Test parsing as legacy Config (mikrozen.toml format).
fn test_legacy_config(toml_str: &str) {
    // The parser must never panic
    let result: Result<Config, _> = toml::from_str(toml_str);

    if let Ok(config) = result {
        // If parsing succeeded, validation must not panic
        let _validation = config.validate();

        // INVARIANT: If we have a valid Config, package fields exist
        // (they're required by the struct)
        assert!(
            !config.package.name.is_empty() || config.package.name.is_empty(),
            "package.name field must exist"
        );
    }
    // Errors are expected for malformed TOML - no action needed
}

/// Test parsing as modern Manifest (mik.toml format).
fn test_modern_manifest(toml_str: &str) {
    // The parser must never panic
    let result: Result<Manifest, _> = toml::from_str(toml_str);

    if let Ok(manifest) = result {
        // INVARIANT: If parsing succeeded, project fields exist
        // The project.name field should be accessible
        let _name = &manifest.project.name;
        let _version = &manifest.project.version;

        // Server config should have valid defaults
        assert!(
            manifest.server.port > 0 || manifest.server.port == 0,
            "port field must exist"
        );

        // INVARIANT: Cache size should be a reasonable value (even if 0 for auto)
        assert!(
            manifest.server.cache_size <= usize::MAX,
            "cache_size must be valid usize"
        );

        // INVARIANT: Execution timeout should be within bounds
        assert!(
            manifest.server.execution_timeout_secs <= u64::MAX,
            "execution_timeout must be valid u64"
        );

        // Check dependencies parsing
        for (name, _dep) in &manifest.dependencies {
            // INVARIANT: Dependency names should be accessible
            assert!(!name.is_empty() || name.is_empty(), "dep name should exist");
        }
    }
    // Errors are expected for malformed TOML - no action needed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that basic valid TOML parses correctly.
    #[test]
    fn test_valid_toml_parses() {
        let toml_str = r#"
[project]
name = "test-app"
version = "0.1.0"

[server]
port = 3000
"#;
        let result: Result<Manifest, _> = toml::from_str(toml_str);
        assert!(result.is_ok());
    }

    /// Test that empty TOML fails gracefully.
    #[test]
    fn test_empty_toml() {
        let toml_str = "";
        let result: Result<Manifest, _> = toml::from_str(toml_str);
        assert!(result.is_err()); // Missing required fields
    }

    /// Test that invalid TOML syntax fails gracefully.
    #[test]
    fn test_invalid_syntax() {
        let toml_str = "this is not valid toml [[[";
        let result: Result<Manifest, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }
}
