//! Pure deterministic planning, state transitions, snapshots, and replay.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{
    BirthCategory, CanonicalHashError, DeathCause, Digest, DomainEvent, EntityId, EventBatch,
    EventBatchError, EventSequence, OrganismRole, SequenceOverflow, SimTick, SpeciesIdentity,
    SpeciesIdentityError, TimeOverflow, WorldId, WorldManifest, WorldStatus,
};

/// Version pinned to each world so old histories are never silently reinterpreted.
pub const RULESET_VERSION: u32 = 1;
pub const SNAPSHOT_SCHEMA_VERSION: u16 = 1;
const STATE_HASH_SCHEMA_VERSION: u16 = 1;

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
    status: WorldStatus,
    tick: SimTick,
    organisms: BTreeMap<EntityId, OrganismState>,
}

impl EngineState {
    #[must_use]
    pub fn new(manifest: WorldManifest) -> Self {
        Self {
            manifest,
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

        let mut events = Vec::with_capacity(initial_organisms.len().saturating_add(1));
        events.push(DomainEvent::WorldStarted {
            manifest: self.manifest.clone(),
        });
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
        let state_hash = next.state_hash()?;
        let batch = EventBatch::new(
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
        Digest::canonical(&StateHashMaterial {
            state_hash_schema_version: STATE_HASH_SCHEMA_VERSION,
            manifest: &self.manifest,
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
        Ok(Self {
            snapshot_schema_version: SNAPSHOT_SCHEMA_VERSION,
            world_id: state.world_id(),
            through_sequence,
            last_event_hash,
            state_hash,
            state,
        })
    }

    pub fn verify_integrity(&self) -> Result<(), EngineError> {
        if self.snapshot_schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(EngineError::UnsupportedSnapshotSchema(
                self.snapshot_schema_version,
            ));
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
    use world_domain::{WorldSeed, WorldStatus};

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
    fn snapshot_plus_tail_matches_genesis_replay() {
        let batches = committed_history();
        let prefix = replay(manifest(), &batches[..1]).expect("valid prefix");
        let snapshot = prefix.snapshot().expect("valid snapshot");
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
