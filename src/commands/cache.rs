//! Cache management commands.
//!
//! Provides commands for managing caches:
//! - `mik cache info` - Display cache statistics and location
//! - `mik cache clean` - Remove stale entries to free disk space
//! - `mik cache clear` - Remove all cached entries
//!
//! Manages two caches:
//! - **AOT cache**: Pre-compiled WASM components for faster startup
//! - **OCI cache**: Downloaded registry artifacts (content-addressable)

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

use crate::CacheAction;
use crate::runtime::aot_cache::{AotCache, AotCacheConfig};

/// Get OCI cache directory.
fn get_oci_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("mik").join("oci").join("blobs"))
}

/// Get OCI cache statistics.
fn get_oci_cache_stats() -> (u64, u64) {
    let Some(cache_dir) = get_oci_cache_dir() else {
        return (0, 0);
    };

    if !cache_dir.exists() {
        return (0, 0);
    }

    let mut entry_count = 0u64;
    let mut total_size = 0u64;

    // Walk through sha256/ subdirectory
    if let Ok(entries) = fs::read_dir(cache_dir.join("sha256")) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                entry_count += 1;
                if let Ok(meta) = entry.metadata() {
                    total_size += meta.len();
                }
            }
        }
    }

    (entry_count, total_size)
}

/// Clear OCI cache.
fn clear_oci_cache() -> (u64, u64) {
    let Some(cache_dir) = get_oci_cache_dir() else {
        return (0, 0);
    };

    if !cache_dir.exists() {
        return (0, 0);
    }

    let (count, size) = get_oci_cache_stats();

    if fs::remove_dir_all(&cache_dir).is_ok() {
        (count, size)
    } else {
        (0, 0)
    }
}

/// Execute cache management command.
pub fn execute(action: CacheAction) -> Result<()> {
    let cache = AotCache::new(AotCacheConfig::default())?;

    match action {
        CacheAction::Info => {
            // AOT cache stats
            let stats = cache.stats()?;
            println!("AOT Cache (compiled modules)");
            println!("============================");
            println!("Location:    {}", stats.cache_dir.display());
            println!("Entries:     {}", stats.entry_count);
            println!("Total size:  {} MB", stats.total_size_bytes / (1024 * 1024));
            println!("Max size:    {} MB", stats.max_size_bytes / (1024 * 1024));
            println!(
                "Usage:       {:.1}%",
                (stats.total_size_bytes as f64 / stats.max_size_bytes as f64) * 100.0
            );

            // OCI cache stats
            let (oci_count, oci_size) = get_oci_cache_stats();
            println!();
            println!("OCI Cache (registry artifacts)");
            println!("==============================");
            if let Some(dir) = get_oci_cache_dir() {
                println!("Location:    {}", dir.display());
            }
            println!("Entries:     {oci_count}");
            println!("Total size:  {} KB", oci_size / 1024);
        },
        CacheAction::Clean { max_size_mb } => {
            // Create cache with custom max size for cleanup
            let config = AotCacheConfig {
                max_size_bytes: max_size_mb * 1024 * 1024,
                bypass: false,
            };
            let cache = AotCache::new(config)?;
            let stats = cache.cleanup()?;

            if stats.entries_removed == 0 {
                println!("AOT cache is already within size limit. Nothing to clean.");
            } else {
                println!("AOT cache cleaned successfully");
                println!("  Entries removed: {}", stats.entries_removed);
                println!(
                    "  Space freed:     {} MB",
                    stats.bytes_freed / (1024 * 1024)
                );
                println!(
                    "  Current size:    {} MB",
                    stats.current_size_bytes / (1024 * 1024)
                );
            }
        },
        CacheAction::Clear => {
            // Clear AOT cache
            let aot_stats = cache.clear()?;

            // Clear OCI cache
            let (oci_count, oci_size) = clear_oci_cache();

            let total_removed = aot_stats.entries_removed as u64 + oci_count;
            let total_freed = aot_stats.bytes_freed + oci_size;

            if total_removed == 0 {
                println!("Caches are already empty.");
            } else {
                println!("Caches cleared successfully");
                println!("  AOT entries removed: {}", aot_stats.entries_removed);
                println!("  OCI entries removed: {oci_count}");
                println!("  Total space freed:   {} MB", total_freed / (1024 * 1024));
            }
        },
    }

    Ok(())
}
