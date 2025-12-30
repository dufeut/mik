//! HTTP API handlers organized by service.

pub mod cron;
pub mod instances;
pub mod kv;
pub mod queue;
pub mod services;
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
pub(crate) use queue::{
    queue_delete, queue_info, queue_list, queue_peek, queue_pop, queue_push, topic_publish,
};
pub(crate) use services::{
    services_delete, services_get, services_heartbeat, services_list, services_register,
};
pub(crate) use sql::{sql_batch, sql_execute, sql_query};
pub(crate) use storage::{storage_delete, storage_get, storage_head, storage_list, storage_put};
