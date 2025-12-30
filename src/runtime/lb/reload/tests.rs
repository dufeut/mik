//! Tests for graceful reload operations.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use super::super::RoundRobin;
use super::super::backend::Backend;
use super::handle::ReloadHandle;
use super::manager::ReloadManager;
use super::types::{ReloadConfig, ReloadResult, ReloadSignal};

fn create_test_backends(addresses: Vec<&str>) -> Arc<RwLock<Vec<Backend>>> {
    let backends: Vec<Backend> = addresses
        .into_iter()
        .map(|addr| Backend::new(addr.to_string()))
        .collect();
    Arc::new(RwLock::new(backends))
}

#[test]
fn test_reload_config_default() {
    let config = ReloadConfig::default();
    assert_eq!(config.drain_timeout, Duration::from_secs(30));
}

#[test]
fn test_reload_config_custom() {
    let config = ReloadConfig::with_drain_timeout(Duration::from_secs(60));
    assert_eq!(config.drain_timeout, Duration::from_secs(60));
}

#[test]
fn test_reload_signal_new() {
    let backends = vec!["127.0.0.1:3001".to_string(), "127.0.0.1:3002".to_string()];
    let signal = ReloadSignal::new(backends.clone());

    assert_eq!(signal.backends, backends);
    assert!(signal.requested_at.elapsed() < Duration::from_secs(1));
}

#[test]
fn test_reload_handle_creation() {
    let (handle, _receiver) = ReloadHandle::new();
    assert!(handle.has_receivers());
}

#[test]
fn test_reload_handle_trigger() {
    let (handle, mut receiver) = ReloadHandle::new();

    let backends = vec!["127.0.0.1:3001".to_string()];
    assert!(handle.trigger_reload(backends.clone()));

    // Check that the signal was received
    assert!(receiver.has_changed().unwrap());
    let signal = receiver.borrow_and_update();
    assert!(signal.is_some());
    assert_eq!(signal.as_ref().unwrap().backends, backends);
}

#[tokio::test]
async fn test_reload_manager_add_backend() {
    let backends = create_test_backends(vec!["127.0.0.1:3001"]);
    let selection = Arc::new(RwLock::new(RoundRobin::new(1)));
    let config = ReloadConfig::default();

    let manager = ReloadManager::new(config, backends.clone(), selection);

    // Add a new backend
    assert!(manager.add_backend("127.0.0.1:3002".to_string()).await);

    // Verify it was added
    let backends_read = backends.read().await;
    assert_eq!(backends_read.len(), 2);
    assert!(
        backends_read
            .iter()
            .any(|b| b.address() == "127.0.0.1:3002")
    );

    // Adding the same backend again should fail
    drop(backends_read);
    assert!(!manager.add_backend("127.0.0.1:3002".to_string()).await);
}

#[tokio::test]
async fn test_reload_manager_remove_backend() {
    let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
    let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
    let config = ReloadConfig::default();

    let manager = ReloadManager::new(config, backends.clone(), selection);

    // Remove a backend
    assert!(manager.remove_backend("127.0.0.1:3001").await);

    // Verify it's marked for draining
    assert!(manager.is_draining("127.0.0.1:3001").await);
    assert_eq!(manager.draining_count().await, 1);

    // Removing non-existent backend should return false
    assert!(!manager.remove_backend("127.0.0.1:9999").await);
}

#[tokio::test]
async fn test_reload_manager_process_draining_with_no_active_requests() {
    let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
    let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
    let config = ReloadConfig::default();

    let manager = ReloadManager::new(config, backends.clone(), selection);

    // Remove a backend (it has no active requests)
    manager.remove_backend("127.0.0.1:3001").await;

    // Process draining - should remove immediately since no active requests
    let removed = manager.process_draining_backends().await;
    assert_eq!(removed, 1);

    // Verify it was removed
    let backends_read = backends.read().await;
    assert_eq!(backends_read.len(), 1);
    assert!(
        !backends_read
            .iter()
            .any(|b| b.address() == "127.0.0.1:3001")
    );
}

#[tokio::test]
async fn test_reload_manager_drain_timeout() {
    let backends = create_test_backends(vec!["127.0.0.1:3001"]);
    let selection = Arc::new(RwLock::new(RoundRobin::new(1)));

    // Very short timeout for testing
    let config = ReloadConfig::with_drain_timeout(Duration::from_millis(50));

    let manager = ReloadManager::new(config, backends.clone(), selection);

    // Simulate an active request
    {
        let backends_read = backends.read().await;
        backends_read[0].start_request();
    }

    // Remove the backend
    manager.remove_backend("127.0.0.1:3001").await;

    // Process draining - should not remove yet (has active request)
    let removed = manager.process_draining_backends().await;
    assert_eq!(removed, 0);

    // Wait for timeout
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now it should be forcefully removed
    let removed = manager.process_draining_backends().await;
    assert_eq!(removed, 1);

    // Verify it was removed
    let backends_read = backends.read().await;
    assert_eq!(backends_read.len(), 0);
}

#[tokio::test]
async fn test_reload_manager_force_remove() {
    let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
    let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
    let config = ReloadConfig::default();

    let manager = ReloadManager::new(config, backends.clone(), selection);

    // Simulate an active request
    {
        let backends_read = backends.read().await;
        backends_read[0].start_request();
    }

    // Force remove should work even with active requests
    assert!(manager.force_remove("127.0.0.1:3001").await);

    // Verify it was removed immediately
    let backends_read = backends.read().await;
    assert_eq!(backends_read.len(), 1);
    assert!(
        !backends_read
            .iter()
            .any(|b| b.address() == "127.0.0.1:3001")
    );
}

#[tokio::test]
async fn test_reload_manager_apply_reload_add_backends() {
    let backends = create_test_backends(vec!["127.0.0.1:3001"]);
    let selection = Arc::new(RwLock::new(RoundRobin::new(1)));
    let config = ReloadConfig::default();

    let manager = ReloadManager::new(config, backends.clone(), selection);

    // Apply reload with additional backend
    let signal = ReloadSignal::new(vec![
        "127.0.0.1:3001".to_string(),
        "127.0.0.1:3002".to_string(),
    ]);

    let result = manager.apply_reload(&signal).await;

    assert_eq!(result.added.len(), 1);
    assert!(result.added.contains(&"127.0.0.1:3002".to_string()));
    assert!(result.draining.is_empty());
    assert_eq!(result.unchanged.len(), 1);

    // Verify backends
    let backends_read = backends.read().await;
    assert_eq!(backends_read.len(), 2);
}

#[tokio::test]
async fn test_reload_manager_apply_reload_remove_backends() {
    let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
    let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
    let config = ReloadConfig::default();

    let manager = ReloadManager::new(config, backends.clone(), selection);

    // Apply reload with one backend removed
    let signal = ReloadSignal::new(vec!["127.0.0.1:3001".to_string()]);

    let result = manager.apply_reload(&signal).await;

    assert!(result.added.is_empty());
    assert_eq!(result.draining.len(), 1);
    assert!(result.draining.contains(&"127.0.0.1:3002".to_string()));
    assert_eq!(result.unchanged.len(), 1);

    // Backend is still present but draining
    let backends_read = backends.read().await;
    assert_eq!(backends_read.len(), 2);

    // The draining backend should be unhealthy
    let draining = backends_read
        .iter()
        .find(|b| b.address() == "127.0.0.1:3002")
        .unwrap();
    assert!(!draining.is_healthy());
}

#[tokio::test]
async fn test_reload_manager_apply_reload_mixed() {
    let backends = create_test_backends(vec!["127.0.0.1:3001", "127.0.0.1:3002"]);
    let selection = Arc::new(RwLock::new(RoundRobin::new(2)));
    let config = ReloadConfig::default();

    let manager = ReloadManager::new(config, backends.clone(), selection);

    // Apply reload: keep 3001, remove 3002, add 3003
    let signal = ReloadSignal::new(vec![
        "127.0.0.1:3001".to_string(),
        "127.0.0.1:3003".to_string(),
    ]);

    let result = manager.apply_reload(&signal).await;

    assert_eq!(result.added.len(), 1);
    assert!(result.added.contains(&"127.0.0.1:3003".to_string()));
    assert_eq!(result.draining.len(), 1);
    assert!(result.draining.contains(&"127.0.0.1:3002".to_string()));
    assert_eq!(result.unchanged.len(), 1);
    assert!(result.unchanged.contains(&"127.0.0.1:3001".to_string()));

    assert!(result.has_changes());
}

#[tokio::test]
async fn test_reload_result_has_changes() {
    let result = ReloadResult {
        added: vec![],
        draining: vec![],
        unchanged: vec!["127.0.0.1:3001".to_string()],
    };
    assert!(!result.has_changes());

    let result = ReloadResult {
        added: vec!["127.0.0.1:3002".to_string()],
        draining: vec![],
        unchanged: vec!["127.0.0.1:3001".to_string()],
    };
    assert!(result.has_changes());
}

#[tokio::test]
async fn test_reload_manager_wait_for_drain() {
    let backends = create_test_backends(vec!["127.0.0.1:3001"]);
    let selection = Arc::new(RwLock::new(RoundRobin::new(1)));
    let config = ReloadConfig::with_drain_timeout(Duration::from_millis(500));

    let manager = ReloadManager::new(config, backends.clone(), selection);

    // Remove the backend (no active requests)
    manager.remove_backend("127.0.0.1:3001").await;

    // Wait for drain should complete quickly
    let drained = manager.wait_for_drain("127.0.0.1:3001").await;
    assert!(drained);
}
