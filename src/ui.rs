//! UI utilities for consistent terminal output formatting.
//!
//! Provides shared formatting functions for error messages, spinners, and status output.

use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Width of error box separators.
const ERROR_BOX_WIDTH: usize = 60;

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
