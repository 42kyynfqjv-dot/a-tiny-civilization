//! Application use cases and infrastructure ports.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sim_engine::Snapshot;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    Digest, EventBatch, EventSequence, SimTick, WorldId, WorldManifest, WorldStatus,
};

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
    #[error("persistent record was not found: {0}")]
    NotFound(String),
    #[error("persistent record failed integrity validation: {0}")]
    Corrupt(String),
}

#[async_trait]
pub trait FoundationStore: Send + Sync {
    async fn ready(&self) -> Result<(), StoreError>;

    async fn record_heartbeat(&self, heartbeat: &ServiceHeartbeat) -> Result<(), StoreError>;

    async fn foundation_status(&self) -> Result<FoundationStatus, StoreError>;
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldCursor {
    pub sequence: EventSequence,
    pub tick: SimTick,
    pub last_event_hash: Digest,
    pub state_hash: Digest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoredWorld {
    pub manifest: WorldManifest,
    pub status: WorldStatus,
    pub cursor: WorldCursor,
    pub predecessor_world_id: Option<WorldId>,
}

/// Atomic persistence boundary used only by the simulation application.
#[async_trait]
pub trait WorldStore: Send + Sync {
    async fn create_world(
        &self,
        manifest: &WorldManifest,
        predecessor_world_id: Option<WorldId>,
    ) -> Result<StoredWorld, StoreError>;

    async fn load_world(&self, world_id: WorldId) -> Result<StoredWorld, StoreError>;

    async fn load_event_batches(
        &self,
        world_id: WorldId,
        after_sequence: EventSequence,
    ) -> Result<Vec<EventBatch>, StoreError>;

    async fn load_latest_snapshot(&self, world_id: WorldId) -> Result<Snapshot, StoreError>;

    async fn commit_transition(
        &self,
        expected: WorldCursor,
        batch: &EventBatch,
        snapshot: &Snapshot,
    ) -> Result<StoredWorld, StoreError>;
}
