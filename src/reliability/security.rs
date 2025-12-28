//! Shared security utilities.
//!
//! This module contains security-related functions used by both the host runtime
//! and the CLI tools. Centralizing these ensures consistent behavior across
//! the codebase.

/// Check if a host is allowed for outgoing HTTP requests.
///
/// Supports three pattern types:
/// - `"*"` - Allow all hosts
/// - `"*.example.com"` - Allow any subdomain of example.com (and example.com itself)
/// - `"api.example.com"` - Exact match only
///
/// Matching is case-insensitive per RFC 1035 (DNS names are case-insensitive).
///
/// # Arguments
///
/// * `host` - The hostname to check (e.g., "api.example.com")
/// * `allowed_patterns` - List of allowed host patterns
///
/// # Returns
///
/// `true` if the host matches any pattern, `false` otherwise.
/// Returns `false` if `allowed_patterns` is empty.
///
/// # Examples
///
/// ```
/// use mik::reliability::security::is_http_host_allowed;
///
/// // Wildcard allows all
/// assert!(is_http_host_allowed("anything.com", &["*".to_string()]));
///
/// // Exact match (case-insensitive)
/// assert!(is_http_host_allowed("api.example.com", &["api.example.com".to_string()]));
/// assert!(is_http_host_allowed("API.EXAMPLE.COM", &["api.example.com".to_string()]));
/// assert!(!is_http_host_allowed("other.example.com", &["api.example.com".to_string()]));
///
/// // Subdomain wildcard
/// let patterns = vec!["*.example.com".to_string()];
/// assert!(is_http_host_allowed("api.example.com", &patterns));
/// assert!(is_http_host_allowed("www.example.com", &patterns));
/// assert!(is_http_host_allowed("example.com", &patterns)); // bare domain matches
/// assert!(!is_http_host_allowed("example.org", &patterns));
///
/// // Empty list = nothing allowed
/// assert!(!is_http_host_allowed("example.com", &[]));
/// ```
pub fn is_http_host_allowed(host: &str, allowed_patterns: &[String]) -> bool {
    if allowed_patterns.is_empty() {
        return false;
    }

    // Normalize host to lowercase (DNS names are case-insensitive per RFC 1035)
    let host_lower = host.to_ascii_lowercase();

    for pattern in allowed_patterns {
        if pattern == "*" {
            return true;
        }

        // Normalize pattern to lowercase
        let pattern_lower = pattern.to_ascii_lowercase();

        if let Some(suffix) = pattern_lower.strip_prefix("*.") {
            // Wildcard pattern: *.example.com matches api.example.com
            let dot_suffix = format!(".{suffix}");
            if host_lower.ends_with(&dot_suffix) || host_lower == suffix {
                return true;
            }
        } else if pattern_lower == host_lower {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_all_hosts() {
        let allowed = vec!["*".to_string()];
        assert!(is_http_host_allowed("example.com", &allowed));
        assert!(is_http_host_allowed("api.example.com", &allowed));
        assert!(is_http_host_allowed("anything.com", &allowed));
    }

    #[test]
    fn test_exact_match() {
        let allowed = vec!["api.example.com".to_string()];
        assert!(is_http_host_allowed("api.example.com", &allowed));
        assert!(!is_http_host_allowed("other.example.com", &allowed));
        assert!(!is_http_host_allowed("example.com", &allowed));
    }

    #[test]
    fn test_wildcard_subdomain() {
        let allowed = vec!["*.example.com".to_string()];
        // Should match subdomains
        assert!(is_http_host_allowed("api.example.com", &allowed));
        assert!(is_http_host_allowed("www.example.com", &allowed));
        assert!(is_http_host_allowed("foo.bar.example.com", &allowed));
        // Should match bare domain (example.com == pattern[2..])
        assert!(is_http_host_allowed("example.com", &allowed));
        // Should NOT match different domains
        assert!(!is_http_host_allowed("example.org", &allowed));
        assert!(!is_http_host_allowed("notexample.com", &allowed));
    }

    #[test]
    fn test_multiple_patterns() {
        let allowed = vec![
            "api.example.com".to_string(),
            "*.supabase.co".to_string(),
            "github.com".to_string(),
        ];
        assert!(is_http_host_allowed("api.example.com", &allowed));
        assert!(is_http_host_allowed("my-project.supabase.co", &allowed));
        assert!(is_http_host_allowed("github.com", &allowed));
        assert!(!is_http_host_allowed("gitlab.com", &allowed));
    }

    #[test]
    fn test_empty_list() {
        let allowed: Vec<String> = vec![];
        assert!(!is_http_host_allowed("example.com", &allowed));
        assert!(!is_http_host_allowed("api.example.com", &allowed));
    }

    #[test]
    fn test_case_insensitive_matching() {
        // Exact match - case insensitive
        let allowed = vec!["api.example.com".to_string()];
        assert!(is_http_host_allowed("api.example.com", &allowed));
        assert!(is_http_host_allowed("API.EXAMPLE.COM", &allowed));
        assert!(is_http_host_allowed("Api.Example.Com", &allowed));

        // Wildcard - case insensitive
        let allowed = vec!["*.EXAMPLE.COM".to_string()];
        assert!(is_http_host_allowed("api.example.com", &allowed));
        assert!(is_http_host_allowed("API.EXAMPLE.COM", &allowed));
        assert!(is_http_host_allowed("example.com", &allowed));
        assert!(is_http_host_allowed("EXAMPLE.COM", &allowed));
    }
}
