//! Application use cases and infrastructure ports.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ServiceHeartbeat {
    pub service_name: String,
    pub instance_id: Uuid,
    pub metadata: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FoundationStatus {
    pub database_time: DateTime<Utc>,
    pub initializing_worlds: i64,
    pub running_worlds: i64,
    pub archived_worlds: i64,
    pub latest_runner_heartbeat: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("persistence service is unavailable: {0}")]
    Unavailable(String),
    #[error("persistence migration failed: {0}")]
    Migration(String),
    #[error("persistence operation conflicted: {0}")]
    Conflict(String),
}

#[async_trait]
pub trait FoundationStore: Send + Sync {
    async fn ready(&self) -> Result<(), StoreError>;

    async fn record_heartbeat(&self, heartbeat: &ServiceHeartbeat) -> Result<(), StoreError>;

    async fn foundation_status(&self) -> Result<FoundationStatus, StoreError>;
}
