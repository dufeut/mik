//! Tests for the KV store module.

use super::*;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_set_and_get() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    store.set("key1", b"value1").unwrap();
    let value = store.get("key1").unwrap().unwrap();
    assert_eq!(value, b"value1");
}

#[test]
fn test_get_nonexistent_key() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    let result = store.get("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_delete() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    store.set("key1", b"value1").unwrap();
    let deleted = store.delete("key1").unwrap();
    assert!(deleted);

    let result = store.get("key1").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_delete_nonexistent() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    let deleted = store.delete("nonexistent").unwrap();
    assert!(!deleted);
}

#[test]
fn test_exists() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    assert!(!store.exists("key1").unwrap());

    store.set("key1", b"value1").unwrap();
    assert!(store.exists("key1").unwrap());

    store.delete("key1").unwrap();
    assert!(!store.exists("key1").unwrap());
}

#[test]
fn test_list_keys_all() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    store.set("key1", b"value1").unwrap();
    store.set("key2", b"value2").unwrap();
    store.set("other", b"value3").unwrap();

    let keys = store.list_keys(None).unwrap();
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&"key1".to_string()));
    assert!(keys.contains(&"key2".to_string()));
    assert!(keys.contains(&"other".to_string()));
}

#[test]
fn test_list_keys_with_prefix() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    store.set("user:1", b"alice").unwrap();
    store.set("user:2", b"bob").unwrap();
    store.set("session:abc", b"xyz").unwrap();

    let user_keys = store.list_keys(Some("user:")).unwrap();
    assert_eq!(user_keys.len(), 2);
    assert!(user_keys.contains(&"user:1".to_string()));
    assert!(user_keys.contains(&"user:2".to_string()));

    let session_keys = store.list_keys(Some("session:")).unwrap();
    assert_eq!(session_keys.len(), 1);
    assert!(session_keys.contains(&"session:abc".to_string()));
}

#[test]
fn test_overwrite_value() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    store.set("key1", b"value1").unwrap();
    store.set("key1", b"value2").unwrap();

    let value = store.get("key1").unwrap().unwrap();
    assert_eq!(value, b"value2");
}

#[test]
fn test_binary_data() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    let binary_data = vec![0u8, 1, 2, 3, 255, 128, 64];
    store.set("binary", &binary_data).unwrap();

    let value = store.get("binary").unwrap().unwrap();
    assert_eq!(value, binary_data);
}

#[test]
fn test_ttl_expiration() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    // Set with 2 second TTL (enough time to verify it exists on slow CI)
    store.set_with_ttl("temp", b"expires soon", 2).unwrap();

    // Should exist immediately
    assert!(store.exists("temp").unwrap());
    let value = store.get("temp").unwrap().unwrap();
    assert_eq!(value, b"expires soon");

    // Wait for expiration (3 seconds to ensure TTL has passed)
    thread::sleep(Duration::from_secs(3));

    // Should be expired and automatically removed
    assert!(!store.exists("temp").unwrap());
    assert!(store.get("temp").unwrap().is_none());
}

#[test]
fn test_ttl_no_expiration() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    // Set with long TTL
    store.set_with_ttl("key", b"value", 3600).unwrap();

    // Should still exist
    assert!(store.exists("key").unwrap());
    let value = store.get("key").unwrap().unwrap();
    assert_eq!(value, b"value");
}

#[test]
fn test_list_keys_filters_expired() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    // Add permanent key
    store.set("permanent", b"forever").unwrap();

    // Add temporary key with 1 second TTL
    store.set_with_ttl("temporary", b"short-lived", 1).unwrap();

    // Both should appear initially
    let keys = store.list_keys(None).unwrap();
    assert_eq!(keys.len(), 2);

    // Wait for expiration
    thread::sleep(Duration::from_secs(2));

    // Only permanent key should remain
    let keys = store.list_keys(None).unwrap();
    assert_eq!(keys.len(), 1);
    assert!(keys.contains(&"permanent".to_string()));
}

#[test]
fn test_persistence_across_reopens() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");

    {
        let store = KvStore::open(&db_path).unwrap();
        store.set("persistent", b"value").unwrap();
    }

    // Reopen database and verify data persists
    {
        let store = KvStore::open(&db_path).unwrap();
        let value = store.get("persistent").unwrap().unwrap();
        assert_eq!(value, b"value");
    }
}

#[test]
fn test_clear() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    store.set("key1", b"value1").unwrap();
    store.set("key2", b"value2").unwrap();
    store.set("key3", b"value3").unwrap();

    assert_eq!(store.list_keys(None).unwrap().len(), 3);

    store.clear().unwrap();

    assert_eq!(store.list_keys(None).unwrap().len(), 0);
    assert!(!store.exists("key1").unwrap());
}

#[test]
fn test_update_ttl() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    // Set with short TTL
    store.set_with_ttl("key", b"value1", 1).unwrap();

    // Overwrite with longer TTL before expiration
    store.set_with_ttl("key", b"value2", 3600).unwrap();

    // Wait for original TTL to pass
    thread::sleep(Duration::from_secs(2));

    // Should still exist with new value
    assert!(store.exists("key").unwrap());
    let value = store.get("key").unwrap().unwrap();
    assert_eq!(value, b"value2");
}

#[test]
fn test_empty_value() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.redb");
    let store = KvStore::open(&db_path).unwrap();

    store.set("empty", b"").unwrap();
    let value = store.get("empty").unwrap().unwrap();
    assert_eq!(value, b"");
}
