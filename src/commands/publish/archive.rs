//! Archive creation for publish command.
//!
//! Creates tar.gz archives containing the component and assets.

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::path::Path;

/// Create a tar.gz archive with component and optional assets.
pub fn create_archive(
    path: &Path,
    name: &str,
    component: &Path,
    wit_dir: Option<&Path>,
    static_dir: Option<&Path>,
    mik_toml: &Path,
) -> Result<()> {
    let file = File::create(path).context("Failed to create archive")?;
    let mut archive = tar::Builder::new(GzEncoder::new(file, Compression::default()));

    // Add component
    archive
        .append_path_with_name(component, format!("{name}.wasm"))
        .context("Failed to add component")?;

    // Add optional directories
    if let Some(wit) = wit_dir.filter(|p| p.is_dir()) {
        archive
            .append_dir_all("wit", wit)
            .context("Failed to add wit/")?;
    }
    if let Some(s) = static_dir.filter(|p| p.is_dir()) {
        archive
            .append_dir_all("static", s)
            .context("Failed to add static/")?;
    }
    if mik_toml.exists() {
        archive
            .append_path_with_name(mik_toml, "mik.toml")
            .context("Failed to add mik.toml")?;
    }

    archive.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_create_archive() {
        let dir = tempdir().unwrap();

        // Create a test component
        let component_path = dir.path().join("test.wasm");
        fs::write(&component_path, b"\0asm\x01\x00\x00\x00").unwrap();

        // Create archive
        let archive_path = dir.path().join("test.tar.gz");
        let result = create_archive(
            &archive_path,
            "test",
            &component_path,
            None,
            None,
            Path::new("nonexistent.toml"), // Won't be added since it doesn't exist
        );

        assert!(result.is_ok());
        assert!(archive_path.exists());
    }
}
