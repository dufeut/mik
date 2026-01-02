//! KV service handlers.
//!
//! Handlers for key-value store operations including listing keys,
//! getting values, setting values with optional TTL, and deleting keys.

use std::time::Duration;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use super::super::types::{KvGetResponse, KvListQuery, KvListResponse, KvSetRequest};
use super::super::{AppError, SharedState, metrics};
use super::get_service;

// Generate the get_kv helper using the shared macro
get_service!(get_kv, kv, crate::daemon::services::kv::KvStore, "KV");

/// GET /kv - List all keys with optional prefix filter.
pub(crate) async fn kv_list(
    State(state): State<SharedState>,
    Query(query): Query<KvListQuery>,
) -> Result<Json<KvListResponse>, AppError> {
    metrics::record_kv_operation("list");
    let kv = get_kv(&state).await?;
    let keys = kv.list_keys(query.prefix.as_deref()).await?;
    Ok(Json(KvListResponse { keys }))
}

/// GET /kv/:key - Get a value by key.
pub(crate) async fn kv_get(
    State(state): State<SharedState>,
    Path(key): Path<String>,
) -> Result<Json<KvGetResponse>, AppError> {
    metrics::record_kv_operation("get");
    let kv = get_kv(&state).await?;
    let bytes = kv
        .get(&key)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Key '{key}' not found")))?;

    let value = String::from_utf8(bytes)
        .map_err(|_| AppError::Internal("Value is not valid UTF-8".to_string()))?;

    Ok(Json(KvGetResponse { key, value }))
}

/// PUT /kv/:key - Set a value with optional TTL.
pub(crate) async fn kv_set(
    State(state): State<SharedState>,
    Path(key): Path<String>,
    Json(req): Json<KvSetRequest>,
) -> Result<StatusCode, AppError> {
    metrics::record_kv_operation("set");
    let kv = get_kv(&state).await?;
    let value_bytes = req.value.into_bytes();
    let ttl = req.ttl.map(Duration::from_secs);

    kv.set(&key, &value_bytes, ttl).await?;

    Ok(StatusCode::OK)
}

/// DELETE /kv/:key - Delete a key.
pub(crate) async fn kv_delete(
    State(state): State<SharedState>,
    Path(key): Path<String>,
) -> Result<StatusCode, AppError> {
    metrics::record_kv_operation("delete");
    let kv = get_kv(&state).await?;
    kv.delete(&key).await?;
    Ok(StatusCode::NO_CONTENT)
}
