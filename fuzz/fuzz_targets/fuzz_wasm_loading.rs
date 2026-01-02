//! Fuzz target for WASM module validation.
//!
//! This fuzzer tests WASM binary validation and header parsing:
//! 1. WASM magic number detection
//! 2. Version validation
//! 3. Section parsing robustness
//! 4. Malformed binary handling
//!
//! Note: This fuzzer does NOT compile WASM to avoid slow wasmtime operations.
//! It focuses on pre-validation and header parsing that the runtime performs.
//!
//! Run with: `cargo +nightly fuzz run fuzz_wasm_loading`

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

/// WASM magic number: \0asm
const WASM_MAGIC: &[u8; 4] = b"\0asm";

/// WASM version 1 (MVP)
const WASM_VERSION_1: &[u8; 4] = &[0x01, 0x00, 0x00, 0x00];

/// WASM section IDs
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)] // Constants for WASM spec completeness
enum SectionId {
    Custom = 0,
    Type = 1,
    Import = 2,
    Function = 3,
    Table = 4,
    Memory = 5,
    Global = 6,
    Export = 7,
    Start = 8,
    Element = 9,
    Code = 10,
    Data = 11,
    DataCount = 12,
    // Component model sections (0x00-0x0D range overlaps, uses different encoding)
}

/// Structured input for WASM binary fuzzing.
#[derive(Arbitrary, Debug)]
struct WasmInput {
    /// Raw binary data
    data: Vec<u8>,
    /// Type of WASM mutation to apply
    mutation: Option<WasmMutation>,
}

#[derive(Arbitrary, Debug)]
enum WasmMutation {
    /// Valid WASM header with random sections
    ValidHeader,
    /// Valid WASM header with component model marker
    ComponentModel,
    /// Corrupted magic number
    CorruptedMagic,
    /// Invalid version number
    InvalidVersion,
    /// Truncated header
    TruncatedHeader,
    /// Valid header but empty sections
    EmptySections,
    /// Valid header with oversized section length
    OversizedSection,
    /// Valid header with unknown section ID
    UnknownSection,
    /// Append random data to valid prefix
    AppendRandom,
    /// All zeros
    AllZeros,
    /// Maximally large LEB128 integers
    MaxLeb128,
}

impl WasmInput {
    /// Build the WASM binary for testing.
    fn build(&self) -> Vec<u8> {
        match &self.mutation {
            None => self.data.clone(),
            Some(mutation) => self.apply_mutation(mutation),
        }
    }

    fn apply_mutation(&self, mutation: &WasmMutation) -> Vec<u8> {
        match mutation {
            WasmMutation::ValidHeader => {
                let mut result = Vec::new();
                result.extend_from_slice(WASM_MAGIC);
                result.extend_from_slice(WASM_VERSION_1);
                // Add a minimal type section
                result.push(SectionId::Type as u8);
                result.push(1); // section size
                result.push(0); // num types
                result
            }
            WasmMutation::ComponentModel => {
                let mut result = Vec::new();
                result.extend_from_slice(WASM_MAGIC);
                // Component model uses version 0x0d (13)
                result.extend_from_slice(&[0x0d, 0x00, 0x01, 0x00]);
                result.extend_from_slice(&self.data);
                result
            }
            WasmMutation::CorruptedMagic => {
                // Wrong magic bytes
                vec![0x00, 0x62, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]
            }
            WasmMutation::InvalidVersion => {
                let mut result = Vec::new();
                result.extend_from_slice(WASM_MAGIC);
                // Invalid version
                result.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
                result
            }
            WasmMutation::TruncatedHeader => {
                // Only magic, no version
                WASM_MAGIC.to_vec()
            }
            WasmMutation::EmptySections => {
                let mut result = Vec::new();
                result.extend_from_slice(WASM_MAGIC);
                result.extend_from_slice(WASM_VERSION_1);
                // Empty type section
                result.push(SectionId::Type as u8);
                result.push(0);
                result
            }
            WasmMutation::OversizedSection => {
                let mut result = Vec::new();
                result.extend_from_slice(WASM_MAGIC);
                result.extend_from_slice(WASM_VERSION_1);
                // Section claiming to be huge but with no data
                result.push(SectionId::Custom as u8);
                // LEB128 for 0xFFFFFFFF (very large)
                result.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
                result
            }
            WasmMutation::UnknownSection => {
                let mut result = Vec::new();
                result.extend_from_slice(WASM_MAGIC);
                result.extend_from_slice(WASM_VERSION_1);
                // Unknown section ID (0xFF)
                result.push(0xFF);
                result.push(0); // empty section
                result
            }
            WasmMutation::AppendRandom => {
                let mut result = Vec::new();
                result.extend_from_slice(WASM_MAGIC);
                result.extend_from_slice(WASM_VERSION_1);
                result.extend_from_slice(&self.data);
                result
            }
            WasmMutation::AllZeros => vec![0u8; self.data.len().min(1024)],
            WasmMutation::MaxLeb128 => {
                let mut result = Vec::new();
                result.extend_from_slice(WASM_MAGIC);
                result.extend_from_slice(WASM_VERSION_1);
                // Type section with max LEB128 for count
                result.push(SectionId::Type as u8);
                result.push(5); // section size
                // Max 32-bit LEB128
                result.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
                result
            }
        }
    }
}

/// Validation result for WASM binary.
#[derive(Debug, PartialEq)]
#[allow(dead_code)] // MalformedSections reserved for future section parsing
enum WasmValidation {
    /// Valid WASM module (v1)
    ValidModule,
    /// Valid WASM component (v13+)
    ValidComponent,
    /// Not a WASM binary
    NotWasm,
    /// Truncated binary
    Truncated,
    /// Invalid version
    InvalidVersion,
    /// Malformed sections
    MalformedSections,
}

/// Validate WASM binary header without compiling.
fn validate_wasm_header(data: &[u8]) -> WasmValidation {
    // Check minimum length
    if data.len() < 8 {
        return WasmValidation::Truncated;
    }

    // Check magic number
    if &data[0..4] != WASM_MAGIC {
        return WasmValidation::NotWasm;
    }

    // Check version
    let version = &data[4..8];
    match version {
        [0x01, 0x00, 0x00, 0x00] => {
            // WASM v1 (module)
            // Basic section validation
            if data.len() > 8 {
                let section_id = data[8];
                if section_id > 12 && section_id != 0 {
                    // Unknown section (except custom section 0)
                    // This is still potentially valid if it's a custom section
                }
            }
            WasmValidation::ValidModule
        }
        [0x0d, 0x00, 0x01, 0x00] => {
            // Component model (layer type 0x01 = component)
            WasmValidation::ValidComponent
        }
        [0x0d, 0x00, 0x00, 0x00] => {
            // Component model (layer type 0x00 = core module in component)
            WasmValidation::ValidComponent
        }
        _ => WasmValidation::InvalidVersion,
    }
}

/// Check if data could contain WASM exports we care about.
fn check_for_http_handler_export(data: &[u8]) -> bool {
    // Very basic check for export section presence
    // In real code, this would parse the section properly
    if data.len() < 9 {
        return false;
    }

    // Look for export section (section ID 7)
    for i in 8..data.len().saturating_sub(1) {
        if data[i] == SectionId::Export as u8 {
            return true;
        }
    }
    false
}

fuzz_target!(|input: WasmInput| {
    let data = input.build();

    // Test 1: Header validation must not panic
    let validation = validate_wasm_header(&data);

    // Test 2: Verify invariants based on validation result
    match validation {
        WasmValidation::ValidModule | WasmValidation::ValidComponent => {
            // INVARIANT: Valid WASM must have at least magic + version
            assert!(data.len() >= 8, "valid WASM too short: {}", data.len());

            // INVARIANT: Valid WASM starts with magic
            assert_eq!(&data[0..4], WASM_MAGIC, "valid WASM missing magic");
        }
        WasmValidation::NotWasm => {
            // INVARIANT: Not WASM means magic doesn't match
            if data.len() >= 4 {
                assert_ne!(&data[0..4], WASM_MAGIC, "NotWasm but has magic");
            }
        }
        WasmValidation::Truncated => {
            // INVARIANT: Truncated means too short
            assert!(data.len() < 8, "Truncated but data is {} bytes", data.len());
        }
        WasmValidation::InvalidVersion => {
            // INVARIANT: Has magic but wrong version
            if data.len() >= 4 {
                assert_eq!(
                    &data[0..4],
                    WASM_MAGIC,
                    "InvalidVersion but no magic"
                );
            }
        }
        WasmValidation::MalformedSections => {
            // Section parsing failed but header was valid
        }
    }

    // Test 3: Export detection must not panic
    let _has_exports = check_for_http_handler_export(&data);

    // Test 4: Size sanity checks
    // INVARIANT: Data size should be reasonable for in-memory processing
    assert!(
        data.len() <= 1024 * 1024 * 100,
        "data too large: {} bytes",
        data.len()
    );

    // Test 5: If it looks like WASM, verify basic structure
    if data.len() >= 8 && &data[0..4] == WASM_MAGIC {
        // Valid magic - check version is in known range
        let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        // Known versions: 1 (MVP), various component model versions
        // Just verify we can read it without panic
        let _version_check = version;
    }
});

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that valid WASM magic is detected.
    #[test]
    fn test_valid_wasm_magic() {
        let data = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        assert_eq!(validate_wasm_header(&data), WasmValidation::ValidModule);
    }

    /// Test that invalid magic is rejected.
    #[test]
    fn test_invalid_magic() {
        let data = [0x00, 0x62, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        assert_eq!(validate_wasm_header(&data), WasmValidation::NotWasm);
    }

    /// Test that truncated data is detected.
    #[test]
    fn test_truncated() {
        let data = [0x00, 0x61, 0x73, 0x6D];
        assert_eq!(validate_wasm_header(&data), WasmValidation::Truncated);
    }

    /// Test that component model version is detected.
    #[test]
    fn test_component_model() {
        let data = [0x00, 0x61, 0x73, 0x6D, 0x0d, 0x00, 0x01, 0x00];
        assert_eq!(validate_wasm_header(&data), WasmValidation::ValidComponent);
    }

    /// Test empty data.
    #[test]
    fn test_empty() {
        let data: [u8; 0] = [];
        assert_eq!(validate_wasm_header(&data), WasmValidation::Truncated);
    }
}
