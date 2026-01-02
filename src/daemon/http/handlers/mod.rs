//! HTTP API handlers organized by service.

pub mod cron;
pub mod instances;
pub mod kv;
pub mod sql;
pub mod storage;

// Re-export all handlers for use in routing
pub(crate) use cron::{
    cron_create, cron_delete, cron_get, cron_history, cron_list, cron_trigger, cron_update,
};
pub(crate) use instances::{
    get_instance, get_logs, health, list_instances, restart_instance, start_instance,
    stop_instance, version,
};
pub(crate) use kv::{kv_delete, kv_get, kv_list, kv_set};
pub(crate) use sql::{sql_batch, sql_execute, sql_query};
pub(crate) use storage::{storage_delete, storage_get, storage_head, storage_list, storage_put};

/// Macro to generate service availability helper functions.
///
/// Each service handler needs a helper function that checks if the service
/// is enabled and returns 503 Service Unavailable if not.
///
/// # Usage
///
/// ```ignore
/// get_service!(get_kv, kv, KvStore, "KV");
/// get_service!(get_sql, sql, SqlService, "SQL");
/// get_service!(get_storage, storage, StorageService, "Storage");
/// ```
macro_rules! get_service {
    ($fn_name:ident, $field:ident, $service_type:ty, $service_name:expr) => {
        async fn $fn_name(
            state: &super::super::SharedState,
        ) -> Result<$service_type, super::super::AppError> {
            let state = state.read().await;
            state.$field.clone().ok_or_else(|| {
                super::super::AppError::ServiceUnavailable(format!(
                    "{} service is disabled. Enable it in ~/.mik/daemon.toml",
                    $service_name
                ))
            })
        }
    };
}

pub(crate) use get_service;
