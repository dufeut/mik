//! AOT cache management commands.

use anyhow::Result;

use crate::CacheAction;
use crate::runtime::aot_cache::{AotCache, AotCacheConfig};

/// Execute cache management command.
pub fn execute(action: CacheAction) -> Result<()> {
    let cache = AotCache::new(AotCacheConfig::default())?;

    match action {
        CacheAction::Info => {
            let stats = cache.stats()?;
            println!("AOT Cache Statistics");
            println!("====================");
            println!("Location:    {}", stats.cache_dir.display());
            println!("Entries:     {}", stats.entry_count);
            println!("Total size:  {} MB", stats.total_size_bytes / (1024 * 1024));
            println!("Max size:    {} MB", stats.max_size_bytes / (1024 * 1024));
            println!(
                "Usage:       {:.1}%",
                (stats.total_size_bytes as f64 / stats.max_size_bytes as f64) * 100.0
            );
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
                println!("Cache is already within size limit. Nothing to clean.");
            } else {
                println!("Cache cleaned successfully");
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
            let stats = cache.clear()?;

            if stats.entries_removed == 0 {
                println!("Cache is already empty.");
            } else {
                println!("Cache cleared successfully");
                println!("  Entries removed: {}", stats.entries_removed);
                println!(
                    "  Space freed:     {} MB",
                    stats.bytes_freed / (1024 * 1024)
                );
            }
        },
    }

    Ok(())
}
