//! Application use cases and infrastructure ports.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sim_engine::{
    EngineError, EngineState, InitialOrganism, Snapshot, replay, replay_from_snapshot,
};
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

    async fn list_running_world_ids(&self) -> Result<Vec<WorldId>, StoreError>;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldSession {
    pub world: StoredWorld,
    pub state: EngineState,
}

#[derive(Debug, Error)]
pub enum WorldRuntimeError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("world history failed runtime verification: {0}")]
    Integrity(String),
}

impl WorldRuntimeError {
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Store(
                StoreError::Unavailable(_) | StoreError::Conflict(_) | StoreError::NotFound(_)
            )
        )
    }
}

/// Creates and commits genesis, or safely resumes an identical interrupted request.
///
/// The caller must supply every pre-genesis input explicitly. This use case never
/// selects a seed, organism, or predecessor on the caller's behalf.
pub async fn initialize_or_resume_world<S: WorldStore + ?Sized>(
    store: &S,
    manifest: WorldManifest,
    predecessor_world_id: Option<WorldId>,
    initial_organisms: Vec<InitialOrganism>,
) -> Result<WorldSession, WorldRuntimeError> {
    let initial = EngineState::new(manifest.clone());
    let initial_hash = initial.state_hash().map_err(EngineError::from)?;
    let genesis_events = initial.plan_genesis(initial_organisms)?;
    let genesis_sequence = EventSequence::new(1);
    let (running, genesis_batch) =
        initial.commit(genesis_sequence, Digest::ZERO, genesis_events)?;
    let genesis_snapshot = Snapshot::new(
        running.clone(),
        genesis_batch.sequence,
        genesis_batch.batch_hash,
    )?;

    let stored = match store.load_world(manifest.world_id).await {
        Ok(existing) => existing,
        Err(StoreError::NotFound(_)) => store.create_world(&manifest, predecessor_world_id).await?,
        Err(error) => return Err(error.into()),
    };
    if stored.manifest != manifest || stored.predecessor_world_id != predecessor_world_id {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {} already exists with different immutable provenance",
            manifest.world_id
        )));
    }

    if stored.status == WorldStatus::Initializing {
        if stored.cursor.sequence != EventSequence::ZERO
            || stored.cursor.tick != SimTick::ZERO
            || stored.cursor.last_event_hash != Digest::ZERO
            || stored.cursor.state_hash != initial_hash
        {
            return Err(WorldRuntimeError::Integrity(format!(
                "initializing world {} has a non-genesis cursor",
                manifest.world_id
            )));
        }
        let world = store
            .commit_transition(stored.cursor, &genesis_batch, &genesis_snapshot)
            .await?;
        return Ok(WorldSession {
            world,
            state: running,
        });
    }

    let batches = store
        .load_event_batches(manifest.world_id, EventSequence::ZERO)
        .await?;
    if batches.first() != Some(&genesis_batch) {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {} genesis differs from the requested initialization",
            manifest.world_id
        )));
    }
    resume_world(store, manifest.world_id).await
}

/// Rebuilds a session from the complete event log and independently checks the latest
/// snapshot. This is intentionally strict on process startup; snapshots are caches.
pub async fn resume_world<S: WorldStore + ?Sized>(
    store: &S,
    world_id: WorldId,
) -> Result<WorldSession, WorldRuntimeError> {
    let world = store.load_world(world_id).await?;
    let batches = store
        .load_event_batches(world_id, EventSequence::ZERO)
        .await?;
    let complete = replay(world.manifest.clone(), &batches)?;
    let snapshot = store.load_latest_snapshot(world_id).await?;
    let tail_start = batches
        .iter()
        .position(|batch| batch.sequence > snapshot.through_sequence)
        .unwrap_or(batches.len());
    let snapshot_outcome = replay_from_snapshot(&snapshot, &batches[tail_start..])?;
    if snapshot_outcome != complete {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {world_id} snapshot plus tail differs from genesis replay"
        )));
    }

    let state_hash = complete.state.state_hash().map_err(EngineError::from)?;
    if complete.state.manifest() != &world.manifest
        || complete.through_sequence != world.cursor.sequence
        || complete.state.tick() != world.cursor.tick
        || complete.state.status() != world.status
        || complete.last_event_hash != world.cursor.last_event_hash
        || state_hash != world.cursor.state_hash
    {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {world_id} cursor disagrees with replayed history"
        )));
    }

    Ok(WorldSession {
        world,
        state: complete.state,
    })
}

/// Plans and atomically persists exactly one simulation transition.
pub async fn advance_world<S: WorldStore + ?Sized>(
    store: &S,
    current: &WorldSession,
) -> Result<WorldSession, WorldRuntimeError> {
    if current.world.status != WorldStatus::Running
        || current.state.status() != WorldStatus::Running
    {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {} is not runnable",
            current.world.manifest.world_id
        )));
    }

    let sequence = current
        .world
        .cursor
        .sequence
        .checked_next()
        .map_err(EngineError::from)?;
    let events = current.state.plan_next_tick()?;
    let (next_state, batch) =
        current
            .state
            .commit(sequence, current.world.cursor.last_event_hash, events)?;
    let snapshot = Snapshot::new(next_state.clone(), batch.sequence, batch.batch_hash)?;
    let world = store
        .commit_transition(current.world.cursor, &batch, &snapshot)
        .await?;

    if world.cursor.sequence != batch.sequence
        || world.cursor.tick != batch.tick
        || world.cursor.last_event_hash != batch.batch_hash
        || world.cursor.state_hash != snapshot.state_hash
        || world.status != next_state.status()
    {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {} persistence result differs from committed transition",
            world.manifest.world_id
        )));
    }

    Ok(WorldSession {
        world,
        state: next_state,
    })
}
