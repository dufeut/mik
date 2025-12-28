//! Fuzz target for JSON parsing in the script handler.
//!
//! This fuzzer tests that:
//! 1. serde_json parsing doesn't panic on arbitrary input
//! 2. Valid JSON round-trips correctly
//! 3. Large/deeply nested JSON is handled safely
//!
//! Run with: `cargo +nightly fuzz run fuzz_json_parsing`

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

/// Maximum nesting depth we allow in tests (to prevent stack overflow).
const MAX_DEPTH: usize = 100;

/// Structured input for JSON fuzzing.
#[derive(Arbitrary, Debug)]
struct JsonInput {
    /// The raw bytes to parse as JSON
    data: Vec<u8>,
    /// Whether to test with valid JSON structure
    use_structured: bool,
    /// Structured JSON to generate
    structured: StructuredJson,
}

/// Structured JSON for valid input generation.
#[derive(Arbitrary, Debug)]
enum StructuredJson {
    Null,
    Bool(bool),
    Number(i64),
    Float(f64),
    String(String),
    Array(Vec<StructuredJsonItem>),
    Object(Vec<(String, StructuredJsonItem)>),
}

/// Wrapper to limit recursion depth.
#[derive(Arbitrary, Debug)]
struct StructuredJsonItem {
    depth: u8,
    value: Box<StructuredJsonSimple>,
}

/// Simple JSON values (no recursion).
#[derive(Arbitrary, Debug)]
enum StructuredJsonSimple {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
}

impl StructuredJson {
    fn to_value(&self, depth: usize) -> Value {
        if depth > MAX_DEPTH {
            return Value::Null;
        }

        match self {
            Self::Null => Value::Null,
            Self::Bool(b) => Value::Bool(*b),
            Self::Number(n) => Value::Number((*n).into()),
            Self::Float(f) => {
                // Handle NaN and Infinity which aren't valid JSON
                if f.is_finite() {
                    serde_json::Number::from_f64(*f)
                        .map_or(Value::Null, Value::Number)
                } else {
                    Value::Null
                }
            }
            Self::String(s) => Value::String(s.clone()),
            Self::Array(items) => {
                Value::Array(
                    items
                        .iter()
                        .take(100) // Limit array size
                        .map(|item| item.to_value(depth + 1))
                        .collect(),
                )
            }
            Self::Object(pairs) => {
                let map: serde_json::Map<String, Value> = pairs
                    .iter()
                    .take(100) // Limit object size
                    .map(|(k, v)| (k.clone(), v.to_value(depth + 1)))
                    .collect();
                Value::Object(map)
            }
        }
    }
}

impl StructuredJsonItem {
    fn to_value(&self, depth: usize) -> Value {
        if depth > MAX_DEPTH || self.depth as usize > MAX_DEPTH {
            return Value::Null;
        }

        match self.value.as_ref() {
            StructuredJsonSimple::Null => Value::Null,
            StructuredJsonSimple::Bool(b) => Value::Bool(*b),
            StructuredJsonSimple::Number(n) => Value::Number((*n).into()),
            StructuredJsonSimple::String(s) => Value::String(s.clone()),
        }
    }
}

fuzz_target!(|input: JsonInput| {
    if input.use_structured {
        // Test with valid structured JSON
        let value = input.structured.to_value(0);

        // Serialize to string
        let serialized = match serde_json::to_string(&value) {
            Ok(s) => s,
            Err(_) => return, // Some values can't be serialized (shouldn't happen)
        };

        // Parse back
        let parsed: Result<Value, _> = serde_json::from_str(&serialized);

        // INVARIANT: Valid JSON must round-trip
        match parsed {
            Ok(parsed_value) => {
                // Values should be equal
                assert_eq!(
                    value, parsed_value,
                    "JSON round-trip failed: {:?} != {:?}",
                    value, parsed_value
                );
            }
            Err(e) => {
                panic!(
                    "Failed to parse serialized JSON: {:?}\nInput: {:?}",
                    e, serialized
                );
            }
        }
    } else {
        // Test with arbitrary bytes (may be invalid JSON)
        // This should never panic, even on malformed input
        let result: Result<Value, _> = serde_json::from_slice(&input.data);

        if let Ok(value) = result {
            // INVARIANT 1: If parsing succeeded, the value should be serializable
            let serialized = serde_json::to_string(&value);
            assert!(
                serialized.is_ok(),
                "Parsed JSON but failed to serialize: {:?}",
                value
            );

            // INVARIANT 2: Serialized form should parse back to same value
            if let Ok(s) = serialized {
                let reparsed: Result<Value, _> = serde_json::from_str(&s);
                assert!(
                    reparsed.is_ok(),
                    "Failed to reparse serialized JSON: {:?}",
                    s
                );
                if let Ok(reparsed_value) = reparsed {
                    assert_eq!(
                        value, reparsed_value,
                        "JSON round-trip changed value"
                    );
                }
            }

            // INVARIANT 3: Depth should be reasonable
            let depth = measure_depth(&value, 0);
            assert!(
                depth <= 1000,
                "JSON depth too large: {} (possible stack overflow risk)",
                depth
            );
        }
        // Parsing failures are expected and safe
    }
});

/// Measure the nesting depth of a JSON value.
fn measure_depth(value: &Value, current: usize) -> usize {
    if current > 1000 {
        return current; // Prevent stack overflow in depth measurement itself
    }

    match value {
        Value::Array(arr) => {
            arr.iter()
                .map(|v| measure_depth(v, current + 1))
                .max()
                .unwrap_or(current)
        }
        Value::Object(obj) => {
            obj.values()
                .map(|v| measure_depth(v, current + 1))
                .max()
                .unwrap_or(current)
        }
        _ => current,
    }
}
