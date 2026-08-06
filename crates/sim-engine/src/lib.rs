//! Pure deterministic planning, state transitions, snapshots, and replay.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{
    BirthCategory, CanonicalHashError, DeathCause, Digest, DomainEvent, EVENT_SCHEMA_VERSION,
    EntityId, EventBatch, EventBatchError, EventSequence, LEGACY_EVENT_SCHEMA_VERSION,
    OrganismRole, SequenceOverflow, SimTick, SpeciesIdentity, SpeciesIdentityError, TimeOverflow,
    WorldConfiguration, WorldConfigurationError, WorldId, WorldManifest, WorldStatus,
};

/// Version pinned to each world so old histories are never silently reinterpreted.
pub const RULESET_VERSION: u32 = 1;
pub const LEGACY_SNAPSHOT_SCHEMA_VERSION: u16 = 1;
pub const SNAPSHOT_SCHEMA_VERSION: u16 = 2;
const LEGACY_STATE_HASH_SCHEMA_VERSION: u16 = 1;
const STATE_HASH_SCHEMA_VERSION: u16 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InitialOrganism {
    pub organism_id: EntityId,
    pub species: SpeciesIdentity,
    pub role: OrganismRole,
    pub birth_category: BirthCategory,
    pub initial_age_ticks: u64,
    pub location_id: Option<EntityId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeathRecord {
    pub tick: SimTick,
    pub cause: DeathCause,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrganismState {
    organism_id: EntityId,
    species: SpeciesIdentity,
    role: OrganismRole,
    birth_category: BirthCategory,
    parent_ids: Vec<EntityId>,
    initialized_at: SimTick,
    born_at: Option<SimTick>,
    initial_age_ticks: u64,
    location_id: Option<EntityId>,
    death: Option<DeathRecord>,
}

impl OrganismState {
    #[must_use]
    pub const fn organism_id(&self) -> EntityId {
        self.organism_id
    }

    #[must_use]
    pub const fn role(&self) -> OrganismRole {
        self.role
    }

    #[must_use]
    pub const fn is_alive(&self) -> bool {
        self.death.is_none()
    }

    #[must_use]
    pub const fn death(&self) -> Option<&DeathRecord> {
        self.death.as_ref()
    }

    #[must_use]
    pub const fn species(&self) -> &SpeciesIdentity {
        &self.species
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EngineState {
    manifest: WorldManifest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    configuration: Option<WorldConfiguration>,
    status: WorldStatus,
    tick: SimTick,
    organisms: BTreeMap<EntityId, OrganismState>,
}

impl EngineState {
    #[must_use]
    pub fn new(manifest: WorldManifest) -> Self {
        Self {
            manifest,
            configuration: None,
            status: WorldStatus::Initializing,
            tick: SimTick::ZERO,
            organisms: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn manifest(&self) -> &WorldManifest {
        &self.manifest
    }

    #[must_use]
    pub const fn configuration(&self) -> Option<&WorldConfiguration> {
        self.configuration.as_ref()
    }

    #[must_use]
    pub const fn world_id(&self) -> WorldId {
        self.manifest.world_id
    }

    #[must_use]
    pub const fn tick(&self) -> SimTick {
        self.tick
    }

    #[must_use]
    pub const fn ruleset_version(&self) -> u32 {
        self.manifest.ruleset_version
    }

    #[must_use]
    pub const fn status(&self) -> WorldStatus {
        self.status
    }

    #[must_use]
    pub fn organisms(&self) -> impl ExactSizeIterator<Item = &OrganismState> {
        self.organisms.values()
    }

    #[must_use]
    pub fn living_people(&self) -> usize {
        self.organisms
            .values()
            .filter(|organism| organism.role == OrganismRole::Person && organism.is_alive())
            .count()
    }

    pub fn plan_genesis(
        &self,
        initial_organisms: Vec<InitialOrganism>,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        self.plan_genesis_internal(None, initial_organisms)
    }

    pub fn plan_configured_genesis(
        &self,
        configuration: WorldConfiguration,
        initial_organisms: Vec<InitialOrganism>,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        configuration.validate()?;
        self.plan_genesis_internal(Some(configuration), initial_organisms)
    }

    fn plan_genesis_internal(
        &self,
        configuration: Option<WorldConfiguration>,
        mut initial_organisms: Vec<InitialOrganism>,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_status(WorldStatus::Initializing)?;
        if self.manifest.ruleset_version == 0 {
            return Err(EngineError::ZeroRulesetVersion);
        }

        initial_organisms.sort_by_key(|organism| organism.organism_id);
        if initial_organisms
            .windows(2)
            .any(|pair| pair[0].organism_id == pair[1].organism_id)
        {
            return Err(EngineError::DuplicateInitialOrganism);
        }

        let mut events = Vec::with_capacity(
            initial_organisms
                .len()
                .saturating_add(1 + usize::from(configuration.is_some())),
        );
        events.push(DomainEvent::WorldStarted {
            manifest: self.manifest.clone(),
        });
        if let Some(configuration) = configuration {
            events.push(DomainEvent::WorldConfigured { configuration });
        }
        events.extend(initial_organisms.into_iter().map(|organism| {
            DomainEvent::OrganismInitialized {
                organism_id: organism.organism_id,
                species: organism.species,
                role: organism.role,
                birth_category: organism.birth_category,
                initial_age_ticks: organism.initial_age_ticks,
                location_id: organism.location_id,
            }
        }));
        Ok(events)
    }

    pub fn plan_next_tick(&self) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_status(WorldStatus::Running)?;
        let next = self.tick.checked_next()?;
        let mut events = vec![DomainEvent::TickAdvanced {
            from: self.tick,
            to: next,
        }];
        if self.living_people() == 0 {
            events.push(DomainEvent::WorldExtinct);
            events.push(DomainEvent::WorldArchived);
        }
        Ok(events)
    }

    pub fn plan_death(
        &self,
        organism_id: EntityId,
        cause: DeathCause,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_status(WorldStatus::Running)?;
        let organism = self
            .organisms
            .get(&organism_id)
            .ok_or(EngineError::UnknownOrganism(organism_id))?;
        if !organism.is_alive() {
            return Err(EngineError::OrganismAlreadyDead(organism_id));
        }

        let mut events = vec![DomainEvent::OrganismDied { organism_id, cause }];
        let mut preview = self.clone();
        preview.apply_events(&events)?;
        if preview.living_people() == 0 {
            events.push(DomainEvent::WorldExtinct);
            events.push(DomainEvent::WorldArchived);
        }
        Ok(events)
    }

    pub fn commit(
        &self,
        sequence: EventSequence,
        previous_hash: Digest,
        events: Vec<DomainEvent>,
    ) -> Result<(Self, EventBatch), EngineError> {
        let mut next = self.clone();
        next.apply_events(&events)?;
        next.validate()?;
        if let Some(configuration) = &next.configuration {
            let actual = u64::try_from(events.len()).map_err(|_| EngineError::TooManyEvents)?;
            let maximum = u64::from(configuration.max_events_per_transition);
            if actual > maximum {
                return Err(EngineError::EventBudgetExceeded { actual, maximum });
            }
        }
        let state_hash = next.state_hash()?;
        let event_schema_version = if next.configuration.is_some() {
            EVENT_SCHEMA_VERSION
        } else {
            LEGACY_EVENT_SCHEMA_VERSION
        };
        let batch = EventBatch::new(
            event_schema_version,
            self.world_id(),
            sequence,
            next.tick,
            self.ruleset_version(),
            previous_hash,
            events,
            state_hash,
        )?;
        Ok((next, batch))
    }

    pub fn state_hash(&self) -> Result<Digest, CanonicalHashError> {
        let state_hash_schema_version = if self.configuration.is_some() {
            STATE_HASH_SCHEMA_VERSION
        } else {
            LEGACY_STATE_HASH_SCHEMA_VERSION
        };
        Digest::canonical(&StateHashMaterial {
            state_hash_schema_version,
            manifest: &self.manifest,
            configuration: self.configuration.as_ref(),
            status: self.status,
            tick: self.tick,
            organisms: self.organisms.values().collect(),
        })
    }

    fn apply_events(&mut self, events: &[DomainEvent]) -> Result<(), EngineError> {
        for event in events {
            self.apply_event(event)?;
        }
        Ok(())
    }

    fn apply_event(&mut self, event: &DomainEvent) -> Result<(), EngineError> {
        match event {
            DomainEvent::WorldStarted { manifest } => {
                self.require_status(WorldStatus::Initializing)?;
                if manifest != &self.manifest {
                    return Err(EngineError::ManifestMismatch);
                }
                if self.tick != SimTick::ZERO || !self.organisms.is_empty() {
                    return Err(EngineError::InvalidGenesisState);
                }
                self.status = WorldStatus::Running;
            }
            DomainEvent::WorldConfigured { configuration } => {
                self.require_status(WorldStatus::Running)?;
                if self.configuration.is_some() {
                    return Err(EngineError::WorldAlreadyConfigured);
                }
                if self.tick != SimTick::ZERO || !self.organisms.is_empty() {
                    return Err(EngineError::ConfigurationAfterOrganisms);
                }
                configuration.validate()?;
                self.configuration = Some(configuration.clone());
            }
            DomainEvent::OrganismInitialized {
                organism_id,
                species,
                role,
                birth_category,
                initial_age_ticks,
                location_id,
            } => {
                self.require_status(WorldStatus::Running)?;
                species.validate()?;
                self.insert_organism(OrganismState {
                    organism_id: *organism_id,
                    species: species.clone(),
                    role: *role,
                    birth_category: birth_category.clone(),
                    parent_ids: Vec::new(),
                    initialized_at: self.tick,
                    born_at: None,
                    initial_age_ticks: *initial_age_ticks,
                    location_id: *location_id,
                    death: None,
                })?;
            }
            DomainEvent::TickAdvanced { from, to } => {
                self.require_status(WorldStatus::Running)?;
                let expected = self.tick.checked_next()?;
                if from != &self.tick || to != &expected {
                    return Err(EngineError::InvalidTickTransition {
                        current: self.tick,
                        from: *from,
                        to: *to,
                    });
                }
                self.tick = *to;
            }
            DomainEvent::OrganismBorn {
                organism_id,
                species,
                role,
                birth_category,
                parent_ids,
                location_id,
            } => {
                self.require_status(WorldStatus::Running)?;
                species.validate()?;
                if parent_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
                    return Err(EngineError::NonCanonicalParentOrder);
                }
                if let Some(missing) = parent_ids
                    .iter()
                    .find(|parent_id| !self.organisms.contains_key(parent_id))
                {
                    return Err(EngineError::UnknownParent(*missing));
                }
                self.insert_organism(OrganismState {
                    organism_id: *organism_id,
                    species: species.clone(),
                    role: *role,
                    birth_category: birth_category.clone(),
                    parent_ids: parent_ids.clone(),
                    initialized_at: self.tick,
                    born_at: Some(self.tick),
                    initial_age_ticks: 0,
                    location_id: *location_id,
                    death: None,
                })?;
            }
            DomainEvent::OrganismDied { organism_id, cause } => {
                self.require_status(WorldStatus::Running)?;
                let organism = self
                    .organisms
                    .get_mut(organism_id)
                    .ok_or(EngineError::UnknownOrganism(*organism_id))?;
                if !organism.is_alive() {
                    return Err(EngineError::OrganismAlreadyDead(*organism_id));
                }
                organism.death = Some(DeathRecord {
                    tick: self.tick,
                    cause: cause.clone(),
                });
            }
            DomainEvent::WorldExtinct => {
                self.require_status(WorldStatus::Running)?;
                if self.living_people() != 0 {
                    return Err(EngineError::LivingPeopleRemain);
                }
                self.status = WorldStatus::Extinct;
            }
            DomainEvent::WorldArchived => {
                self.require_status(WorldStatus::Extinct)?;
                self.status = WorldStatus::Archived;
            }
        }
        Ok(())
    }

    fn insert_organism(&mut self, organism: OrganismState) -> Result<(), EngineError> {
        let id = organism.organism_id;
        if self.organisms.insert(id, organism).is_some() {
            return Err(EngineError::DuplicateOrganism(id));
        }
        Ok(())
    }

    fn require_status(&self, expected: WorldStatus) -> Result<(), EngineError> {
        if self.status == expected {
            Ok(())
        } else {
            Err(EngineError::InvalidLifecycle {
                expected,
                actual: self.status,
            })
        }
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self.manifest.ruleset_version == 0 {
            return Err(EngineError::ZeroRulesetVersion);
        }
        if let Some(configuration) = &self.configuration {
            configuration.validate()?;
        }
        if self.status == WorldStatus::Initializing
            && (self.tick != SimTick::ZERO || !self.organisms.is_empty())
        {
            return Err(EngineError::InvalidGenesisState);
        }
        if matches!(self.status, WorldStatus::Extinct | WorldStatus::Archived)
            && self.living_people() != 0
        {
            return Err(EngineError::LivingPeopleRemain);
        }
        for (id, organism) in &self.organisms {
            if id != &organism.organism_id {
                return Err(EngineError::OrganismKeyMismatch(*id));
            }
            organism.species.validate()?;
            if organism
                .parent_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            {
                return Err(EngineError::NonCanonicalParentOrder);
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct StateHashMaterial<'a> {
    state_hash_schema_version: u16,
    manifest: &'a WorldManifest,
    #[serde(skip_serializing_if = "Option::is_none")]
    configuration: Option<&'a WorldConfiguration>,
    status: WorldStatus,
    tick: SimTick,
    organisms: Vec<&'a OrganismState>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Snapshot {
    pub snapshot_schema_version: u16,
    pub world_id: WorldId,
    pub through_sequence: EventSequence,
    pub last_event_hash: Digest,
    pub state_hash: Digest,
    pub state: EngineState,
}

impl Snapshot {
    pub fn new(
        state: EngineState,
        through_sequence: EventSequence,
        last_event_hash: Digest,
    ) -> Result<Self, EngineError> {
        state.validate()?;
        let state_hash = state.state_hash()?;
        let snapshot_schema_version = if state.configuration.is_some() {
            SNAPSHOT_SCHEMA_VERSION
        } else {
            LEGACY_SNAPSHOT_SCHEMA_VERSION
        };
        Ok(Self {
            snapshot_schema_version,
            world_id: state.world_id(),
            through_sequence,
            last_event_hash,
            state_hash,
            state,
        })
    }

    pub fn verify_integrity(&self) -> Result<(), EngineError> {
        if !matches!(
            self.snapshot_schema_version,
            LEGACY_SNAPSHOT_SCHEMA_VERSION | SNAPSHOT_SCHEMA_VERSION
        ) {
            return Err(EngineError::UnsupportedSnapshotSchema(
                self.snapshot_schema_version,
            ));
        }
        let expected_schema_version = if self.state.configuration.is_some() {
            SNAPSHOT_SCHEMA_VERSION
        } else {
            LEGACY_SNAPSHOT_SCHEMA_VERSION
        };
        if self.snapshot_schema_version != expected_schema_version {
            return Err(EngineError::SnapshotSchemaMismatch {
                expected: expected_schema_version,
                actual: self.snapshot_schema_version,
            });
        }
        if self.world_id != self.state.world_id() {
            return Err(EngineError::SnapshotWorldMismatch);
        }
        self.state.validate()?;
        let calculated = self.state.state_hash()?;
        if calculated != self.state_hash {
            return Err(EngineError::SnapshotHashMismatch {
                expected: self.state_hash,
                calculated,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayOutcome {
    pub state: EngineState,
    pub through_sequence: EventSequence,
    pub last_event_hash: Digest,
}

impl ReplayOutcome {
    pub fn snapshot(&self) -> Result<Snapshot, EngineError> {
        Snapshot::new(
            self.state.clone(),
            self.through_sequence,
            self.last_event_hash,
        )
    }
}

pub fn replay(
    manifest: WorldManifest,
    batches: &[EventBatch],
) -> Result<ReplayOutcome, EngineError> {
    replay_from_cursor(
        EngineState::new(manifest),
        EventSequence::ZERO,
        Digest::ZERO,
        batches,
    )
}

pub fn replay_from_snapshot(
    snapshot: &Snapshot,
    tail: &[EventBatch],
) -> Result<ReplayOutcome, EngineError> {
    snapshot.verify_integrity()?;
    replay_from_cursor(
        snapshot.state.clone(),
        snapshot.through_sequence,
        snapshot.last_event_hash,
        tail,
    )
}

fn replay_from_cursor(
    mut state: EngineState,
    mut through_sequence: EventSequence,
    mut last_event_hash: Digest,
    batches: &[EventBatch],
) -> Result<ReplayOutcome, EngineError> {
    for batch in batches {
        batch.verify_integrity()?;
        if batch.world_id != state.world_id() {
            return Err(EngineError::BatchWorldMismatch);
        }
        if batch.ruleset_version != state.ruleset_version() {
            return Err(EngineError::BatchRulesetMismatch);
        }
        let expected_sequence = through_sequence.checked_next()?;
        if batch.sequence != expected_sequence {
            return Err(EngineError::BatchSequenceMismatch {
                expected: expected_sequence,
                actual: batch.sequence,
            });
        }
        if batch.previous_hash != last_event_hash {
            return Err(EngineError::PreviousHashMismatch {
                expected: last_event_hash,
                actual: batch.previous_hash,
            });
        }

        let configures_world = batch
            .events
            .iter()
            .any(|record| matches!(&record.event, DomainEvent::WorldConfigured { .. }));
        let expected_event_schema = if state.configuration.is_some() || configures_world {
            EVENT_SCHEMA_VERSION
        } else {
            LEGACY_EVENT_SCHEMA_VERSION
        };
        if batch.event_schema_version != expected_event_schema {
            return Err(EngineError::BatchEventSchemaMismatch {
                expected: expected_event_schema,
                actual: batch.event_schema_version,
            });
        }

        let events = batch
            .events
            .iter()
            .map(|record| record.event.clone())
            .collect::<Vec<_>>();
        state.apply_events(&events)?;
        state.validate()?;
        if state.tick != batch.tick {
            return Err(EngineError::BatchTickMismatch {
                expected: state.tick,
                actual: batch.tick,
            });
        }
        let calculated_state_hash = state.state_hash()?;
        if calculated_state_hash != batch.post_state_hash {
            return Err(EngineError::PostStateHashMismatch {
                expected: batch.post_state_hash,
                calculated: calculated_state_hash,
            });
        }

        through_sequence = batch.sequence;
        last_event_hash = batch.batch_hash;
    }

    Ok(ReplayOutcome {
        state,
        through_sequence,
        last_event_hash,
    })
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("ruleset version must be greater than zero")]
    ZeroRulesetVersion,
    #[error("world lifecycle expected {expected:?}, found {actual:?}")]
    InvalidLifecycle {
        expected: WorldStatus,
        actual: WorldStatus,
    },
    #[error("genesis event manifest does not match the precommitted manifest")]
    ManifestMismatch,
    #[error("initializing state must be empty at tick zero")]
    InvalidGenesisState,
    #[error("initial organism list contains a duplicate identity")]
    DuplicateInitialOrganism,
    #[error("world configuration must be committed before initial organisms")]
    ConfigurationAfterOrganisms,
    #[error("world configuration was already committed")]
    WorldAlreadyConfigured,
    #[error("transition planned {actual} events; configured maximum is {maximum}")]
    EventBudgetExceeded { actual: u64, maximum: u64 },
    #[error("transition contains more events than the host can count")]
    TooManyEvents,
    #[error("organism {0} already exists")]
    DuplicateOrganism(EntityId),
    #[error("organism map key {0} does not match its value")]
    OrganismKeyMismatch(EntityId),
    #[error("organism {0} does not exist")]
    UnknownOrganism(EntityId),
    #[error("parent organism {0} does not exist")]
    UnknownParent(EntityId),
    #[error("organism {0} is already dead")]
    OrganismAlreadyDead(EntityId),
    #[error("parent identities must be strictly sorted and unique")]
    NonCanonicalParentOrder,
    #[error("cannot mark the world extinct while viable people remain")]
    LivingPeopleRemain,
    #[error("invalid tick transition at {current}: event says {from} -> {to}")]
    InvalidTickTransition {
        current: SimTick,
        from: SimTick,
        to: SimTick,
    },
    #[error("event batch belongs to another world")]
    BatchWorldMismatch,
    #[error("event batch ruleset does not match the world manifest")]
    BatchRulesetMismatch,
    #[error("event schema mismatch: world expects {expected}, batch uses {actual}")]
    BatchEventSchemaMismatch { expected: u16, actual: u16 },
    #[error("event sequence mismatch: expected {expected}, found {actual}")]
    BatchSequenceMismatch {
        expected: EventSequence,
        actual: EventSequence,
    },
    #[error("previous event hash mismatch: expected {expected}, found {actual}")]
    PreviousHashMismatch { expected: Digest, actual: Digest },
    #[error("event batch tick mismatch: state is {expected}, batch says {actual}")]
    BatchTickMismatch { expected: SimTick, actual: SimTick },
    #[error("post-state hash mismatch: stored {expected}, calculated {calculated}")]
    PostStateHashMismatch {
        expected: Digest,
        calculated: Digest,
    },
    #[error("snapshot schema version {0} is unsupported")]
    UnsupportedSnapshotSchema(u16),
    #[error("snapshot schema mismatch: state expects {expected}, snapshot uses {actual}")]
    SnapshotSchemaMismatch { expected: u16, actual: u16 },
    #[error("snapshot world does not match its state")]
    SnapshotWorldMismatch,
    #[error("snapshot hash mismatch: stored {expected}, calculated {calculated}")]
    SnapshotHashMismatch {
        expected: Digest,
        calculated: Digest,
    },
    #[error(transparent)]
    SpeciesIdentity(#[from] SpeciesIdentityError),
    #[error(transparent)]
    WorldConfiguration(#[from] WorldConfigurationError),
    #[error(transparent)]
    TimeOverflow(#[from] TimeOverflow),
    #[error(transparent)]
    SequenceOverflow(#[from] SequenceOverflow),
    #[error(transparent)]
    EventBatch(#[from] EventBatchError),
    #[error(transparent)]
    CanonicalHash(#[from] CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use world_domain::{SpatialGrid, WorldDataBundleReference, WorldSeed, WorldStatus};

    fn manifest() -> WorldManifest {
        WorldManifest::new(
            WorldId::from_uuid(Uuid::from_u128(0x100)),
            WorldSeed::new(7640891576956012809),
            RULESET_VERSION,
        )
    }

    fn human() -> SpeciesIdentity {
        SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("verified test species")
    }

    fn initial_person(world_id: WorldId) -> InitialOrganism {
        InitialOrganism {
            organism_id: EntityId::deterministic(world_id, b"initial-person-1"),
            species: human(),
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("female").expect("valid category"),
            initial_age_ticks: 0,
            location_id: None,
        }
    }

    fn world_configuration() -> WorldConfiguration {
        WorldConfiguration::new(
            300,
            SpatialGrid {
                epsg: 32_736,
                origin_easting_mm: 500_000_000,
                origin_northing_mm: 9_700_000_000,
                cell_size_mm: 10_000,
                width_cells: 100,
                height_cells: 100,
            },
            WorldDataBundleReference::new(
                1,
                "configured-engine-test",
                "0.1.0",
                Digest::sha256(b"configured engine test data"),
                "https://data.atinycivilization.com/configured-engine-test/0.1.0.json",
                "CC-BY-4.0",
            )
            .expect("valid bundle reference"),
            10_000,
        )
        .expect("valid world configuration")
    }

    fn committed_history() -> Vec<EventBatch> {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_genesis(vec![initial_person(manifest.world_id)])
            .expect("valid genesis");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("valid genesis batch");
        let tick_events = running.plan_next_tick().expect("valid tick");
        let (_, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("valid tick batch");
        vec![genesis, tick]
    }

    #[test]
    fn independent_histories_have_identical_hashes() {
        let first = committed_history();
        let second = committed_history();
        assert_eq!(first, second);

        let replayed = replay(manifest(), &first);
        assert!(matches!(
            replayed,
            Ok(outcome)
                if outcome.through_sequence == EventSequence::new(2)
                    && outcome.state.tick() == SimTick::new(1)
        ));
    }

    #[test]
    fn configured_genesis_pins_world_data_and_event_schema() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let configuration = world_configuration();
        let genesis_events = initial
            .plan_configured_genesis(
                configuration.clone(),
                vec![initial_person(manifest.world_id)],
            )
            .expect("valid configured genesis");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("valid configured genesis batch");

        assert_eq!(genesis.event_schema_version, EVENT_SCHEMA_VERSION);
        assert_eq!(running.configuration(), Some(&configuration));
        let tick_events = running.plan_next_tick().expect("valid configured tick");
        let (_, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("valid configured tick batch");
        assert_eq!(tick.event_schema_version, EVENT_SCHEMA_VERSION);
        let prefix = replay(manifest.clone(), std::slice::from_ref(&genesis))
            .expect("valid configured prefix");
        let snapshot = prefix.snapshot().expect("valid configured snapshot");
        assert_eq!(snapshot.snapshot_schema_version, SNAPSHOT_SCHEMA_VERSION);
        let complete =
            replay(manifest.clone(), &[genesis, tick.clone()]).expect("valid configured history");
        let from_snapshot =
            replay_from_snapshot(&snapshot, &[tick]).expect("valid configured snapshot tail");
        assert_eq!(from_snapshot, complete);

        let mut downgraded_snapshot = snapshot;
        downgraded_snapshot.snapshot_schema_version = LEGACY_SNAPSHOT_SCHEMA_VERSION;
        assert!(matches!(
            downgraded_snapshot.verify_integrity(),
            Err(EngineError::SnapshotSchemaMismatch {
                expected: SNAPSHOT_SCHEMA_VERSION,
                actual: LEGACY_SNAPSHOT_SCHEMA_VERSION,
            })
        ));

        assert!(
            committed_history()
                .iter()
                .all(|batch| batch.event_schema_version == LEGACY_EVENT_SCHEMA_VERSION)
        );
    }

    #[test]
    fn configured_world_rejects_schema_downgrade() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                world_configuration(),
                vec![initial_person(manifest.world_id)],
            )
            .expect("valid configured genesis");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("valid configured genesis batch");
        let tick_events = running.plan_next_tick().expect("valid configured tick");
        let (_, canonical_tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("valid configured tick batch");
        let downgraded_events = canonical_tick
            .events
            .iter()
            .map(|record| record.event.clone())
            .collect();
        let downgraded_tick = EventBatch::new(
            LEGACY_EVENT_SCHEMA_VERSION,
            canonical_tick.world_id,
            canonical_tick.sequence,
            canonical_tick.tick,
            canonical_tick.ruleset_version,
            canonical_tick.previous_hash,
            downgraded_events,
            canonical_tick.post_state_hash,
        )
        .expect("internally valid legacy-schema batch");

        assert!(matches!(
            replay(manifest, &[genesis, downgraded_tick]),
            Err(EngineError::BatchEventSchemaMismatch {
                expected: EVENT_SCHEMA_VERSION,
                actual: LEGACY_EVENT_SCHEMA_VERSION,
            })
        ));
    }

    #[test]
    fn configured_event_budget_covers_genesis_and_ticks() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let mut configuration = world_configuration();
        configuration.max_events_per_transition = 2;
        let genesis_events = initial
            .plan_configured_genesis(configuration, vec![initial_person(manifest.world_id)])
            .expect("valid configured genesis plan");

        assert!(matches!(
            initial.commit(EventSequence::new(1), Digest::ZERO, genesis_events),
            Err(EngineError::EventBudgetExceeded {
                actual: 3,
                maximum: 2,
            })
        ));
    }

    #[test]
    fn world_configuration_is_tick_zero_only_and_single_assignment() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let legacy_events = initial
            .plan_genesis(vec![initial_person(manifest.world_id)])
            .expect("valid legacy genesis");
        let (legacy_running, legacy_genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, legacy_events)
            .expect("valid legacy genesis batch");
        assert!(matches!(
            legacy_running.commit(
                EventSequence::new(2),
                legacy_genesis.batch_hash,
                vec![DomainEvent::WorldConfigured {
                    configuration: world_configuration(),
                }],
            ),
            Err(EngineError::ConfigurationAfterOrganisms)
        ));

        let configured_events = initial
            .plan_configured_genesis(
                world_configuration(),
                vec![initial_person(manifest.world_id)],
            )
            .expect("valid configured genesis");
        let (configured_running, configured_genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, configured_events)
            .expect("valid configured genesis batch");
        assert!(matches!(
            configured_running.commit(
                EventSequence::new(2),
                configured_genesis.batch_hash,
                vec![DomainEvent::WorldConfigured {
                    configuration: world_configuration(),
                }],
            ),
            Err(EngineError::WorldAlreadyConfigured)
        ));
    }

    #[test]
    fn snapshot_plus_tail_matches_genesis_replay() {
        let batches = committed_history();
        let prefix = replay(manifest(), &batches[..1]).expect("valid prefix");
        let snapshot = prefix.snapshot().expect("valid snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            LEGACY_SNAPSHOT_SCHEMA_VERSION
        );
        let complete = replay(manifest(), &batches).expect("valid full replay");
        let from_snapshot =
            replay_from_snapshot(&snapshot, &batches[1..]).expect("valid tail replay");

        assert_eq!(from_snapshot, complete);
        let from_snapshot_hash = from_snapshot.state.state_hash();
        let complete_hash = complete.state.state_hash();
        assert!(matches!(
            (from_snapshot_hash, complete_hash),
            (Ok(left), Ok(right)) if left == right
        ));
    }

    #[test]
    fn missing_or_modified_batches_fail_replay() {
        let batches = committed_history();
        let missing = replay(manifest(), &batches[1..]);
        assert!(matches!(
            missing,
            Err(EngineError::BatchSequenceMismatch { .. })
        ));

        let mut modified = batches;
        modified[1].post_state_hash = Digest::sha256(b"fabricated");
        assert!(matches!(
            replay(manifest(), &modified),
            Err(EngineError::EventBatch(
                EventBatchError::HashMismatch { .. }
            ))
        ));
    }

    #[test]
    fn last_person_death_archives_world_exactly_once() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_genesis(vec![initial_person(manifest.world_id)])
            .expect("valid genesis");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("valid genesis batch");
        let person_id = initial_person(manifest.world_id).organism_id;
        let events = running
            .plan_death(
                person_id,
                DeathCause {
                    mechanism: "verification_fixture".to_owned(),
                },
            )
            .expect("valid death plan");
        let (archived, death) = running
            .commit(EventSequence::new(2), genesis.batch_hash, events)
            .expect("valid extinction batch");

        assert_eq!(archived.status(), WorldStatus::Archived);
        assert_eq!(archived.living_people(), 0);
        assert!(matches!(
            archived.plan_next_tick(),
            Err(EngineError::InvalidLifecycle {
                actual: WorldStatus::Archived,
                ..
            })
        ));
        assert!(replay(manifest, &[genesis, death]).is_ok());
    }
}
