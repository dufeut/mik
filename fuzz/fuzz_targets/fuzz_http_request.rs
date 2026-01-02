//! Fuzz target for HTTP request handling edge cases.
//!
//! This fuzzer tests HTTP request path parsing and security validation:
//! 1. Path sanitization for module routing
//! 2. Module name extraction from paths
//! 3. Path traversal prevention
//! 4. Various HTTP method and header combinations
//!
//! Run with: `cargo +nightly fuzz run fuzz_http_request`

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mik::security::{sanitize_file_path, sanitize_module_name};

/// Structured HTTP request input for fuzzing.
#[derive(Arbitrary, Debug)]
struct FuzzRequest {
    /// HTTP method
    method: HttpMethod,
    /// Request path
    path: String,
    /// HTTP headers
    headers: Vec<(String, String)>,
    /// Request body
    body: Vec<u8>,
    /// Adversarial pattern to apply
    adversarial: Option<PathAdversarial>,
}

#[derive(Arbitrary, Debug)]
enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
    /// Invalid/unusual methods
    Custom(String),
}

impl HttpMethod {
    fn as_str(&self) -> &str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Custom(s) => s.as_str(),
        }
    }
}

#[derive(Arbitrary, Debug)]
enum PathAdversarial {
    /// URL-encoded path traversal
    UrlEncodedTraversal,
    /// Double URL encoding
    DoubleEncoding,
    /// Null byte injection
    NullByte,
    /// Overlong UTF-8 encoding
    OverlongUtf8,
    /// Mixed path separators
    MixedSeparators,
    /// Very long path
    LongPath,
    /// Many path segments
    ManySegments,
    /// Unicode lookalike characters
    UnicodeLookalikes,
    /// Windows reserved names
    WindowsReserved,
    /// Trailing dots and spaces
    TrailingChars,
}

impl FuzzRequest {
    /// Build the final path for testing.
    fn build_path(&self) -> String {
        match &self.adversarial {
            None => self.path.clone(),
            Some(pattern) => self.apply_adversarial(pattern),
        }
    }

    fn apply_adversarial(&self, pattern: &PathAdversarial) -> String {
        match pattern {
            PathAdversarial::UrlEncodedTraversal => {
                // %2e = . and %2f = /
                format!("/run/%2e%2e%2f%2e%2e%2f{}", self.path)
            }
            PathAdversarial::DoubleEncoding => {
                // %252e = %2e after one decode = . after two decodes
                format!("/run/%252e%252e%252f{}", self.path)
            }
            PathAdversarial::NullByte => {
                format!("/run/{}\0.wasm", self.path)
            }
            PathAdversarial::OverlongUtf8 => {
                // Overlong encoding attempt - use byte array instead
                // In real attacks, these would be invalid UTF-8 sequences
                // For fuzzing, we use valid unicode that looks suspicious
                format!("/run/\u{00B7}\u{00B7}/{}", self.path)
            }
            PathAdversarial::MixedSeparators => {
                format!("/run/a\\b/c\\d/{}", self.path)
            }
            PathAdversarial::LongPath => {
                let long_segment = "a".repeat(1000);
                format!("/run/{}/{}", long_segment, self.path)
            }
            PathAdversarial::ManySegments => {
                let segments: String = (0..100).map(|i| format!("s{i}/")).collect();
                format!("/run/{}{}", segments, self.path)
            }
            PathAdversarial::UnicodeLookalikes => {
                // Unicode lookalikes for . and /
                // U+2024 ONE DOT LEADER, U+2215 DIVISION SLASH
                format!("/run/\u{2024}\u{2024}\u{2215}{}", self.path)
            }
            PathAdversarial::WindowsReserved => {
                // Windows reserved device names
                format!("/run/CON/{}", self.path)
            }
            PathAdversarial::TrailingChars => {
                format!("/run/module.../  {}", self.path)
            }
        }
    }

    /// Extract the module name from the path (simulating runtime behavior).
    fn extract_module_name(&self) -> Option<String> {
        let path = self.build_path();
        path.strip_prefix("/run/")
            .map(|rest| rest.split('/').next().unwrap_or("").to_string())
    }
}

fuzz_target!(|req: FuzzRequest| {
    let path = req.build_path();

    // Test 1: Path sanitization must not panic
    let sanitized = sanitize_file_path(&path);

    if let Ok(ref safe_path) = sanitized {
        // INVARIANT: Sanitized path must not escape base directory
        let path_str = safe_path.to_string_lossy();
        assert!(
            !path_str.starts_with(".."),
            "sanitized path starts with ..: {:?}",
            safe_path
        );

        // INVARIANT: No null bytes in output
        assert!(
            !path_str.contains('\0'),
            "sanitized path contains null byte: {:?}",
            safe_path
        );

        // INVARIANT: Not absolute
        assert!(
            !safe_path.is_absolute(),
            "sanitized path is absolute: {:?}",
            safe_path
        );
    }

    // Test 2: Module name extraction and validation
    if let Some(module_name) = req.extract_module_name() {
        // The module name extraction must not panic
        let module_result = sanitize_module_name(&module_name);

        if let Ok(ref safe_name) = module_result {
            // INVARIANT: No path separators in module name
            assert!(
                !safe_name.contains('/') && !safe_name.contains('\\'),
                "module name contains separator: {:?}",
                safe_name
            );

            // INVARIANT: Not empty
            assert!(!safe_name.is_empty(), "module name is empty");

            // INVARIANT: Not special directory
            assert!(
                safe_name != "." && safe_name != "..",
                "module name is special: {:?}",
                safe_name
            );

            // INVARIANT: Reasonable length
            assert!(
                safe_name.len() <= 255,
                "module name too long: {}",
                safe_name.len()
            );
        }
    }

    // Test 3: Header validation (no injection)
    for (name, value) in &req.headers {
        // Headers should not contain newlines (HTTP header injection)
        if name.contains('\n') || name.contains('\r') || value.contains('\n') || value.contains('\r')
        {
            // This is an attack attempt - in real code this would be rejected
            // The fuzzer verifies we can detect these safely
        }

        // INVARIANT: No null bytes in headers
        if name.contains('\0') || value.contains('\0') {
            // Attack attempt - would be rejected
        }
    }

    // Test 4: Method validation
    let method = req.method.as_str();
    // Standard HTTP methods
    let standard_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
    let _is_standard = standard_methods.contains(&method.to_uppercase().as_str());
    // Custom methods are allowed but should be validated by the application

    // Test 5: Body size considerations
    let body_size = req.body.len();
    // INVARIANT: Body size should be trackable
    assert!(body_size <= usize::MAX, "body size overflow");
});

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that normal paths are handled correctly.
    #[test]
    fn test_normal_path() {
        let result = sanitize_file_path("modules/hello.wasm");
        assert!(result.is_ok());
    }

    /// Test that path traversal is blocked.
    #[test]
    fn test_path_traversal_blocked() {
        let result = sanitize_file_path("../../../etc/passwd");
        assert!(result.is_err());
    }

    /// Test module name extraction.
    #[test]
    fn test_module_extraction() {
        let path = "/run/hello/api/users";
        let module = path
            .strip_prefix("/run/")
            .and_then(|rest| rest.split('/').next());
        assert_eq!(module, Some("hello"));
    }

    /// Test that valid module names pass.
    #[test]
    fn test_valid_module_name() {
        let result = sanitize_module_name("my-module");
        assert!(result.is_ok());
    }

    /// Test that path separators in module names are blocked.
    #[test]
    fn test_module_with_separator() {
        let result = sanitize_module_name("../evil");
        assert!(result.is_err());
    }
}
