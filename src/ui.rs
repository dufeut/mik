//! UI utilities for consistent terminal output formatting.
//!
//! Provides shared formatting functions for error messages, spinners, and status output.
//!
//! # Available Helpers
//!
//! - [`create_spinner`] - Create a progress spinner
//! - [`print_section`] - Print a section header
//! - [`print_error_box`] - Print error with stderr/stdout
//! - [`print_error_box_from_output`] - Print error from command output
//! - [`print_error_box_with_hints`] - Print error with troubleshooting hints
//! - [`print_error_section`] - Print error with title and message
//! - [`print_error_details`] - Print error with bullet-pointed details

use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Width of error box separators.
pub const ERROR_BOX_WIDTH: usize = 60;

/// Width of summary box separators (used for build output).
pub const SUMMARY_BOX_WIDTH: usize = 50;

/// Default spinner tick interval in milliseconds.
const SPINNER_TICK_MS: u64 = 100;

// =============================================================================
// Spinner Utilities
// =============================================================================

/// Create a spinner with the given message.
///
/// Returns a cyan-colored spinner that ticks every 100ms.
///
/// # Example
///
/// ```ignore
/// let spinner = create_spinner("Building component...");
/// // ... do work ...
/// spinner.finish_and_clear();
/// ```
pub fn create_spinner(msg: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    spinner.set_message(msg.to_string());
    spinner.enable_steady_tick(Duration::from_millis(SPINNER_TICK_MS));
    spinner
}

// =============================================================================
// Error Box Utilities
// =============================================================================

/// Print an error box with a title and optional stderr/stdout content.
///
/// Formats errors consistently across the codebase with:
/// - A separator line of `=` characters
/// - The error title
/// - Another separator line
/// - Optional stderr content
/// - Optional stdout content
///
/// # Example
///
/// ```ignore
/// print_error_box("Build Failed", Some(&stderr), Some(&stdout));
/// ```
///
/// Outputs:
/// ```text
/// ============================================================
/// Build Failed
/// ============================================================
///
/// <stderr content>
/// <stdout content>
/// ```
pub fn print_error_box(title: &str, stderr: Option<&str>, stdout: Option<&str>) {
    eprintln!("\n{}", "=".repeat(ERROR_BOX_WIDTH));
    eprintln!("{title}");
    eprintln!("{}", "=".repeat(ERROR_BOX_WIDTH));

    if let Some(err) = stderr
        && !err.is_empty()
    {
        eprintln!("\n{err}");
    }

    if let Some(out) = stdout
        && !out.is_empty()
    {
        eprintln!("{out}");
    }
}

/// Print an error box from command output.
///
/// Convenience function that extracts stderr/stdout from `std::process::Output`.
pub fn print_error_box_from_output(title: &str, output: &std::process::Output) {
    let stderr = String::from_utf8(output.stderr.clone()).ok();
    let stdout = String::from_utf8(output.stdout.clone()).ok();

    print_error_box(title, stderr.as_deref(), stdout.as_deref());
}

/// Print an error box with troubleshooting hints.
///
/// Used for composition and other complex operations that need additional help.
pub fn print_error_box_with_hints(title: &str, output: &std::process::Output, hints: &[&str]) {
    print_error_box_from_output(title, output);

    if !hints.is_empty() {
        eprintln!("\n{}", "=".repeat(ERROR_BOX_WIDTH));
        eprintln!("Common Issues:");
        eprintln!("{}", "=".repeat(ERROR_BOX_WIDTH));

        for (i, hint) in hints.iter().enumerate() {
            eprintln!("\n{}. {hint}", i + 1);
        }
        eprintln!();
    }
}

// =============================================================================
// Section Headers
// =============================================================================

/// Print a section header with separator lines.
///
/// # Example
///
/// ```ignore
/// print_section("Authentication Error");
/// ```
///
/// Outputs:
/// ```text
///
/// ============================================================
/// Authentication Error
/// ============================================================
/// ```
pub fn print_section(title: &str) {
    eprintln!("\n{}", "=".repeat(ERROR_BOX_WIDTH));
    eprintln!("{title}");
    eprintln!("{}", "=".repeat(ERROR_BOX_WIDTH));
}

/// Print a section with a message.
///
/// # Example
///
/// ```ignore
/// print_error_section("Build Failed", "Missing dependency foo");
/// ```
pub fn print_error_section(title: &str, message: &str) {
    print_section(title);
    eprintln!("\n{message}");
}

/// Print an error with bullet-pointed details.
///
/// # Example
///
/// ```ignore
/// print_error_details("Network Error", "Failed to connect", &[
///     "Check your internet connection",
///     "Verify the server is running",
/// ]);
/// ```
pub fn print_error_details(title: &str, message: &str, details: &[&str]) {
    print_section(title);
    eprintln!("\n{message}");

    if !details.is_empty() {
        eprintln!();
        for detail in details {
            eprintln!("  - {detail}");
        }
    }
}

/// Print numbered steps (for "To fix this:" sections).
///
/// # Example
///
/// ```ignore
/// print_numbered_steps(&[
///     "Run: gh auth login",
///     "Follow the prompts",
///     "Verify: gh auth status",
/// ]);
/// ```
pub fn print_numbered_steps(steps: &[&str]) {
    eprintln!("\nTo fix this:");
    for (i, step) in steps.iter().enumerate() {
        eprintln!("  {}. {step}", i + 1);
    }
}

// =============================================================================
// Summary Box Utilities (Build Output)
// =============================================================================

/// Print a summary box header with a title.
///
/// Used for build output summaries.
///
/// # Example
///
/// ```ignore
/// print_summary_header("Build Summary");
/// println!("Output:     dist/component.wasm");
/// println!("Size:       128.5 KB");
/// print_summary_footer();
/// ```
pub fn print_summary_header(title: &str) {
    println!();
    println!("{}", "=".repeat(SUMMARY_BOX_WIDTH));
    println!("{title}");
    println!("{}", "=".repeat(SUMMARY_BOX_WIDTH));
    println!();
}

/// Print a summary box footer.
///
/// Use after `print_summary_header` and content lines.
pub fn print_summary_footer() {
    println!();
    println!("{}", "=".repeat(SUMMARY_BOX_WIDTH));
}

// =============================================================================
// Tool Checking Display
// =============================================================================

/// Print a tool check result with checkmark/cross.
///
/// Used by `--version-verbose` to show tool availability.
pub fn print_tool_check(name: &str, install_cmd: &str) {
    use std::process::Command;

    let result = Command::new(name).arg("--version").output();

    match result {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let version_line = version.lines().next().unwrap_or("unknown");
            println!("  ✓ {version_line}");
        },
        _ => {
            println!("  ✗ {name} not found (install: {install_cmd})");
        },
    }
}
