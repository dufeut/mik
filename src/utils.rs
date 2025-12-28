//! Shared utility functions.

use std::fs;

/// Get project name from Cargo.toml in the current directory.
///
/// Returns `None` if Cargo.toml doesn't exist or can't be parsed.
pub fn get_cargo_name() -> Option<String> {
    let content = fs::read_to_string("Cargo.toml").ok()?;
    let table: toml::Table = content.parse().ok()?;
    table
        .get("package")?
        .get("name")?
        .as_str()
        .map(std::string::ToString::to_string)
}

/// Get project version from Cargo.toml in the current directory.
///
/// Returns `None` if Cargo.toml doesn't exist or can't be parsed.
#[allow(dead_code)]
pub fn get_cargo_version() -> Option<String> {
    let content = fs::read_to_string("Cargo.toml").ok()?;
    let table: toml::Table = content.parse().ok()?;
    table
        .get("package")?
        .get("version")?
        .as_str()
        .map(std::string::ToString::to_string)
}

/// Format bytes in human-readable form.
///
/// # Examples
///
/// ```
/// use mik::utils::format_bytes;
///
/// assert_eq!(format_bytes(0), "0 bytes");
/// assert_eq!(format_bytes(1024), "1.0 KB");
/// assert_eq!(format_bytes(1536), "1.5 KB");
/// assert_eq!(format_bytes(1048576), "1.0 MB");
/// ```
#[allow(clippy::cast_precision_loss)]
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;
    const TB: u64 = 1024 * 1024 * 1024 * 1024;

    if bytes == 0 {
        "0 bytes".to_string()
    } else if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

/// Format a duration in human-readable form.
///
/// # Examples
///
/// ```
/// use chrono::Duration;
/// use mik::utils::format_duration;
///
/// assert_eq!(format_duration(Duration::seconds(30)), "30s");
/// assert_eq!(format_duration(Duration::seconds(90)), "1m 30s");
/// assert_eq!(format_duration(Duration::seconds(3660)), "1h 1m");
/// assert_eq!(format_duration(Duration::seconds(90000)), "1d 1h");
/// ```
pub fn format_duration(duration: chrono::Duration) -> String {
    let secs = duration.num_seconds();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(chrono::Duration::seconds(30)), "30s");
        assert_eq!(format_duration(chrono::Duration::seconds(90)), "1m 30s");
        assert_eq!(format_duration(chrono::Duration::seconds(3660)), "1h 1m");
        assert_eq!(format_duration(chrono::Duration::seconds(90000)), "1d 1h");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 bytes");
        assert_eq!(format_bytes(512), "512 bytes");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TB");
    }
}
