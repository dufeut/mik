//! Collect static files from all dependencies.
//!
//! Extracts static/ folders from dependency archives and organizes them
//! into a namespaced structure: `collected-static/<project-name>/...`
//!
//! This allows a host to serve all static files under `/static/<project>/`.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::{Dependency, Manifest};

/// Sanitize a project/dependency name for use in file paths.
///
/// Prevents directory traversal by rejecting names with path separators
/// or special directory names.
fn sanitize_project_name(name: &str) -> Result<String> {
    if name.is_empty() {
        anyhow::bail!("Project name cannot be empty");
    }

    if name.contains('/') || name.contains('\\') {
        anyhow::bail!("Project name '{name}' contains path separators");
    }

    if name == "." || name == ".." {
        anyhow::bail!("Project name cannot be '.' or '..'");
    }

    if name.contains('\0') {
        anyhow::bail!("Project name contains null bytes");
    }

    Ok(name.to_string())
}

/// Default output directory for collected static files.
const DEFAULT_OUTPUT_DIR: &str = "collected-static";

/// Static asset directory candidates (ordered by priority).
const STATIC_CANDIDATES: [&str; 6] = [
    "static",
    "dist",
    "build",
    "public",
    "frontend/dist",
    "web/dist",
];

/// Collect static files from all dependencies.
pub fn execute(output_dir: Option<&str>) -> Result<()> {
    let manifest = Manifest::load().context("No mik.toml found. Run 'mik init' first.")?;

    let output = output_dir.unwrap_or(DEFAULT_OUTPUT_DIR);
    let output_path = Path::new(output);

    // Create output directory
    fs::create_dir_all(output_path)?;

    println!("Collecting static files into {output}/");
    println!();

    let mut collected = 0;

    // Collect from dependencies
    for (name, dep) in &manifest.dependencies {
        // Security: sanitize dependency name to prevent path traversal
        let safe_name = sanitize_project_name(name)
            .with_context(|| format!("Invalid dependency name: {name}"))?;

        if let Some(static_path) = find_dependency_static(name, dep, output_path) {
            let dest = output_path.join(&safe_name);
            copy_dir_recursive(&static_path, &dest, output_path)?;
            println!("  + {safe_name}/");
            collected += 1;
        }
    }

    // Also collect local static if present
    let project_name = &manifest.project.name;
    let safe_project_name = sanitize_project_name(project_name)
        .with_context(|| format!("Invalid project name: {project_name}"))?;

    if let Some(local_static) = find_local_static(output_path) {
        let dest = output_path.join(&safe_project_name);
        copy_dir_recursive(&local_static, &dest, output_path)?;
        println!("  + {safe_project_name}/ (local)");
        collected += 1;
    }

    println!();

    if collected == 0 {
        println!("No static files found");
    } else {
        println!("Collected {collected} static folders");
        println!();
        println!("Static files are organized as:");
        println!("  {output}/");
        println!("    <project-name>/");
        println!("      index.html, assets/, ...");
        println!();
        println!("Host can serve these at /static/<project>/");
    }

    Ok(())
}

/// Find static files for a dependency.
fn find_dependency_static(name: &str, dep: &Dependency, exclude: &Path) -> Option<PathBuf> {
    match dep {
        Dependency::Simple(_) => find_module_static(name, exclude),
        Dependency::Detailed(d) => {
            // Local path dependency - check static candidates
            if let Some(path) = &d.path
                && let Some(found) = find_first_static_dir(Path::new(path), exclude)
            {
                return Some(found);
            }
            // Registry dependency - check extracted modules
            if d.registry.is_some() {
                return find_module_static(name, exclude);
            }
            None
        },
    }
}

/// Find static directory in extracted module.
fn find_module_static(name: &str, exclude: &Path) -> Option<PathBuf> {
    let static_path = Path::new("modules").join(name).join("static");
    (static_path.is_dir() && !is_same_path(&static_path, exclude)).then_some(static_path)
}

/// Find first existing static directory from candidates.
fn find_first_static_dir(base: &Path, exclude: &Path) -> Option<PathBuf> {
    STATIC_CANDIDATES
        .iter()
        .map(|c| base.join(c))
        .find(|p| p.is_dir() && !is_same_path(p, exclude))
}

/// Find local static directory.
fn find_local_static(exclude: &Path) -> Option<PathBuf> {
    find_first_static_dir(Path::new("."), exclude)
}

/// Check if two paths are the same (handling relative paths).
fn is_same_path(a: &Path, b: &Path) -> bool {
    if let (Ok(a_canon), Ok(b_canon)) = (a.canonicalize(), b.canonicalize()) {
        a_canon == b_canon
    } else {
        // Fallback to string comparison
        a.to_string_lossy() == b.to_string_lossy()
    }
}

/// Copy directory recursively, excluding the output directory.
fn copy_dir_recursive(src: &Path, dest: &Path, exclude: &Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }

    // Don't copy into ourselves
    if is_same_path(src, exclude) {
        return Ok(());
    }

    fs::create_dir_all(dest)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dest.join(&file_name);

        // Skip if this is the exclude directory
        if is_same_path(&path, exclude) {
            continue;
        }

        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path, exclude)?;
        } else {
            fs::copy(&path, &dest_path)?;
        }
    }

    Ok(())
}
