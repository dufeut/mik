//! Asset discovery for publish command.
//!
//! Finds WASM components, WIT directories, and static assets.

use anyhow::Result;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use super::types::{STATIC_DIRS, WASM_PATTERNS, WIT_DIRS};

/// Find the built WASM component.
pub fn find_component() -> Result<PathBuf> {
    for pattern in WASM_PATTERNS {
        if let Some(path) = glob::glob(pattern)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .find(|p| !p.to_string_lossy().contains("deps"))
        {
            // Validate file exists by attempting to open it (TOCTOU prevention)
            if File::open(&path).is_ok() {
                return Ok(path);
            }
            // If open fails, continue to next candidate
        }
    }

    // Fallback: any .wasm in target/
    if let Ok(entries) = fs::read_dir("target") {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "wasm") {
                let path = entry.path();
                // Validate file exists by attempting to open it (TOCTOU prevention)
                if File::open(&path).is_ok() {
                    return Ok(path);
                }
            }
        }
    }

    anyhow::bail!("No component found. Run 'mik build --release' first.")
}

/// Find first existing directory from candidates.
pub fn find_dir(candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(Path::new)
        .find(|p| p.is_dir())
        .map(std::path::Path::to_path_buf)
}

/// Find the WIT directory if it exists.
pub fn find_wit_dir() -> Option<PathBuf> {
    find_dir(&WIT_DIRS)
}

/// Find the static assets directory if it exists.
pub fn find_static_dir() -> Option<PathBuf> {
    find_dir(&STATIC_DIRS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_find_component_success() {
        use std::fs;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("target/wasm32-wasip2/release");
        fs::create_dir_all(&target_dir).unwrap();

        let wasm_path = target_dir.join("test.wasm");
        fs::write(&wasm_path, b"\0asm\x01\x00\x00\x00").unwrap();

        // Change to temp directory - use guard to ensure restoration
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let result = find_component();

        // Restore original directory (ignore errors since cwd might have changed)
        let _ = std::env::set_current_dir(&original_dir);

        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_find_component_not_found() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let result = find_component();

        // Restore original directory (ignore errors since cwd might have changed)
        let _ = std::env::set_current_dir(&original_dir);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No component found"));
        assert!(err.contains("mik build"));
    }
}
