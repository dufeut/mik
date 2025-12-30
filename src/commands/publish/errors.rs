//! Error handling and diagnostics for publish command.
//!
//! Provides detailed error messages and troubleshooting guidance.

use anyhow::Result;

/// Handle publish errors with detailed diagnostics.
pub fn handle_publish_error(error: anyhow::Error, repo: &str, version: &str) -> Result<()> {
    let error_msg = error.to_string();

    print_error_header("Publish Failed", &error_msg);

    // Detect specific error patterns and provide targeted help
    if is_auth_error(&error_msg) {
        print_auth_error_help();
        anyhow::bail!("GitHub authentication required");
    }

    if is_version_conflict_error(&error_msg) {
        print_version_conflict_help(repo, version);
        anyhow::bail!("Release version already exists");
    }

    if is_network_error(&error_msg) {
        print_network_error_help();
        anyhow::bail!("Network connection failed");
    }

    if is_permission_error(&error_msg) {
        print_permission_error_help(repo);
        anyhow::bail!("Insufficient permissions");
    }

    if is_repo_not_found_error(&error_msg) {
        print_repo_not_found_help(repo);
        anyhow::bail!("Repository not found");
    }

    // Generic error - show troubleshooting steps
    print_generic_troubleshooting_help(repo);

    Err(error)
}

/// Handle asset upload errors with detailed diagnostics.
pub fn handle_upload_error(error: anyhow::Error, repo: &str, version: &str) -> Result<()> {
    let error_msg = error.to_string();

    eprintln!("\n{}", "=".repeat(60));
    eprintln!("Asset Upload Failed");
    eprintln!("{}", "=".repeat(60));
    eprintln!("\nError: {error_msg}");

    // Release was created but asset upload failed
    eprintln!("\nNote: The release '{version}' was created, but asset upload failed.");

    if is_auth_error(&error_msg) {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("Authentication Error");
        eprintln!("{}", "=".repeat(60));
        eprintln!("\nAuthentication failed during asset upload.");
        eprintln!("\nTo fix and retry:");
        eprintln!("  1. Authenticate: gh auth login");
        eprintln!("  2. Upload manually: gh release upload {version} <file> --repo {repo}");
        anyhow::bail!("GitHub authentication required for upload");
    }

    if is_network_error(&error_msg) {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("Network Error");
        eprintln!("{}", "=".repeat(60));
        eprintln!("\nNetwork failure during asset upload.");
        eprintln!("\nTo retry upload:");
        eprintln!("  gh release upload {version} target/*.wasm --repo {repo} --clobber");
        anyhow::bail!("Network connection failed during upload");
    }

    // Generic upload error
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("Manual Upload");
    eprintln!("{}", "=".repeat(60));
    eprintln!("\nYou can manually upload assets to the release:");
    eprintln!("  gh release upload {version} target/*.wasm --repo {repo} --clobber");
    eprintln!("\nOr via web interface:");
    eprintln!("  https://github.com/{repo}/releases/edit/{version}");

    Err(error)
}

/// Print a formatted error header with separator lines.
fn print_error_header(title: &str, error_msg: &str) {
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("{title}");
    eprintln!("{}", "=".repeat(60));
    eprintln!("\nError: {error_msg}");
}

/// Print a formatted section header.
fn print_section_header(title: &str) {
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("{title}");
    eprintln!("{}", "=".repeat(60));
}

/// Print help for authentication errors.
fn print_auth_error_help() {
    print_section_header("Authentication Error");
    eprintln!("\nYou are not authenticated with GitHub.");
    eprintln!("\nTo fix this:");
    eprintln!("  1. Run: gh auth login");
    eprintln!("  2. Follow the prompts to authenticate with GitHub");
    eprintln!("  3. Verify authentication: gh auth status");
    eprintln!("\nAlternatively, set GITHUB_TOKEN environment variable:");
    eprintln!("  export GITHUB_TOKEN=your_personal_access_token");
    eprintln!("\nFor more help:");
    eprintln!("  https://cli.github.com/manual/gh_auth_login");
}

/// Print help for version conflict errors.
fn print_version_conflict_help(repo: &str, version: &str) {
    print_section_header("Version Conflict");
    eprintln!("\nRelease version '{version}' already exists.");
    eprintln!("\nTo fix this:");
    eprintln!("  1. Use a different version tag:");
    eprintln!(
        "     mik publish --tag v{}.1",
        version.trim_start_matches('v')
    );
    eprintln!("  2. Or delete the existing release:");
    eprintln!("     gh release delete {version} --repo {repo} --yes");
    eprintln!("\nCheck existing releases:");
    eprintln!("  gh release list --repo {repo}");
    eprintln!("  https://github.com/{repo}/releases");
}

/// Print help for network errors.
fn print_network_error_help() {
    print_section_header("Network Error");
    eprintln!("\nFailed to connect to GitHub.");
    eprintln!("\nPossible causes:");
    eprintln!("  - No internet connection");
    eprintln!("  - GitHub API is down or rate-limited");
    eprintln!("  - Firewall or proxy blocking connection");
    eprintln!("  - DNS resolution failure");
    eprintln!("\nTo fix this:");
    eprintln!("  1. Check your internet connection");
    eprintln!("  2. Verify GitHub status: https://www.githubstatus.com/");
    eprintln!("  3. Check API rate limits: gh api rate_limit");
    eprintln!("  4. Try again in a few minutes");
    eprintln!("\nIf behind a proxy, configure:");
    eprintln!("  export HTTPS_PROXY=http://proxy:port");
}

/// Print help for permission errors.
fn print_permission_error_help(repo: &str) {
    print_section_header("Permission Error");
    eprintln!("\nYou don't have permission to create releases in this repository.");
    eprintln!("\nRepository: {repo}");
    eprintln!("\nTo fix this:");
    eprintln!("  1. Ensure you have write access to the repository");
    eprintln!("  2. Check repository permissions: gh repo view {repo}");
    eprintln!("  3. Verify you're authenticated with the correct account: gh auth status");
    eprintln!("  4. If using a token, ensure it has 'repo' scope");
    eprintln!("\nFor organization repos, you may need:");
    eprintln!("  - Maintainer or Admin role");
    eprintln!("  - Repository write permissions");
}

/// Print help for repository not found errors.
fn print_repo_not_found_help(repo: &str) {
    print_section_header("Repository Not Found");
    eprintln!("\nRepository '{repo}' not found or not accessible.");
    eprintln!("\nPossible causes:");
    eprintln!("  - Repository doesn't exist");
    eprintln!("  - Repository is private and you don't have access");
    eprintln!("  - Typo in repository name");
    eprintln!("\nTo fix this:");
    eprintln!("  1. Verify repository exists: https://github.com/{repo}");
    eprintln!("  2. Check git remote: git remote -v");
    eprintln!(
        "  3. Update origin if needed: git remote set-url origin https://github.com/{repo}.git"
    );
}

/// Print generic troubleshooting help.
fn print_generic_troubleshooting_help(repo: &str) {
    print_section_header("Troubleshooting");
    eprintln!("\n1. Verify gh CLI is installed and authenticated:");
    eprintln!("   gh auth status");
    eprintln!("\n2. Check repository access:");
    eprintln!("   gh repo view {repo}");
    eprintln!("\n3. View existing releases:");
    eprintln!("   gh release list --repo {repo}");
    eprintln!("\n4. Try with --dry-run to test without publishing:");
    eprintln!("   mik publish --dry-run");
}

/// Detect authentication errors.
pub fn is_auth_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains("not authenticated")
        || lower.contains("authentication")
            && (lower.contains("failed") || lower.contains("required"))
        || lower.contains("gh auth")
        || lower.contains("unauthorized")
        || lower.contains("401")
        || lower.contains("bad credentials")
        || lower.contains("invalid token")
        || lower.contains("token") && (lower.contains("invalid") || lower.contains("expired"))
}

/// Detect version conflict errors (release already exists).
pub fn is_version_conflict_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    (lower.contains("already exists") || lower.contains("already_exists"))
        && (lower.contains("release") || lower.contains("tag"))
        || lower.contains("duplicate") && lower.contains("release")
        || lower.contains("422") && lower.contains("release")
        || lower.contains("validation failed") && lower.contains("already_exists")
}

/// Detect network errors.
pub fn is_network_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains("network")
        || lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("connection")
            && (lower.contains("refused")
                || lower.contains("failed")
                || lower.contains("reset")
                || lower.contains("closed"))
        || lower.contains("could not resolve")
        || lower.contains("name resolution")
        || lower.contains("dns") && (lower.contains("fail") || lower.contains("error"))
        || lower.contains("unreachable")
        || lower.contains("no route to host")
        || lower.contains("temporary failure")
        || lower.contains("connect: ") && lower.contains("error")
}

/// Detect permission/authorization errors.
pub fn is_permission_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    (lower.contains("permission") || lower.contains("forbidden"))
        && (lower.contains("denied") || lower.contains("error"))
        || lower.contains("403")
        || lower.contains("not permitted")
        || lower.contains("access denied")
        || lower.contains("insufficient") && lower.contains("permission")
}

/// Detect repository not found errors.
pub fn is_repo_not_found_error(error_msg: &str) -> bool {
    let lower = error_msg.to_lowercase();
    lower.contains("not found") && (lower.contains("repository") || lower.contains("repo"))
        || lower.contains("404")
            && (lower.contains("repository")
                || lower.contains("repo")
                || lower.contains("not found"))
        || lower.contains("could not resolve to a repository")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_detection() {
        assert!(is_auth_error(
            "Not authenticated with GitHub. Run: gh auth login"
        ));
        assert!(is_auth_error("authentication failed"));
        assert!(is_auth_error("Error: HTTP 401: Unauthorized"));
        assert!(is_auth_error("Bad credentials"));
        assert!(is_auth_error("Invalid token"));
        assert!(is_auth_error("Token expired"));
        assert!(is_auth_error("Authentication required"));
        assert!(!is_auth_error("Some other error"));
        assert!(!is_auth_error("Network timeout"));
    }

    #[test]
    fn test_version_conflict_detection() {
        assert!(is_version_conflict_error("Release already exists"));
        assert!(is_version_conflict_error("Tag v1.0.0 already exists"));
        assert!(is_version_conflict_error(
            "HTTP 422: Validation Failed (already_exists)"
        ));
        assert!(is_version_conflict_error("Duplicate release"));
        assert!(is_version_conflict_error(
            "validation failed: already_exists"
        ));
        assert!(!is_version_conflict_error("Some other error"));
        assert!(!is_version_conflict_error("Network timeout"));
    }

    #[test]
    fn test_network_error_detection() {
        assert!(is_network_error("Network timeout"));
        assert!(is_network_error("Connection timed out"));
        assert!(is_network_error("Connection refused"));
        assert!(is_network_error("Connection failed"));
        assert!(is_network_error("Connection reset by peer"));
        assert!(is_network_error("Could not resolve host"));
        assert!(is_network_error("DNS failure"));
        assert!(is_network_error("Host unreachable"));
        assert!(is_network_error("No route to host"));
        assert!(is_network_error("Temporary failure in name resolution"));
        assert!(!is_network_error("Some other error"));
        assert!(!is_network_error("Authentication failed"));
    }

    #[test]
    fn test_permission_error_detection() {
        assert!(is_permission_error("Permission denied"));
        assert!(is_permission_error("HTTP 403: Forbidden"));
        assert!(is_permission_error("Access denied"));
        assert!(is_permission_error("Not permitted to perform this action"));
        assert!(is_permission_error("Insufficient permissions"));
        assert!(is_permission_error("Forbidden error"));
        assert!(!is_permission_error("Some other error"));
        assert!(!is_permission_error("Not found"));
    }

    #[test]
    fn test_repo_not_found_detection() {
        assert!(is_repo_not_found_error("Repository not found"));
        assert!(is_repo_not_found_error("HTTP 404: Not Found"));
        assert!(is_repo_not_found_error("Could not resolve to a Repository"));
        assert!(is_repo_not_found_error("Repo not found"));
        assert!(is_repo_not_found_error("404: repository not found"));
        assert!(!is_repo_not_found_error("Some other error"));
        assert!(!is_repo_not_found_error("Permission denied"));
    }
}
