//! Application use cases and infrastructure ports.

mod cognition;
mod cognition_worker;
mod memory;

pub use cognition::*;
pub use cognition_worker::*;
pub use memory::*;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sim_engine::{
    EngineError, EngineState, InitialMaterialInstance, InitialOrganism,
    PERSISTENT_PERCEPTION_RULESET_VERSION, ReplayOutcome, Snapshot, replay, replay_from_snapshot,
};
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CelestialState, Digest, DomainEvent, EntityId, EventBatch, EventSequence, PerceptionChannel,
    SimTick, WorldConfiguration, WorldId, WorldManifest, WorldStatus,
};

use std::collections::{BTreeMap, BTreeSet};

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
    pub latest_projector_heartbeat: Option<DateTime<Utc>>,
    pub latest_memory_worker_heartbeat: Option<DateTime<Utc>>,
    pub latest_cognition_worker_heartbeat: Option<DateTime<Utc>>,
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

    /// Enumerates durable worlds for observer projectors. This is read-only and does
    /// not grant an observer a way to initialize, advance, or modify a world.
    async fn list_world_ids(&self) -> Result<Vec<WorldId>, StoreError>;

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
        effects: &TransitionEffects,
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
    initialize_or_resume_world_internal(
        store,
        manifest,
        predecessor_world_id,
        None,
        initial_organisms,
        Vec::new(),
    )
    .await
}

/// Creates or resumes a world whose immutable causal scale and data provenance are
/// committed in the genesis batch.
///
/// Configuration is an explicit caller-owned input. This boundary neither discovers
/// datasets nor upgrades their scientific status; it only commits the exact validated
/// reference supplied by the caller.
pub async fn initialize_or_resume_configured_world<S: WorldStore + ?Sized>(
    store: &S,
    manifest: WorldManifest,
    predecessor_world_id: Option<WorldId>,
    configuration: WorldConfiguration,
    initial_organisms: Vec<InitialOrganism>,
) -> Result<WorldSession, WorldRuntimeError> {
    initialize_or_resume_world_internal(
        store,
        manifest,
        predecessor_world_id,
        Some(configuration),
        initial_organisms,
        Vec::new(),
    )
    .await
}

/// Creates or resumes one configured world whose organisms and material instances
/// are committed atomically in the same genesis batch.
pub async fn initialize_or_resume_configured_world_with_materials<S: WorldStore + ?Sized>(
    store: &S,
    manifest: WorldManifest,
    predecessor_world_id: Option<WorldId>,
    configuration: WorldConfiguration,
    initial_organisms: Vec<InitialOrganism>,
    initial_materials: Vec<InitialMaterialInstance>,
) -> Result<WorldSession, WorldRuntimeError> {
    initialize_or_resume_world_internal(
        store,
        manifest,
        predecessor_world_id,
        Some(configuration),
        initial_organisms,
        initial_materials,
    )
    .await
}

async fn initialize_or_resume_world_internal<S: WorldStore + ?Sized>(
    store: &S,
    manifest: WorldManifest,
    predecessor_world_id: Option<WorldId>,
    configuration: Option<WorldConfiguration>,
    initial_organisms: Vec<InitialOrganism>,
    initial_materials: Vec<InitialMaterialInstance>,
) -> Result<WorldSession, WorldRuntimeError> {
    let initial = EngineState::new(manifest.clone());
    let initial_hash = initial.state_hash().map_err(EngineError::from)?;
    let genesis_events = match configuration {
        Some(configuration) => initial.plan_configured_genesis_with_materials(
            configuration,
            initial_organisms,
            initial_materials,
        )?,
        None if initial_materials.is_empty() => initial.plan_genesis(initial_organisms)?,
        None => {
            return Err(WorldRuntimeError::Integrity(
                "initial material instances require a configured world".to_owned(),
            ));
        }
    };
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
            .commit_transition(
                stored.cursor,
                &genesis_batch,
                &genesis_snapshot,
                &TransitionEffects::default(),
            )
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

    finish_resume(world, complete)
}

/// Resumes from the newest cache checkpoint after anchoring it to its immutable event
/// batch, then replays only the bounded tail. Full operator verification continues to
/// use [`resume_world`] and independently replays from genesis.
pub async fn resume_world_from_snapshot<S: WorldStore + ?Sized>(
    store: &S,
    world_id: WorldId,
) -> Result<WorldSession, WorldRuntimeError> {
    let world = store.load_world(world_id).await?;
    let snapshot = store.load_latest_snapshot(world_id).await?;
    let after_sequence = if snapshot.through_sequence == EventSequence::ZERO {
        EventSequence::ZERO
    } else {
        EventSequence::new(snapshot.through_sequence.get() - 1)
    };
    let anchored_tail = store.load_event_batches(world_id, after_sequence).await?;
    let tail = if snapshot.through_sequence == EventSequence::ZERO {
        anchored_tail.as_slice()
    } else {
        let anchor = anchored_tail.first().ok_or_else(|| {
            WorldRuntimeError::Integrity(format!(
                "world {world_id} snapshot has no immutable event anchor"
            ))
        })?;
        if anchor.sequence != snapshot.through_sequence
            || anchor.batch_hash != snapshot.last_event_hash
            || anchor.post_state_hash != snapshot.state_hash
            || anchor.world_id != snapshot.world_id
        {
            return Err(WorldRuntimeError::Integrity(format!(
                "world {world_id} snapshot disagrees with its immutable event anchor"
            )));
        }
        &anchored_tail[1..]
    };
    let outcome = replay_from_snapshot(&snapshot, tail)?;
    finish_resume(world, outcome)
}

fn finish_resume(
    world: StoredWorld,
    outcome: ReplayOutcome,
) -> Result<WorldSession, WorldRuntimeError> {
    let world_id = world.manifest.world_id;
    let state_hash = outcome.state.state_hash().map_err(EngineError::from)?;
    if outcome.state.manifest() != &world.manifest
        || outcome.through_sequence != world.cursor.sequence
        || outcome.state.tick() != world.cursor.tick
        || outcome.state.status() != world.status
        || outcome.last_event_hash != world.cursor.last_event_hash
        || state_hash != world.cursor.state_hash
    {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {world_id} cursor disagrees with replayed history"
        )));
    }

    Ok(WorldSession {
        world,
        state: outcome.state,
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

    let events = current.state.plan_next_tick()?;
    advance_world_events(store, current, events).await
}

/// Commit the engine-selected world-total cognition request as a same-tick
/// transition. Returns `None` while another request is pending.
pub async fn schedule_world_cognition<S: WorldStore + ?Sized>(
    store: &S,
    current: &WorldSession,
) -> Result<Option<WorldSession>, WorldRuntimeError> {
    if current.world.status != WorldStatus::Running
        || current.state.status() != WorldStatus::Running
    {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {} is not runnable",
            current.world.manifest.world_id
        )));
    }
    let events = current.state.plan_scheduled_cognition_request()?;
    if events.is_empty() {
        return Ok(None);
    }
    advance_world_events(store, current, events).await.map(Some)
}

/// Advance one ruleset-three world using one already-evaluated, source-backed
/// celestial state. The source adapter lives outside this crate; replay uses only
/// the committed event and therefore never invokes it.
pub async fn advance_world_with_celestial<S: WorldStore + ?Sized>(
    store: &S,
    current: &WorldSession,
    celestial_state: CelestialState,
) -> Result<WorldSession, WorldRuntimeError> {
    if current.world.status != WorldStatus::Running
        || current.state.status() != WorldStatus::Running
    {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {} is not runnable",
            current.world.manifest.world_id
        )));
    }
    let events = current
        .state
        .plan_next_tick_with_celestial(celestial_state)?;
    advance_world_events(store, current, events).await
}

/// Advance one cognition-enabled world after atomically freezing every external
/// result due in this transition. A crash after latching returns the same inputs on
/// retry, while replay consumes only the committed events.
pub async fn advance_world_with_celestial_and_cognition<S>(
    store: &S,
    current: &WorldSession,
    celestial_state: CelestialState,
) -> Result<WorldSession, WorldRuntimeError>
where
    S: WorldStore + CognitionJobStore + ?Sized,
{
    if current.world.status != WorldStatus::Running
        || current.state.status() != WorldStatus::Running
    {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {} is not runnable",
            current.world.manifest.world_id
        )));
    }
    let target_sequence = current
        .world
        .cursor
        .sequence
        .checked_next()
        .map_err(EngineError::from)?;
    let target_tick = current
        .world
        .cursor
        .tick
        .checked_next()
        .map_err(EngineError::from)?;
    let inputs = store
        .latch_due_cognition_inputs(
            current.world.manifest.world_id,
            target_sequence,
            target_tick,
        )
        .await?;
    let events = current
        .state
        .plan_next_tick_with_celestial_and_cognition(celestial_state, &inputs)?;
    advance_world_events(store, current, events).await
}

async fn advance_world_events<S: WorldStore + ?Sized>(
    store: &S,
    current: &WorldSession,
    events: Vec<world_domain::DomainEvent>,
) -> Result<WorldSession, WorldRuntimeError> {
    let sequence = current
        .world
        .cursor
        .sequence
        .checked_next()
        .map_err(EngineError::from)?;
    let (next_state, batch) =
        current
            .state
            .commit(sequence, current.world.cursor.last_event_hash, events)?;
    let snapshot = Snapshot::new(next_state.clone(), batch.sequence, batch.batch_hash)?;
    let effects = derive_transition_effects(&current.state, &batch)?;
    let world = store
        .commit_transition(current.world.cursor, &batch, &snapshot, &effects)
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

#[derive(Serialize)]
struct RetainedDirectObservation<'a> {
    subject_id: Option<EntityId>,
    channel: PerceptionChannel,
    property_code: &'a str,
    quantized_value: i32,
    uncertainty: u16,
}

/// Select a bounded, deterministic subset of canonical experiences for the
/// external subjective-memory adapter. This is an application-side projection:
/// it never affects the batch, state hash, or the next action.
fn derive_transition_effects(
    prior_state: &EngineState,
    batch: &EventBatch,
) -> Result<TransitionEffects, WorldRuntimeError> {
    if prior_state.manifest().ruleset_version < PERSISTENT_PERCEPTION_RULESET_VERSION {
        return Ok(TransitionEffects::default());
    }
    let mut effects = TransitionEffects::default();
    let mut next_ordinal = BTreeMap::<EntityId, u32>::new();
    let mut selected = BTreeSet::<(EntityId, Option<EntityId>, PerceptionChannel, String)>::new();
    for record in &batch.events {
        let DomainEvent::OrganismPerceived {
            organism_id,
            perception,
        } = &record.event
        else {
            continue;
        };
        let organism = prior_state
            .organisms()
            .find(|organism| organism.organism_id() == *organism_id)
            .ok_or_else(|| {
                WorldRuntimeError::Integrity(format!(
                    "perception event references unknown organism {organism_id}"
                ))
            })?;
        for reading in &perception.readings {
            let key = (
                *organism_id,
                perception.subject_id,
                reading.channel,
                reading.property_code.clone(),
            );
            if organism.has_perception_memory_at(
                perception.subject_id,
                reading.channel,
                &reading.property_code,
            ) || !selected.insert(key)
            {
                continue;
            }
            let content = serde_json::to_string(&RetainedDirectObservation {
                subject_id: perception.subject_id,
                channel: reading.channel,
                property_code: &reading.property_code,
                quantized_value: reading.quantized_value,
                uncertainty: reading.uncertainty,
            })
            .map_err(|error| WorldRuntimeError::Integrity(error.to_string()))?;
            let ordinal = next_ordinal.entry(*organism_id).or_default();
            effects.memory_retains.push(
                MemoryRetain::new(
                    batch.world_id,
                    *organism_id,
                    batch.sequence,
                    batch.tick,
                    *ordinal,
                    content,
                    "canonical-direct-perception-v1",
                )
                .map_err(|error| WorldRuntimeError::Integrity(error.to_string()))?,
            );
            *ordinal = ordinal.checked_add(1).ok_or_else(|| {
                WorldRuntimeError::Integrity("per-agent memory ordinal overflowed".to_owned())
            })?;
        }
    }
    Ok(effects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_engine::{InitialOrganism, PERSISTENT_PERCEPTION_RULESET_VERSION};
    use uuid::Uuid;
    use world_domain::{
        BirthCategory, Digest, OrganismRole, PerceptionChannel, PropertyReading,
        SituatedPerception, SpeciesIdentity, WorldSeed,
    };

    fn organism(world_id: WorldId) -> InitialOrganism {
        InitialOrganism {
            organism_id: EntityId::deterministic(world_id, b"application-memory-organism"),
            species: SpeciesIdentity::new(
                "gbif",
                "2436436",
                "Homo sapiens",
                "https://www.gbif.org/species/2436436",
            )
            .expect("species"),
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("unspecified").expect("category"),
            initial_age_ticks: 0,
            location_id: None,
            embodied_patch: None,
            metabolic_rate: None,
            adult_body_mass: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
        }
    }

    #[test]
    fn first_direct_perception_is_retained_once_per_memory_address() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0xA77));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(17),
            PERSISTENT_PERCEPTION_RULESET_VERSION,
        );
        let initial = EngineState::new(manifest);
        let organism = organism(world_id);
        let organism_id = organism.organism_id;
        let (running, genesis) = initial
            .commit(
                EventSequence::new(1),
                Digest::ZERO,
                initial.plan_genesis(vec![organism]).expect("genesis plan"),
            )
            .expect("genesis");
        let perception = SituatedPerception {
            subject_id: None,
            readings: vec![PropertyReading {
                channel: PerceptionChannel::Touch,
                property_code: "temperature".to_owned(),
                quantized_value: 7,
                uncertainty: 0,
            }],
        };
        let (after_first, first_batch) = running
            .commit(
                EventSequence::new(2),
                genesis.batch_hash,
                running
                    .plan_perception(organism_id, perception.clone())
                    .expect("perception plan"),
            )
            .expect("perception batch");
        let first_effects =
            derive_transition_effects(&running, &first_batch).expect("first effects");
        assert_eq!(first_effects.memory_retains.len(), 1);
        assert_eq!(first_effects.memory_retains[0].agent_id, organism_id);

        let (_, repeat_batch) = after_first
            .commit(
                EventSequence::new(3),
                first_batch.batch_hash,
                after_first
                    .plan_perception(organism_id, perception)
                    .expect("repeat perception plan"),
            )
            .expect("repeat perception batch");
        assert!(
            derive_transition_effects(&after_first, &repeat_batch)
                .expect("repeat effects")
                .memory_retains
                .is_empty()
        );
    }
}
