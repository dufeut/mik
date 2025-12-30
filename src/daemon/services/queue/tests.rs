//! Tests for the queue service.

use super::*;
use anyhow::Result;
use chrono::Utc;
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

#[test]
fn test_push_pop() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    // Push messages
    let id1 = service.push("test_queue", b"message 1")?;
    let id2 = service.push("test_queue", b"message 2")?;

    assert_ne!(id1, id2);
    assert_eq!(service.len("test_queue")?, 2);

    // Pop messages in FIFO order
    let msg1 = service.pop("test_queue")?.unwrap();
    assert_eq!(msg1.data, b"message 1");
    assert_eq!(msg1.id, id1);

    let msg2 = service.pop("test_queue")?.unwrap();
    assert_eq!(msg2.data, b"message 2");
    assert_eq!(msg2.id, id2);

    // Queue should be empty now
    assert_eq!(service.len("test_queue")?, 0);
    assert!(service.pop("test_queue")?.is_none());

    Ok(())
}

#[test]
fn test_peek() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    service.push("test_queue", b"message 1")?;
    service.push("test_queue", b"message 2")?;

    // Peek doesn't remove
    let msg = service.peek("test_queue")?.unwrap();
    assert_eq!(msg.data, b"message 1");
    assert_eq!(service.len("test_queue")?, 2);

    // Peek again, should be the same
    let msg = service.peek("test_queue")?.unwrap();
    assert_eq!(msg.data, b"message 1");

    Ok(())
}

#[test]
fn test_clear() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    service.push("test_queue", b"message 1")?;
    service.push("test_queue", b"message 2")?;
    service.push("test_queue", b"message 3")?;

    let count = service.clear("test_queue")?;
    assert_eq!(count, 3);
    assert_eq!(service.len("test_queue")?, 0);

    Ok(())
}

#[test]
fn test_multiple_queues() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    service.push("queue_a", b"message a1")?;
    service.push("queue_b", b"message b1")?;
    service.push("queue_a", b"message a2")?;

    assert_eq!(service.len("queue_a")?, 2);
    assert_eq!(service.len("queue_b")?, 1);

    let msg_a = service.pop("queue_a")?.unwrap();
    assert_eq!(msg_a.data, b"message a1");

    let msg_b = service.pop("queue_b")?.unwrap();
    assert_eq!(msg_b.data, b"message b1");

    assert_eq!(service.len("queue_a")?, 1);
    assert_eq!(service.len("queue_b")?, 0);

    Ok(())
}

#[test]
fn test_max_queue_size() -> Result<()> {
    let config = QueueConfig {
        max_queue_size: Some(2),
        ..Default::default()
    };
    let service = QueueService::new(config)?;

    service.push("test_queue", b"message 1")?;
    service.push("test_queue", b"message 2")?;

    // Third push should fail
    let result = service.push("test_queue", b"message 3");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Queue is full"));

    Ok(())
}

#[test]
fn test_list_queues() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    service.push("queue_a", b"message")?;
    service.push("queue_b", b"message")?;
    service.push("queue_c", b"message")?;

    let mut queues = service.list_queues();
    queues.sort();

    assert_eq!(queues, vec!["queue_a", "queue_b", "queue_c"]);

    Ok(())
}

#[test]
fn test_delete_queue() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    service.push("test_queue", b"message 1")?;
    service.push("test_queue", b"message 2")?;

    assert!(service.delete_queue("test_queue")?);
    assert_eq!(service.len("test_queue")?, 0);
    assert!(!service.delete_queue("test_queue")?);

    Ok(())
}

#[tokio::test]
async fn test_pubsub() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    // Subscribe to a topic
    let mut rx1 = service.subscribe("test_topic");
    let mut rx2 = service.subscribe("test_topic");

    assert_eq!(service.subscriber_count("test_topic"), 2);

    // Publish a message
    let count = service.publish("test_topic", b"hello world")?;
    assert_eq!(count, 2);

    // Both subscribers should receive it
    let msg1 = timeout(Duration::from_millis(100), rx1.recv())
        .await?
        .unwrap();
    assert_eq!(msg1.data, b"hello world");

    let msg2 = timeout(Duration::from_millis(100), rx2.recv())
        .await?
        .unwrap();
    assert_eq!(msg2.data, b"hello world");

    Ok(())
}

#[tokio::test]
async fn test_pubsub_no_subscribers() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    // Publish to a topic with no subscribers
    let count = service.publish("empty_topic", b"message")?;
    assert_eq!(count, 0);

    Ok(())
}

#[test]
fn test_list_topics() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    service.subscribe("topic_a");
    service.subscribe("topic_b");
    service.subscribe("topic_c");

    let mut topics = service.list_topics();
    topics.sort();

    assert_eq!(topics, vec!["topic_a", "topic_b", "topic_c"]);

    Ok(())
}

#[test]
fn test_persistence() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("queue.db");

    // Create service with persistence
    {
        let config = QueueConfig {
            persist: true,
            db_path: Some(db_path.clone()),
            max_queue_size: None,
        };
        let service = QueueService::new(config)?;

        service.push("persistent_queue", b"message 1")?;
        service.push("persistent_queue", b"message 2")?;
        service.push("another_queue", b"message 3")?;
    }

    // Reload from disk
    {
        let config = QueueConfig {
            persist: true,
            db_path: Some(db_path.clone()),
            max_queue_size: None,
        };
        let service = QueueService::new(config)?;

        assert_eq!(service.len("persistent_queue")?, 2);
        assert_eq!(service.len("another_queue")?, 1);

        let msg1 = service.pop("persistent_queue")?.unwrap();
        assert_eq!(msg1.data, b"message 1");

        let msg2 = service.pop("persistent_queue")?.unwrap();
        assert_eq!(msg2.data, b"message 2");

        let msg3 = service.pop("another_queue")?.unwrap();
        assert_eq!(msg3.data, b"message 3");
    }

    Ok(())
}

#[test]
fn test_message_metadata() -> Result<()> {
    let service = QueueService::new(QueueConfig::default())?;

    let before = Utc::now();
    let id = service.push("test_queue", b"test message")?;
    let after = Utc::now();

    let msg = service.pop("test_queue")?.unwrap();

    // Check ID format (UUID v4)
    assert_eq!(msg.id, id);
    assert!(Uuid::parse_str(&msg.id).is_ok());

    // Check timestamp is within reasonable bounds
    assert!(msg.created_at >= before);
    assert!(msg.created_at <= after);

    Ok(())
}
