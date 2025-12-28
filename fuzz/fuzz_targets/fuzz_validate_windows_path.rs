//! Fuzz target for `validate_windows_path` - Windows-specific path validation.
//!
//! This fuzzer tests that:
//! 1. No input causes a panic
//! 2. Reserved Windows names are properly detected
//! 3. UNC paths and alternate data streams are blocked
//!
//! Run with: `cargo +nightly fuzz run fuzz_validate_windows_path`

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mik::security::{validate_windows_path, PathTraversalError};

/// Windows reserved device names.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Structured input for targeted Windows path fuzzing.
#[derive(Arbitrary, Debug)]
struct WindowsPathInput {
    /// Base path string
    base: String,
    /// Type of attack to inject
    attack_type: AttackType,
    /// Additional path component
    suffix: String,
}

#[derive(Arbitrary, Debug)]
enum AttackType {
    /// No attack - normal path
    None,
    /// UNC path (\\server\share)
    UncPath,
    /// Forward slash UNC (//server/share)
    UncPathForward,
    /// Alternate data stream (file:stream)
    AlternateDataStream,
    /// Reserved device name
    ReservedName(ReservedNameType),
    /// Reserved name with extension
    ReservedNameWithExt(ReservedNameType),
    /// Reserved name in subdirectory
    ReservedNameInDir(ReservedNameType),
}

#[derive(Arbitrary, Debug)]
enum ReservedNameType {
    Con,
    Prn,
    Aux,
    Nul,
    Com(u8), // 1-9
    Lpt(u8), // 1-9
}

impl ReservedNameType {
    fn as_str(&self) -> String {
        match self {
            Self::Con => "CON".to_string(),
            Self::Prn => "PRN".to_string(),
            Self::Aux => "AUX".to_string(),
            Self::Nul => "NUL".to_string(),
            Self::Com(n) => format!("COM{}", (n % 9) + 1),
            Self::Lpt(n) => format!("LPT{}", (n % 9) + 1),
        }
    }
}

impl WindowsPathInput {
    fn build(&self) -> String {
        match &self.attack_type {
            AttackType::None => {
                if self.suffix.is_empty() {
                    self.base.clone()
                } else {
                    format!("{}/{}", self.base, self.suffix)
                }
            }
            AttackType::UncPath => {
                format!("\\\\{}", self.base)
            }
            AttackType::UncPathForward => {
                format!("//{}", self.base)
            }
            AttackType::AlternateDataStream => {
                // Inject : at a position that's not index 1 (to avoid C: confusion)
                if self.base.is_empty() {
                    "file:stream".to_string()
                } else {
                    format!("{}:{}", self.base, self.suffix)
                }
            }
            AttackType::ReservedName(name_type) => {
                name_type.as_str()
            }
            AttackType::ReservedNameWithExt(name_type) => {
                format!("{}.txt", name_type.as_str())
            }
            AttackType::ReservedNameInDir(name_type) => {
                format!("{}/{}", self.base, name_type.as_str())
            }
        }
    }
}

fuzz_target!(|data: WindowsPathInput| {
    let path = data.build();

    // The function must never panic
    let result = validate_windows_path(&path);

    match result {
        Ok(()) => {
            // INVARIANT 1: Valid paths must not start with UNC prefix
            assert!(
                !path.starts_with("\\\\") && !path.starts_with("//"),
                "validate_windows_path accepted UNC path: {:?}",
                path
            );

            // INVARIANT 2: Valid paths must not contain reserved device names
            // Check each component of the path
            for component in path.split(['/', '\\']) {
                if component.is_empty() {
                    continue;
                }
                let stem = component.split('.').next().unwrap_or(component);
                let upper_stem = stem.to_uppercase();
                assert!(
                    !RESERVED_NAMES.contains(&upper_stem.as_str()),
                    "validate_windows_path accepted reserved name: {:?} in path {:?}",
                    component,
                    path
                );
            }

            // INVARIANT 3: Valid paths must not have suspicious colons
            // (colons only allowed at position 1 for drive letters)
            if let Some(colon_pos) = path.find(':') {
                assert_eq!(
                    colon_pos, 1,
                    "validate_windows_path accepted colon at position {}: {:?}",
                    colon_pos, path
                );
            }
        }
        Err(e) => {
            // Errors are expected - verify they match the input
            match e {
                PathTraversalError::UncPath => {
                    assert!(
                        path.starts_with("\\\\") || path.starts_with("//"),
                        "UncPath error but path doesn't have UNC prefix: {:?}",
                        path
                    );
                }
                PathTraversalError::ReservedWindowsName => {
                    // Check that at least one component contains a reserved name
                    let has_reserved = path.split(['/', '\\']).any(|component| {
                        if component.is_empty() {
                            return false;
                        }
                        let stem = component.split('.').next().unwrap_or(component);
                        RESERVED_NAMES.contains(&stem.to_uppercase().as_str())
                    });
                    assert!(
                        has_reserved,
                        "ReservedWindowsName error but no reserved names found: {:?}",
                        path
                    );
                }
                PathTraversalError::AlternateDataStream => {
                    // Should have a colon not at position 1
                    let colon_pos = path.find(':');
                    assert!(
                        colon_pos.is_some() && colon_pos != Some(1),
                        "AlternateDataStream error but no suspicious colon: {:?}",
                        path
                    );
                }
                // These errors shouldn't come from validate_windows_path
                PathTraversalError::NullByte
                | PathTraversalError::EmptyPath
                | PathTraversalError::AbsolutePath
                | PathTraversalError::EscapesBaseDirectory => {
                    panic!(
                        "Unexpected error type from validate_windows_path: {:?} for input {:?}",
                        e, path
                    );
                }
            }
        }
    }
});
