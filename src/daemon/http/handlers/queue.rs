//! Queue service handlers.
//!
//! Handlers for message queue operations including push, pop, peek,
//! and topic publish/subscribe functionality.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

use super::super::types::{
    ListQueuesResponse, QueueInfoResponse, QueueMessageInfo, QueuePeekResponse, QueuePopResponse,
    QueuePushRequest, TopicPublishRequest,
};
use super::super::{AppError, SharedState, metrics};

/// GET /queues - List all queues.
pub(crate) async fn queue_list(
    State(state): State<SharedState>,
) -> Result<Json<ListQueuesResponse>, AppError> {
    metrics::record_queue_operation("list", "all");
    let state = state.read().await;
    let names = state.queue.list_queues();

    let mut queues = Vec::with_capacity(names.len());
    for name in names {
        let length = state.queue.len(&name)?;
        metrics::set_queue_length(&name, length);
        queues.push(QueueInfoResponse {
            name,
            length,
            persistent: false, // In-memory queues
        });
    }

    Ok(Json(ListQueuesResponse { queues }))
}

/// GET /queues/:name - Get queue info.
pub(crate) async fn queue_info(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> Result<Json<QueueInfoResponse>, AppError> {
    metrics::record_queue_operation("info", &name);
    let state = state.read().await;
    let length = state.queue.len(&name)?;
    metrics::set_queue_length(&name, length);

    Ok(Json(QueueInfoResponse {
        name,
        length,
        persistent: false,
    }))
}

/// DELETE /queues/:name - Delete a queue.
pub(crate) async fn queue_delete(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> Result<StatusCode, AppError> {
    metrics::record_queue_operation("delete", &name);
    let state = state.write().await;
    state.queue.delete_queue(&name)?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /queues/:name/push - Push a message to a queue.
pub(crate) async fn queue_push(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(req): Json<QueuePushRequest>,
) -> Result<StatusCode, AppError> {
    metrics::record_queue_operation("push", &name);
    let state = state.write().await;
    let payload = serde_json::to_vec(&req.payload)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON payload: {e}")))?;

    state.queue.push(&name, &payload)?;
    Ok(StatusCode::CREATED)
}

/// POST /queues/:name/pop - Pop a message from a queue.
pub(crate) async fn queue_pop(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> Result<Json<QueuePopResponse>, AppError> {
    metrics::record_queue_operation("pop", &name);
    let state = state.write().await;
    let message = state.queue.pop(&name)?;

    let response = match message {
        Some(msg) => {
            let payload: serde_json::Value =
                serde_json::from_slice(&msg.data).unwrap_or_else(|_| {
                    // Encode as hex if not valid JSON
                    let hex = msg.data.iter().fold(String::new(), |mut acc, b| {
                        use std::fmt::Write;
                        let _ = write!(acc, "{b:02x}");
                        acc
                    });
                    serde_json::json!({"hex": hex})
                });

            QueuePopResponse {
                message: Some(QueueMessageInfo {
                    id: msg.id,
                    payload,
                    created_at: msg.created_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                }),
            }
        },
        None => QueuePopResponse { message: None },
    };

    Ok(Json(response))
}

/// GET /queues/:name/peek - Peek at the next message without removing it.
pub(crate) async fn queue_peek(
    State(state): State<SharedState>,
    Path(name): Path<String>,
) -> Result<Json<QueuePeekResponse>, AppError> {
    metrics::record_queue_operation("peek", &name);
    let state = state.read().await;
    let message = state.queue.peek(&name)?;

    let response = match message {
        Some(msg) => {
            let payload: serde_json::Value =
                serde_json::from_slice(&msg.data).unwrap_or_else(|_| {
                    // Encode as hex if not valid JSON
                    let hex = msg.data.iter().fold(String::new(), |mut acc, b| {
                        use std::fmt::Write;
                        let _ = write!(acc, "{b:02x}");
                        acc
                    });
                    serde_json::json!({"hex": hex})
                });

            QueuePeekResponse {
                message: Some(QueueMessageInfo {
                    id: msg.id,
                    payload,
                    created_at: msg.created_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                }),
            }
        },
        None => QueuePeekResponse { message: None },
    };

    Ok(Json(response))
}

/// POST /topics/:name/publish - Publish a message to a topic.
pub(crate) async fn topic_publish(
    State(state): State<SharedState>,
    Path(name): Path<String>,
    Json(req): Json<TopicPublishRequest>,
) -> Result<StatusCode, AppError> {
    metrics::record_queue_operation("publish", &name);
    let state = state.write().await;
    let payload = serde_json::to_vec(&req.payload)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON payload: {e}")))?;

    state.queue.publish(&name, &payload)?;
    Ok(StatusCode::OK)
}
