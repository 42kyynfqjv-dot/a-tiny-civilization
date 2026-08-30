//! Application use cases and infrastructure ports.

mod cognition;
mod cognition_worker;
mod memory;
mod research;
mod research_billing;
mod research_nci60_qualification;
mod research_novelty;
mod research_patient_derived_qualification;
mod research_scheduler;
mod research_tcga_target_context;
mod research_tissue_refinement;
mod research_tissue_worker;
mod research_virtual_lab;
mod research_worker;

pub use cognition::*;
pub use cognition_worker::*;
pub use memory::*;
pub use research::*;
pub use research_billing::*;
pub use research_nci60_qualification::*;
pub use research_novelty::*;
pub use research_patient_derived_qualification::*;
pub use research_scheduler::*;
pub use research_tcga_target_context::*;
pub use research_tissue_refinement::*;
pub use research_tissue_worker::*;
pub use research_virtual_lab::*;
pub use research_worker::*;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sim_engine::{
    EngineError, EngineState, InitialMaterialInstance, InitialOrganism,
    PERSISTENT_PERCEPTION_RULESET_VERSION, ReplayOutcome, Snapshot,
    ordinary_world_hardening_active, replay, replay_from_snapshot,
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

    /// Loads only the immutable first batch. Existing-world initialization
    /// must not materialize an unbounded history merely to compare genesis.
    async fn load_genesis_event_batch(
        &self,
        world_id: WorldId,
    ) -> Result<Option<EventBatch>, StoreError>;

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

/// Fully constructed genesis artifacts before any persistence side effect.
///
/// Operators use this to prove that a canonical input bundle can produce a
/// self-consistent, replayable genesis even when a database is unavailable.
/// Persistence still commits these exact artifacts through [`WorldStore`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstructedGenesis {
    pub initial_state_hash: Digest,
    pub state: EngineState,
    pub batch: EventBatch,
    pub snapshot: Snapshot,
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

/// Construct the exact configured genesis batch and snapshot without writing
/// external state. This is the database-free counterpart to
/// [`initialize_or_resume_configured_world_with_materials`].
pub fn construct_configured_genesis_with_materials(
    manifest: WorldManifest,
    configuration: WorldConfiguration,
    initial_organisms: Vec<InitialOrganism>,
    initial_materials: Vec<InitialMaterialInstance>,
) -> Result<ConstructedGenesis, WorldRuntimeError> {
    construct_genesis(
        manifest,
        Some(configuration),
        initial_organisms,
        initial_materials,
    )
}

fn construct_genesis(
    manifest: WorldManifest,
    configuration: Option<WorldConfiguration>,
    initial_organisms: Vec<InitialOrganism>,
    initial_materials: Vec<InitialMaterialInstance>,
) -> Result<ConstructedGenesis, WorldRuntimeError> {
    let initial = EngineState::new(manifest);
    let initial_state_hash = initial.state_hash().map_err(EngineError::from)?;
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
    let (state, batch) = initial.commit(EventSequence::new(1), Digest::ZERO, genesis_events)?;
    let snapshot = Snapshot::new(state.clone(), batch.sequence, batch.batch_hash)?;
    Ok(ConstructedGenesis {
        initial_state_hash,
        state,
        batch,
        snapshot,
    })
}

async fn initialize_or_resume_world_internal<S: WorldStore + ?Sized>(
    store: &S,
    manifest: WorldManifest,
    predecessor_world_id: Option<WorldId>,
    configuration: Option<WorldConfiguration>,
    initial_organisms: Vec<InitialOrganism>,
    initial_materials: Vec<InitialMaterialInstance>,
) -> Result<WorldSession, WorldRuntimeError> {
    let genesis = construct_genesis(
        manifest.clone(),
        configuration,
        initial_organisms,
        initial_materials,
    )?;
    let initial_hash = genesis.initial_state_hash;
    let running = genesis.state;
    let genesis_batch = genesis.batch;
    let genesis_snapshot = genesis.snapshot;

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

    let stored_genesis = store.load_genesis_event_batch(manifest.world_id).await?;
    if stored_genesis.as_ref() != Some(&genesis_batch) {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {} genesis differs from the requested initialization",
            manifest.world_id
        )));
    }
    resume_world_from_snapshot(store, manifest.world_id).await
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

/// Atomically retire one still-populated world for a disclosed successor. This is
/// an explicit operator lifecycle use case, never part of ordinary simulation
/// planning and never presented as biological extinction.
pub async fn retire_world_for_successor<S: WorldStore + ?Sized>(
    store: &S,
    current: &WorldSession,
    successor_world_id: WorldId,
) -> Result<WorldSession, WorldRuntimeError> {
    if current.world.status != WorldStatus::Running
        || current.state.status() != WorldStatus::Running
    {
        return Err(WorldRuntimeError::Integrity(format!(
            "world {} is not running and cannot be retired",
            current.world.manifest.world_id
        )));
    }
    let events = current
        .state
        .plan_successor_retirement(successor_world_id)?;
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

const MAX_EPISODIC_MEMORY_RETAINS_PER_TRANSITION: usize = 8;
const MAX_EPISODIC_MEMORY_RETAINS_PER_AGENT: usize = 1;
const CHANGED_PERCEPTION_SAMPLE_CADENCE_TICKS: u64 = 64;
const STABLE_PERCEPTION_REFRESH_CADENCE_TICKS: u64 = 1_152;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum EpisodicSamplingReason {
    NewAddress,
    AcousticChange,
    MeaningfulChange,
    PeriodicRefresh,
}

impl EpisodicSamplingReason {
    const fn priority(self) -> u8 {
        match self {
            Self::NewAddress => 0,
            Self::AcousticChange => 1,
            Self::MeaningfulChange => 2,
            Self::PeriodicRefresh => 3,
        }
    }
}

#[derive(Serialize)]
struct RetainedDirectObservationEpisode<'a> {
    subject_id: Option<EntityId>,
    channel: PerceptionChannel,
    property_code: &'a str,
    quantized_value: i32,
    uncertainty: u16,
    prior_quantized_value: Option<i32>,
    prior_uncertainty: Option<u16>,
    prior_observed_at: Option<SimTick>,
    sampling_reason: EpisodicSamplingReason,
}

#[derive(Serialize)]
struct EpisodicMemoryRankMaterial<'a> {
    world_id: WorldId,
    tick: SimTick,
    organism_id: EntityId,
    subject_id: Option<EntityId>,
    channel: PerceptionChannel,
    property_code: &'a str,
}

struct EpisodicMemoryCandidate {
    agent_id: EntityId,
    priority: u8,
    rank: Digest,
    content: String,
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
    if prior_state.manifest().experiment.is_none()
        && ordinary_world_hardening_active(prior_state.manifest().ruleset_version, batch.tick)
    {
        return derive_bounded_episodic_memory_effects(prior_state, batch);
    }
    derive_legacy_first_address_memory_effects(prior_state, batch)
}

fn derive_legacy_first_address_memory_effects(
    prior_state: &EngineState,
    batch: &EventBatch,
) -> Result<TransitionEffects, WorldRuntimeError> {
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

fn derive_bounded_episodic_memory_effects(
    prior_state: &EngineState,
    batch: &EventBatch,
) -> Result<TransitionEffects, WorldRuntimeError> {
    let mut candidates = Vec::new();
    let mut selected_addresses =
        BTreeSet::<(EntityId, Option<EntityId>, PerceptionChannel, String)>::new();
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
        if organism.role() != world_domain::OrganismRole::Person {
            continue;
        }
        for reading in &perception.readings {
            let address = (
                *organism_id,
                perception.subject_id,
                reading.channel,
                reading.property_code.clone(),
            );
            if !selected_addresses.insert(address) {
                continue;
            }
            let previous = organism.perception_memory_reading_at(
                perception.subject_id,
                reading.channel,
                &reading.property_code,
            );
            let address_slot = episodic_address_slot(
                batch.world_id,
                *organism_id,
                perception.subject_id,
                reading.channel,
                &reading.property_code,
            )?;
            let sampling_reason = match previous {
                None => Some(EpisodicSamplingReason::NewAddress),
                Some((prior_value, _, _))
                    if reading.channel == PerceptionChannel::Sound
                        && reading.property_code == "signal_amplitude"
                        && prior_value != reading.quantized_value =>
                {
                    Some(EpisodicSamplingReason::AcousticChange)
                }
                Some((prior_value, prior_uncertainty, _))
                    if is_meaningful_perception_change(
                        prior_value,
                        prior_uncertainty,
                        reading.quantized_value,
                        reading.uncertainty,
                    ) && cadence_matches(
                        batch.tick,
                        address_slot,
                        CHANGED_PERCEPTION_SAMPLE_CADENCE_TICKS,
                    ) =>
                {
                    Some(EpisodicSamplingReason::MeaningfulChange)
                }
                Some(_)
                    if cadence_matches(
                        batch.tick,
                        address_slot,
                        STABLE_PERCEPTION_REFRESH_CADENCE_TICKS,
                    ) =>
                {
                    Some(EpisodicSamplingReason::PeriodicRefresh)
                }
                Some(_) => None,
            };
            let Some(sampling_reason) = sampling_reason else {
                continue;
            };
            let content = serde_json::to_string(&RetainedDirectObservationEpisode {
                subject_id: perception.subject_id,
                channel: reading.channel,
                property_code: &reading.property_code,
                quantized_value: reading.quantized_value,
                uncertainty: reading.uncertainty,
                prior_quantized_value: previous.map(|prior| prior.0),
                prior_uncertainty: previous.map(|prior| prior.1),
                prior_observed_at: previous.map(|prior| prior.2),
                sampling_reason,
            })
            .map_err(|error| WorldRuntimeError::Integrity(error.to_string()))?;
            let rank = Digest::canonical(&EpisodicMemoryRankMaterial {
                world_id: batch.world_id,
                tick: batch.tick,
                organism_id: *organism_id,
                subject_id: perception.subject_id,
                channel: reading.channel,
                property_code: &reading.property_code,
            })
            .map_err(|error| WorldRuntimeError::Integrity(error.to_string()))?;
            candidates.push(EpisodicMemoryCandidate {
                agent_id: *organism_id,
                priority: sampling_reason.priority(),
                rank,
                content,
            });
        }
    }
    candidates.sort_by_key(|candidate| (candidate.priority, candidate.rank, candidate.agent_id));

    let mut effects = TransitionEffects::default();
    let mut retained_per_agent = BTreeMap::<EntityId, usize>::new();
    let mut next_ordinal = BTreeMap::<EntityId, u32>::new();
    for candidate in candidates {
        if effects.memory_retains.len() >= MAX_EPISODIC_MEMORY_RETAINS_PER_TRANSITION {
            break;
        }
        let retained = retained_per_agent.entry(candidate.agent_id).or_default();
        if *retained >= MAX_EPISODIC_MEMORY_RETAINS_PER_AGENT {
            continue;
        }
        let ordinal = next_ordinal.entry(candidate.agent_id).or_default();
        effects.memory_retains.push(
            MemoryRetain::new(
                batch.world_id,
                candidate.agent_id,
                batch.sequence,
                batch.tick,
                *ordinal,
                candidate.content,
                "canonical-direct-perception-episode-v2",
            )
            .map_err(|error| WorldRuntimeError::Integrity(error.to_string()))?,
        );
        *ordinal = ordinal.checked_add(1).ok_or_else(|| {
            WorldRuntimeError::Integrity("per-agent memory ordinal overflowed".to_owned())
        })?;
        *retained += 1;
    }
    Ok(effects)
}

fn episodic_address_slot(
    world_id: WorldId,
    organism_id: EntityId,
    subject_id: Option<EntityId>,
    channel: PerceptionChannel,
    property_code: &str,
) -> Result<u64, WorldRuntimeError> {
    #[derive(Serialize)]
    struct Address<'a> {
        world_id: WorldId,
        organism_id: EntityId,
        subject_id: Option<EntityId>,
        channel: PerceptionChannel,
        property_code: &'a str,
    }
    let digest = Digest::canonical(&Address {
        world_id,
        organism_id,
        subject_id,
        channel,
        property_code,
    })
    .map_err(|error| WorldRuntimeError::Integrity(error.to_string()))?;
    Ok(u64::from_be_bytes(
        digest.as_bytes()[..8]
            .try_into()
            .expect("SHA-256 has at least eight bytes"),
    ))
}

const fn cadence_matches(tick: SimTick, address_slot: u64, cadence: u64) -> bool {
    tick.get() % cadence == address_slot % cadence
}

fn is_meaningful_perception_change(
    prior_value: i32,
    prior_uncertainty: u16,
    value: i32,
    uncertainty: u16,
) -> bool {
    let delta = i64::from(value).abs_diff(i64::from(prior_value));
    let magnitude_threshold = i64::from(prior_value).unsigned_abs().div_ceil(8).max(8);
    let uncertainty_threshold = u64::from(prior_uncertainty.max(uncertainty)).saturating_mul(2);
    delta >= magnitude_threshold.max(uncertainty_threshold)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_engine::{InitialOrganism, PERSISTENT_PERCEPTION_RULESET_VERSION};
    use uuid::Uuid;
    use world_domain::{
        BirthCategory, CapacityExhaustionPolicy, Digest, EarthResolutionLevels, FullEarthGrid,
        OrganismRole, PartitionedExecution, PerceptionChannel, PersonRepresentation,
        PropertyReading, ProvisionalWorldCompositionReference, S2Projection, SchedulerKind,
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

    #[test]
    fn episodic_memory_retains_changed_calls_but_not_unchanged_repeats() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0xA79));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(19),
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
        let signal = |value| SituatedPerception {
            subject_id: Some(organism_id),
            readings: vec![PropertyReading {
                channel: PerceptionChannel::Sound,
                property_code: "signal_amplitude".to_owned(),
                quantized_value: value,
                uncertainty: 0,
            }],
        };
        assert_ne!(
            episodic_address_slot(
                world_id,
                organism_id,
                Some(organism_id),
                PerceptionChannel::Sound,
                "signal_amplitude",
            )
            .expect("address slot")
                % STABLE_PERCEPTION_REFRESH_CADENCE_TICKS,
            0,
            "fixture must not land on the periodic-refresh slot"
        );
        let (after_first, first_batch) = running
            .commit(
                EventSequence::new(2),
                genesis.batch_hash,
                running
                    .plan_perception(organism_id, signal(7))
                    .expect("first signal"),
            )
            .expect("first signal batch");
        let first =
            derive_bounded_episodic_memory_effects(&running, &first_batch).expect("first episode");
        assert_eq!(first.memory_retains.len(), 1);
        assert_eq!(
            first.memory_retains[0].context,
            "canonical-direct-perception-episode-v2"
        );

        let (_, repeat_batch) = after_first
            .commit(
                EventSequence::new(3),
                first_batch.batch_hash,
                after_first
                    .plan_perception(organism_id, signal(7))
                    .expect("repeat signal"),
            )
            .expect("repeat signal batch");
        assert!(
            derive_bounded_episodic_memory_effects(&after_first, &repeat_batch)
                .expect("repeat episode")
                .memory_retains
                .is_empty()
        );

        let (_, changed_batch) = after_first
            .commit(
                EventSequence::new(3),
                first_batch.batch_hash,
                after_first
                    .plan_perception(organism_id, signal(19))
                    .expect("changed signal"),
            )
            .expect("changed signal batch");
        let changed = derive_bounded_episodic_memory_effects(&after_first, &changed_batch)
            .expect("changed episode");
        assert_eq!(changed.memory_retains.len(), 1);
        assert!(
            changed.memory_retains[0]
                .content
                .contains("acoustic_change")
        );
        assert!(
            changed.memory_retains[0]
                .content
                .contains("prior_quantized_value\":7")
        );
    }

    #[test]
    fn episodic_memory_has_population_independent_transition_cap() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0xA7A));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(20),
            PERSISTENT_PERCEPTION_RULESET_VERSION,
        );
        let initial = EngineState::new(manifest);
        let organisms = (0..32_u128)
            .map(|ordinal| {
                let mut person = organism(world_id);
                person.organism_id = EntityId::from_uuid(Uuid::from_u128(0xA7A_0000 + ordinal));
                person
            })
            .collect::<Vec<_>>();
        let (running, genesis) = initial
            .commit(
                EventSequence::new(1),
                Digest::ZERO,
                initial.plan_genesis(organisms).expect("genesis plan"),
            )
            .expect("genesis");
        let mut perception_events = Vec::new();
        for person in running.organisms() {
            perception_events.extend(
                running
                    .plan_perception(
                        person.organism_id(),
                        SituatedPerception {
                            subject_id: None,
                            readings: vec![PropertyReading {
                                channel: PerceptionChannel::Touch,
                                property_code: "temperature".to_owned(),
                                quantized_value: 7,
                                uncertainty: 0,
                            }],
                        },
                    )
                    .expect("direct perception"),
            );
        }
        let (_, batch) = running
            .commit(EventSequence::new(2), genesis.batch_hash, perception_events)
            .expect("perception batch");
        let first =
            derive_bounded_episodic_memory_effects(&running, &batch).expect("bounded episodes");
        let second = derive_bounded_episodic_memory_effects(&running, &batch)
            .expect("deterministic bounded episodes");
        assert_eq!(first, second);
        assert_eq!(
            first.memory_retains.len(),
            MAX_EPISODIC_MEMORY_RETAINS_PER_TRANSITION
        );
        assert_eq!(
            first
                .memory_retains
                .iter()
                .map(|memory| memory.agent_id)
                .collect::<BTreeSet<_>>()
                .len(),
            first.memory_retains.len(),
            "the per-agent cap prevents one life from monopolizing a transition"
        );
    }

    #[test]
    fn database_free_genesis_matches_event_replay_and_snapshot() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0xA78));
        let manifest = WorldManifest::new(world_id, WorldSeed::new(18), 1);
        let patch = "0000000000004000".parse().expect("L23 patch");
        let mut founder = organism(world_id);
        founder.embodied_patch = Some(patch);
        let configuration = WorldConfiguration::new_provisional_full_earth(
            300,
            FullEarthGrid {
                physics_crs_epsg: 4978,
                catalog_crs_epsg: 4979,
                vertical_crs_epsg: 3855,
                s2_definition_url: "https://s2geometry.io/devguide/s2cell_hierarchy.html"
                    .to_owned(),
                s2_library_revision: "0123456789abcdef".to_owned(),
                s2_definition_hash: Digest::sha256(b"S2 definition"),
                s2_projection: S2Projection::Quadratic,
                levels: EarthResolutionLevels {
                    planetary_aggregate: 10,
                    regional_ecology: 14,
                    active_landscape: 18,
                    embodied_patch: 23,
                },
                refinement_policy_version: 1,
            },
            ProvisionalWorldCompositionReference::new(
                1,
                "application-genesis-proof",
                "0.1.0",
                Digest::sha256(b"composition"),
            )
            .expect("composition reference"),
            PartitionedExecution {
                scheduler_schema_version: 1,
                scheduler: SchedulerKind::DeterministicEventQueue,
                partition_s2_level: 10,
                person_representation: PersonRepresentation::DurableIndividuals,
                capacity_exhaustion: CapacityExhaustionPolicy::PauseAtCommittedBoundary,
                max_events_per_partition_transition: 10_000,
            },
        )
        .expect("configuration");
        let genesis = construct_configured_genesis_with_materials(
            manifest.clone(),
            configuration,
            vec![founder],
            Vec::new(),
        )
        .expect("construct genesis");
        let complete =
            replay(manifest, std::slice::from_ref(&genesis.batch)).expect("event replay");
        let from_snapshot = replay_from_snapshot(&genesis.snapshot, &[]).expect("snapshot replay");
        assert_eq!(complete, from_snapshot);
        assert_eq!(complete.state, genesis.state);
        assert_eq!(genesis.batch.post_state_hash, genesis.snapshot.state_hash);
    }
}
