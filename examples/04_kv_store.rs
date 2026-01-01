//! # Key-Value Store Example
//!
//! This example demonstrates how to use the embedded key-value store service
//! for caching, session data, and general-purpose storage. The KV store is
//! backed by redb, providing ACID guarantees and persistence.
//!
//! ## What This Example Shows
//!
//! - Opening and configuring the KV store
//! - Basic CRUD operations (Create, Read, Update, Delete)
//! - TTL (Time-To-Live) for automatic expiration
//! - Key listing with prefix filtering
//! - Error handling patterns
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example 04_kv_store
//! ```
//!
//! This example creates a temporary database that is cleaned up on exit.
//!
//! ## Related Documentation
//!
//! - [`mik::daemon::services::kv::KvStore`] - The KV store implementation
//! - [redb documentation](https://docs.rs/redb) - Underlying storage engine

use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== mik Key-Value Store Example ===\n");

    // =========================================================================
    // Part 1: Opening the KV Store
    // =========================================================================
    //
    // The KV store can be opened from a specific path or using the default
    // location (~/.mik/kv.redb). For this example, we use a temporary directory.

    println!("--- Part 1: Opening the KV Store ---\n");

    // Create a temporary directory for our example database
    let temp_dir = TempDir::new()?;
    let db_path: PathBuf = temp_dir.path().join("example.redb");

    println!("Creating KV store at: {}", db_path.display());

    // Open the KV store (creates the database if it doesn't exist)
    let kv = mik::daemon::services::kv::KvStore::file(&db_path)?;

    println!("KV store opened successfully!\n");

    // Note: In production, you would typically use:
    // let kv = mik::daemon::services::kv::KvStore::file("~/.mik/kv.redb")?;

    // =========================================================================
    // Part 2: Basic CRUD Operations
    // =========================================================================
    //
    // The KV store provides simple get/set/delete operations.
    // Values are stored as bytes, allowing any binary data.

    println!("--- Part 2: Basic CRUD Operations ---\n");

    // CREATE: Store a value (None for no TTL)
    println!("CREATE: Setting key 'user:1001'");
    let user_data = r#"{"name": "Alice", "email": "alice@example.com"}"#;
    kv.set("user:1001", user_data.as_bytes(), None).await?;
    println!("  Stored: {}", user_data);

    // READ: Retrieve a value
    println!("\nREAD: Getting key 'user:1001'");
    if let Some(value) = kv.get("user:1001").await? {
        let data = String::from_utf8_lossy(&value);
        println!("  Retrieved: {}", data);
    }

    // READ: Non-existent key returns None
    println!("\nREAD: Getting non-existent key 'user:9999'");
    match kv.get("user:9999").await? {
        Some(_) => println!("  Found (unexpected)"),
        None => println!("  Not found (as expected)"),
    }

    // UPDATE: Overwrite existing value
    println!("\nUPDATE: Updating key 'user:1001'");
    let updated_data = r#"{"name": "Alice Smith", "email": "alice.smith@example.com"}"#;
    kv.set("user:1001", updated_data.as_bytes(), None).await?;
    if let Some(value) = kv.get("user:1001").await? {
        let data = String::from_utf8_lossy(&value);
        println!("  Updated: {}", data);
    }

    // DELETE: Remove a key
    println!("\nDELETE: Removing key 'user:1001'");
    let deleted = kv.delete("user:1001").await?;
    println!("  Key existed and was deleted: {}", deleted);

    // Verify deletion
    println!("  Verifying deletion...");
    match kv.get("user:1001").await? {
        Some(_) => println!("  Still exists (unexpected)"),
        None => println!("  Confirmed deleted"),
    }

    // DELETE: Deleting non-existent key is safe (idempotent)
    println!("\nDELETE: Deleting already-deleted key");
    let deleted_again = kv.delete("user:1001").await?;
    println!(
        "  Key existed: {} (safe to call multiple times)",
        deleted_again
    );

    // =========================================================================
    // Part 3: Working with Binary Data
    // =========================================================================
    //
    // The KV store accepts any byte slice, not just strings.
    // This is useful for storing serialized data, images, etc.

    println!("\n--- Part 3: Working with Binary Data ---\n");

    // Store binary data
    let binary_data: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    println!("Storing binary data (PNG header signature)");
    println!("  Bytes: {:?}", binary_data);
    kv.set("image:header", &binary_data, None).await?;

    // Retrieve binary data
    if let Some(value) = kv.get("image:header").await? {
        println!("  Retrieved {} bytes: {:?}", value.len(), value);
    }

    // Clean up
    kv.delete("image:header").await?;

    // =========================================================================
    // Part 4: TTL (Time-To-Live) for Automatic Expiration
    // =========================================================================
    //
    // Keys can be set with a TTL, after which they automatically expire.
    // This is useful for caching, rate limiting, and session management.

    println!("\n--- Part 4: TTL (Time-To-Live) ---\n");

    // Set a key with 2-second TTL
    println!("Setting key 'session:abc123' with 2-second TTL");
    kv.set(
        "session:abc123",
        b"session_data_here",
        Some(Duration::from_secs(2)),
    )
    .await?;

    // Verify it exists immediately
    let exists_now = kv.exists("session:abc123").await?;
    println!("  Exists immediately: {}", exists_now);

    // Wait for expiration
    println!("  Waiting 3 seconds for expiration...");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Check after expiration
    let exists_after = kv.exists("session:abc123").await?;
    println!("  Exists after TTL: {}", exists_after);

    // Accessing an expired key returns None and cleans it up
    match kv.get("session:abc123").await? {
        Some(_) => println!("  Found (unexpected - TTL should have expired)"),
        None => println!("  Not found (TTL expired as expected)"),
    }

    // =========================================================================
    // Part 5: Key Listing and Prefix Filtering
    // =========================================================================
    //
    // You can list all keys or filter by prefix. This is useful for
    // finding related entries or implementing namespaced storage.

    println!("\n--- Part 5: Key Listing and Prefix Filtering ---\n");

    // Create some test data with namespaced keys
    let test_keys = vec![
        ("users:1", "Alice"),
        ("users:2", "Bob"),
        ("users:3", "Charlie"),
        ("products:1", "Widget"),
        ("products:2", "Gadget"),
        ("settings:theme", "dark"),
    ];

    println!("Creating test data:");
    for (key, value) in &test_keys {
        kv.set(key, value.as_bytes(), None).await?;
        println!("  {} = {}", key, value);
    }

    // List all keys
    println!("\nListing all keys:");
    let all_keys = kv.list_keys(None).await?;
    for key in &all_keys {
        println!("  {}", key);
    }

    // List keys with prefix
    println!("\nListing keys with prefix 'users:':");
    let user_keys = kv.list_keys(Some("users:")).await?;
    for key in &user_keys {
        println!("  {}", key);
    }

    println!("\nListing keys with prefix 'products:':");
    let product_keys = kv.list_keys(Some("products:")).await?;
    for key in &product_keys {
        println!("  {}", key);
    }

    println!("\nListing keys with prefix 'nonexistent:':");
    let empty_keys = kv.list_keys(Some("nonexistent:")).await?;
    println!("  Found {} keys (empty as expected)", empty_keys.len());

    // Clean up test data
    for (key, _) in &test_keys {
        kv.delete(key).await?;
    }

    // =========================================================================
    // Part 6: Checking Key Existence
    // =========================================================================
    //
    // The exists() method is a convenient way to check if a key exists
    // without retrieving the full value.

    println!("\n--- Part 6: Checking Key Existence ---\n");

    // Set a key
    kv.set("test:exists", b"value", None).await?;

    // Check existence
    println!("Checking key existence:");
    println!("  'test:exists': {}", kv.exists("test:exists").await?);
    println!("  'test:missing': {}", kv.exists("test:missing").await?);

    // Clean up
    kv.delete("test:exists").await?;

    // =========================================================================
    // Part 7: Error Handling Patterns
    // =========================================================================
    //
    // The KV store uses anyhow::Result for error handling, providing
    // detailed context about what went wrong.

    println!("\n--- Part 7: Error Handling Patterns ---\n");

    // Demonstrate proper error handling patterns
    async fn process_user_data(
        kv: &mik::daemon::services::kv::KvStore,
        user_id: &str,
    ) -> Result<String> {
        let key = format!("user:{}", user_id);

        // The ? operator propagates errors with context
        match kv.get(&key).await? {
            Some(data) => {
                // Convert bytes to string, handling potential UTF-8 errors
                let json = String::from_utf8(data)
                    .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in user data: {}", e))?;
                Ok(json)
            },
            None => {
                // Return a clear error for missing data
                Err(anyhow::anyhow!("User {} not found", user_id))
            },
        }
    }

    // Set up test data
    kv.set("user:alice", b"valid user data", None).await?;
    kv.set("user:broken", &[0xFF, 0xFE], None).await?; // Invalid UTF-8

    // Test the function
    println!("Testing error handling patterns:");

    // Success case
    match process_user_data(&kv, "alice").await {
        Ok(data) => println!("  user:alice -> Ok({})", data),
        Err(e) => println!("  user:alice -> Err({})", e),
    }

    // Not found case
    match process_user_data(&kv, "unknown").await {
        Ok(data) => println!("  user:unknown -> Ok({})", data),
        Err(e) => println!("  user:unknown -> Err({})", e),
    }

    // Invalid data case
    match process_user_data(&kv, "broken").await {
        Ok(data) => println!("  user:broken -> Ok({})", data),
        Err(e) => println!("  user:broken -> Err({})", e),
    }

    // Clean up
    kv.delete("user:alice").await?;
    kv.delete("user:broken").await?;

    // =========================================================================
    // Part 8: Thread Safety and Cloning
    // =========================================================================
    //
    // KvStore is Clone and thread-safe. You can share it across tasks
    // using Arc or by cloning (internally uses Arc).

    println!("\n--- Part 8: Thread Safety and Cloning ---\n");

    // Clone the store (cheap - internally uses Arc)
    let kv_clone = kv.clone();

    // Both references access the same underlying database
    kv.set("shared:key", b"original", None).await?;
    if let Some(value) = kv_clone.get("shared:key").await? {
        println!(
            "Clone sees original value: {}",
            String::from_utf8_lossy(&value)
        );
    }

    // Updates from clone are visible to original
    kv_clone
        .set("shared:key", b"updated by clone", None)
        .await?;
    if let Some(value) = kv.get("shared:key").await? {
        println!(
            "Original sees clone's update: {}",
            String::from_utf8_lossy(&value)
        );
    }

    // Clean up
    kv.delete("shared:key").await?;

    println!("\nIn a multi-threaded context, you would use:");
    println!("  let kv = Arc::new(KvStore::file(...)?);");
    println!("  let kv_task = kv.clone();");
    println!("  tokio::spawn(async move {{ kv_task.set(...).await }});");

    // =========================================================================
    // Part 9: Best Practices
    // =========================================================================

    println!("\n--- Part 9: Best Practices ---\n");

    println!("Key naming conventions:");
    println!("  - Use namespaces: 'users:', 'sessions:', 'cache:'");
    println!("  - Include identifiers: 'user:1001', 'session:abc123'");
    println!("  - Be consistent: 'type:id' or 'type/id'");

    println!("\nTTL recommendations:");
    println!("  - Sessions: 30 minutes to 24 hours");
    println!("  - Cache: seconds to minutes based on freshness needs");
    println!("  - Rate limits: typically 1 minute windows");

    println!("\nData format recommendations:");
    println!("  - Use JSON for structured data (human-readable, debuggable)");
    println!("  - Use MessagePack/CBOR for high-volume or size-sensitive data");
    println!("  - Store raw bytes for binary content");

    println!("\n=== Example Complete ===");

    // The temporary directory and database are automatically cleaned up
    // when temp_dir goes out of scope

    Ok(())
}
