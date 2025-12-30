//! Manifest parsing for cron schedules.
//!
//! Parses the `[[schedules]]` section from mik.toml configuration files.

use anyhow::{Context, Result};
use serde::Deserialize;

use super::types::ScheduleConfig;

/// Partial manifest for parsing [[schedules]] from mik.toml.
#[derive(Debug, Default, Deserialize)]
struct SchedulesManifest {
    #[serde(default)]
    schedules: Vec<ScheduleConfig>,
}

/// Parse [[schedules]] from a mik.toml file.
///
/// Returns an empty Vec if the file doesn't exist or has no schedules.
///
/// # Example
///
/// ```toml
/// [[schedules]]
/// name = "cleanup"
/// module = "modules/cleanup.wasm"
/// cron = "0 0 0 * * * *"  # Daily at midnight
/// ```
pub fn parse_schedules_from_manifest(path: &std::path::Path) -> Result<Vec<ScheduleConfig>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let manifest: SchedulesManifest =
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;

    Ok(manifest.schedules)
}
