//! Pure deterministic planning, state transitions, snapshots, and replay.

#[allow(dead_code)]
mod partition;
#[allow(dead_code)]
mod refinement;
#[allow(dead_code)]
mod spatial;

use std::collections::BTreeMap;

use partition::{
    Emission, PartitionOutput, PartitionSchedule, ScheduledWork, SchedulerError, SubjectKey,
    WorkKey, WorkOutput,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{
    BirthCategory, CELESTIAL_STATE_EVENT_SCHEMA_VERSION, CONFIGURED_EVENT_SCHEMA_VERSION,
    CanonicalHashError, CelestialState, DeathCause, Digest, DomainEvent,
    EMBODIED_POSITION_EVENT_SCHEMA_VERSION, EVENT_SCHEMA_VERSION, EntityId, EventBatch,
    EventBatchError, EventSequence, ExecutionScale, LEGACY_EVENT_SCHEMA_VERSION, OrganismRole,
    PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION, PrimitiveAction, S2CellId, S2CellIdError,
    SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION, SequenceOverflow, SimTick, SituatedPerception,
    SpeciesIdentity, SpeciesIdentityError, TimeOverflow, WorldConfiguration,
    WorldConfigurationError, WorldId, WorldManifest, WorldStatus,
};

/// Ruleset one has the original empty full-Earth execution schedule.
pub const LEGACY_RULESET_VERSION: u32 = 1;
/// Ruleset two adds the executable per-organism barrier while preserving
/// ruleset-one replay byte-for-byte.
pub const ORGANISM_EXECUTION_RULESET_VERSION: u32 = 2;
/// Current executable ruleset. Existing provisional worlds continue to use the
/// verified body-clock kernel; the runner selects its own default for newly
/// initialized full-Earth worlds.
pub const RULESET_VERSION: u32 = ORGANISM_EXECUTION_RULESET_VERSION;
/// Ruleset three requires one source-backed celestial input per tick.
pub const CELESTIAL_DRIVER_RULESET_VERSION: u32 = 3;
pub const LEGACY_SNAPSHOT_SCHEMA_VERSION: u16 = 1;
pub const SNAPSHOT_SCHEMA_VERSION: u16 = 2;
pub const EMBODIED_POSITION_SNAPSHOT_SCHEMA_VERSION: u16 = 3;
pub const PARTITIONED_EXECUTION_SNAPSHOT_SCHEMA_VERSION: u16 = 4;
pub const PROVISIONAL_WORLD_SNAPSHOT_SCHEMA_VERSION: u16 = 5;
pub const SCHEDULED_CAUSAL_SNAPSHOT_SCHEMA_VERSION: u16 = 6;
pub const CELESTIAL_DRIVER_SNAPSHOT_SCHEMA_VERSION: u16 = 7;
/// The first deterministic execution phase: every living embodied organism receives
/// one body/ecology-process slot per tick. Ruleset-specific processes can later emit
/// physical state changes through this fixed barrier without changing its ordering.
const ORGANISM_BODY_PHASE_CODE: u16 = 1;
const ORGANISM_BODY_PROCESS_CODE: u16 = 1;
const LEGACY_STATE_HASH_SCHEMA_VERSION: u16 = 1;
const STATE_HASH_SCHEMA_VERSION: u16 = 2;
const EMBODIED_POSITION_STATE_HASH_SCHEMA_VERSION: u16 = 3;
const PARTITIONED_EXECUTION_STATE_HASH_SCHEMA_VERSION: u16 = 4;
const PROVISIONAL_WORLD_STATE_HASH_SCHEMA_VERSION: u16 = 5;
const SCHEDULED_CAUSAL_STATE_HASH_SCHEMA_VERSION: u16 = 6;
const CELESTIAL_DRIVER_STATE_HASH_SCHEMA_VERSION: u16 = 7;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InitialOrganism {
    pub organism_id: EntityId,
    pub species: SpeciesIdentity,
    pub role: OrganismRole,
    pub birth_category: BirthCategory,
    pub initial_age_ticks: u64,
    pub location_id: Option<EntityId>,
    pub embodied_patch: Option<S2CellId>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    age_ticks: Option<u64>,
    location_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embodied_patch: Option<S2CellId>,
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

    #[must_use]
    pub const fn embodied_patch(&self) -> Option<S2CellId> {
        self.embodied_patch
    }

    fn age_ticks(&self) -> Option<u64> {
        self.age_ticks
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    partition_schedule: Option<PartitionSchedule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    celestial_state: Option<CelestialState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    celestial_tick: Option<SimTick>,
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
            partition_schedule: None,
            celestial_state: None,
            celestial_tick: None,
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

    #[must_use]
    pub fn scheduled_work_count(&self) -> usize {
        self.partition_schedule
            .as_ref()
            .map_or(0, |schedule| schedule.entries().len())
    }

    /// The latest exact source state admitted by the ruleset-three driver.
    #[must_use]
    pub const fn celestial_state(&self) -> Option<CelestialState> {
        self.celestial_state
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
                embodied_patch: organism.embodied_patch,
            }
        }));
        Ok(events)
    }

    pub fn plan_next_tick(&self) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_status(WorldStatus::Running)?;
        if self.uses_celestial_driver() {
            return Err(EngineError::CelestialStateRequired);
        }
        self.plan_next_tick_internal(None)
    }

    /// Plan one complete ruleset-three transition. The caller supplies the exact
    /// result of evaluating the world-pinned celestial source; replay consumes this
    /// recorded result and never opens an ephemeris itself.
    pub fn plan_next_tick_with_celestial(
        &self,
        celestial_state: CelestialState,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        if !self.uses_celestial_driver() {
            return Err(EngineError::CelestialStateUnsupported);
        }
        self.plan_next_tick_internal(Some(celestial_state))
    }

    fn plan_next_tick_internal(
        &self,
        celestial_state: Option<CelestialState>,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_status(WorldStatus::Running)?;
        let next = self.tick.checked_next()?;
        let scheduled_events = self.plan_partition_tick_events()?;
        let mut events = vec![DomainEvent::TickAdvanced {
            from: self.tick,
            to: next,
        }];
        events.extend(scheduled_events);
        if let Some(state) = celestial_state {
            events.push(DomainEvent::CelestialStateRecorded { state });
        }
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

    /// Record direct sensory evidence for a living organism. This is not an observer
    /// projection and cannot contain an affordance or culturally learned conclusion.
    pub fn plan_perception(
        &self,
        organism_id: EntityId,
        perception: SituatedPerception,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_living_organism(organism_id)?;
        perception
            .validate()
            .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
        Ok(vec![DomainEvent::OrganismPerceived {
            organism_id,
            perception,
        }])
    }

    /// Record a chosen primitive bodily act for a living organism. World physics will
    /// eventually resolve effects; this event itself encodes no cultural outcome.
    pub fn plan_action(
        &self,
        organism_id: EntityId,
        action: PrimitiveAction,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_living_organism(organism_id)?;
        action
            .validate()
            .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
        Ok(vec![DomainEvent::OrganismActed {
            organism_id,
            action,
        }])
    }

    /// Record a resolved relocation between two full-Earth embodied patches. The
    /// primitive bodily action that caused it remains a separate event.
    pub fn plan_movement(
        &self,
        organism_id: EntityId,
        to_patch: S2CellId,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_living_organism(organism_id)?;
        self.validate_embodied_patch(to_patch)?;
        let from_patch = self
            .organisms
            .get(&organism_id)
            .and_then(|organism| organism.embodied_patch)
            .ok_or(EngineError::MissingEmbodiedPatch(organism_id))?;
        Ok(vec![DomainEvent::OrganismMoved {
            organism_id,
            from_patch,
            to_patch,
        }])
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
            self.validate_event_budget(configuration, &events, &next)?;
        }
        let state_hash = next.state_hash()?;
        let event_schema_version = next.event_schema_version();
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
        let state_hash_schema_version = self.state_hash_schema_version();
        Digest::canonical(&StateHashMaterial {
            state_hash_schema_version,
            manifest: &self.manifest,
            configuration: self.configuration.as_ref(),
            status: self.status,
            tick: self.tick,
            organisms: self.organisms.values().collect(),
            partition_schedule: self.partition_schedule.as_ref(),
            celestial_state: self.celestial_state,
            celestial_tick: self.celestial_tick,
        })
    }

    fn partition_profile(&self) -> Option<(u8, u32)> {
        self.configuration
            .as_ref()
            .and_then(WorldConfiguration::partitioned_execution)
            .map(|execution| {
                (
                    execution.partition_s2_level,
                    execution.max_events_per_partition_transition,
                )
            })
    }

    fn plan_partition_tick_events(&self) -> Result<Vec<DomainEvent>, EngineError> {
        let Some((partition_level, maximum_events)) = self.partition_profile() else {
            return Ok(Vec::new());
        };
        if !self.uses_organism_execution_kernel() {
            return Ok(Vec::new());
        }
        let schedule = self.partition_schedule.as_ref().ok_or_else(|| {
            EngineError::PartitionScheduleState(
                "full-Earth state has no durable partition schedule".to_owned(),
            )
        })?;
        if schedule != &self.expected_partition_schedule(partition_level)? {
            return Err(EngineError::PartitionScheduleState(
                "partition schedule does not exactly cover every living embodied organism"
                    .to_owned(),
            ));
        }
        let plan = schedule.plan_next_tick(self.tick)?;
        let outputs = plan
            .partitions()
            .iter()
            .map(|partition| {
                let work_outputs = partition
                    .work()
                    .iter()
                    .map(|work| {
                        let organism = self
                            .organisms
                            .values()
                            .find(|organism| {
                                SubjectKey::from_entity(organism.organism_id)
                                    == work.key().subject()
                            })
                            .ok_or_else(|| {
                                EngineError::PartitionScheduleState(
                                    "scheduled work has no matching organism".to_owned(),
                                )
                            })?;
                        let from_age_ticks = organism.age_ticks().ok_or_else(|| {
                            EngineError::PartitionScheduleState(
                                "ruleset-two organism has no durable age state".to_owned(),
                            )
                        })?;
                        let to_age_ticks = from_age_ticks
                            .checked_add(1)
                            .ok_or(EngineError::AgeOverflow(organism.organism_id))?;
                        Ok(WorkOutput::new(
                            work.key(),
                            vec![Emission::new(
                                partition.partition(),
                                work.key(),
                                0,
                                DomainEvent::OrganismAgeAdvanced {
                                    organism_id: organism.organism_id,
                                    from_age_ticks,
                                    to_age_ticks,
                                },
                            )],
                            Vec::new(),
                        ))
                    })
                    .collect::<Result<Vec<_>, EngineError>>()?;
                Ok(PartitionOutput::new(partition.partition(), work_outputs))
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let resolved = plan.complete(outputs, maximum_events)?;
        Ok(resolved
            .emissions()
            .iter()
            .map(|emission| emission.event().clone())
            .collect())
    }

    fn resolve_partition_tick(&self) -> Result<Option<PartitionSchedule>, EngineError> {
        let Some((partition_level, maximum_events)) = self.partition_profile() else {
            return Ok(None);
        };
        let schedule = self.partition_schedule.as_ref().ok_or_else(|| {
            EngineError::PartitionScheduleState(
                "full-Earth state has no durable partition schedule".to_owned(),
            )
        })?;
        if schedule.partition_level() != partition_level {
            return Err(EngineError::PartitionScheduleState(
                "partition schedule level differs from world configuration".to_owned(),
            ));
        }
        let expected = self.expected_partition_schedule(partition_level)?;
        if schedule != &expected {
            return Err(EngineError::PartitionScheduleState(
                "partition schedule does not exactly cover every living embodied organism"
                    .to_owned(),
            ));
        }
        let plan = schedule.plan_next_tick(self.tick)?;
        let outputs = plan
            .partitions()
            .iter()
            .map(|partition| {
                PartitionOutput::new(
                    partition.partition(),
                    partition
                        .work()
                        .iter()
                        .map(|work| WorkOutput::new(work.key(), Vec::new(), Vec::new()))
                        .collect(),
                )
            })
            .collect();
        // The ruleset has not yet admitted scientific body/ecology effects. The
        // barrier is nevertheless fully executed: every scheduled organism is
        // accounted for, output sets are complete, and budgets are enforced. A
        // future process may add emissions without weakening these invariants.
        let resolved = plan.complete::<DomainEvent>(outputs, maximum_events)?;
        Ok(Some(resolved.next_schedule().clone()))
    }

    fn expected_partition_schedule(
        &self,
        partition_level: u8,
    ) -> Result<PartitionSchedule, EngineError> {
        if !self.uses_organism_execution_kernel() {
            return Ok(PartitionSchedule::new(partition_level, Vec::new())?);
        }
        let due_tick = self.tick.checked_next()?;
        let entries = self
            .organisms
            .values()
            .filter(|organism| organism.is_alive())
            .map(|organism| {
                let patch = organism.embodied_patch.ok_or_else(|| {
                    EngineError::PartitionScheduleState(
                        "a living full-Earth organism lacks an embodied patch".to_owned(),
                    )
                })?;
                let key = WorkKey::new(
                    ORGANISM_BODY_PHASE_CODE,
                    SubjectKey::from_entity(organism.organism_id),
                    ORGANISM_BODY_PROCESS_CODE,
                    0,
                )?;
                ScheduledWork::routed(due_tick, patch, partition_level, key)
                    .map_err(EngineError::from)
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        Ok(PartitionSchedule::new(partition_level, entries)?)
    }

    fn refresh_partition_schedule(&mut self) -> Result<(), EngineError> {
        let Some((partition_level, _)) = self.partition_profile() else {
            return Ok(());
        };
        self.partition_schedule = Some(self.expected_partition_schedule(partition_level)?);
        Ok(())
    }

    fn uses_organism_execution_kernel(&self) -> bool {
        self.manifest.ruleset_version >= ORGANISM_EXECUTION_RULESET_VERSION
    }

    fn uses_celestial_driver(&self) -> bool {
        self.manifest.ruleset_version >= CELESTIAL_DRIVER_RULESET_VERSION
    }

    fn validate_event_budget(
        &self,
        configuration: &WorldConfiguration,
        events: &[DomainEvent],
        resulting_state: &Self,
    ) -> Result<(), EngineError> {
        let maximum = u64::from(configuration.transition_event_limit());
        match &configuration.execution {
            ExecutionScale::SingleTransition { .. } => {
                let actual = u64::try_from(events.len()).map_err(|_| EngineError::TooManyEvents)?;
                if actual > maximum {
                    return Err(EngineError::EventBudgetExceeded { actual, maximum });
                }
            }
            ExecutionScale::Partitioned {
                partitioned_execution,
            } => {
                let mut counts = BTreeMap::<Option<S2CellId>, u64>::new();
                for event in events {
                    let partition = self.event_partition(
                        event,
                        resulting_state,
                        partitioned_execution.partition_s2_level,
                    )?;
                    let count = counts.entry(partition).or_default();
                    *count = count.checked_add(1).ok_or(EngineError::TooManyEvents)?;
                }
                if let Some((partition, actual)) =
                    counts.into_iter().find(|(_, actual)| *actual > maximum)
                {
                    return Err(EngineError::PartitionEventBudgetExceeded {
                        partition,
                        actual,
                        maximum,
                    });
                }
            }
        }
        Ok(())
    }

    fn event_partition(
        &self,
        event: &DomainEvent,
        resulting_state: &Self,
        partition_level: u8,
    ) -> Result<Option<S2CellId>, EngineError> {
        let patch = match event {
            DomainEvent::OrganismInitialized { embodied_patch, .. }
            | DomainEvent::OrganismBorn { embodied_patch, .. } => *embodied_patch,
            DomainEvent::OrganismMoved { to_patch, .. } => Some(*to_patch),
            DomainEvent::OrganismDied { organism_id, .. }
            | DomainEvent::OrganismAgeAdvanced { organism_id, .. }
            | DomainEvent::OrganismPerceived { organism_id, .. }
            | DomainEvent::OrganismActed { organism_id, .. } => self
                .organisms
                .get(organism_id)
                .or_else(|| resulting_state.organisms.get(organism_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::WorldStarted { .. }
            | DomainEvent::WorldConfigured { .. }
            | DomainEvent::TickAdvanced { .. }
            | DomainEvent::CelestialStateRecorded { .. }
            | DomainEvent::WorldExtinct
            | DomainEvent::WorldArchived => None,
        };
        patch
            .map(|patch| patch.ancestor(partition_level).map_err(EngineError::from))
            .transpose()
    }

    fn apply_events(&mut self, events: &[DomainEvent]) -> Result<(), EngineError> {
        for event in events {
            self.apply_event(event)?;
        }
        Ok(())
    }

    fn event_schema_version(&self) -> u16 {
        if self.uses_celestial_driver() {
            CELESTIAL_STATE_EVENT_SCHEMA_VERSION
        } else if self.uses_organism_execution_kernel()
            && self
                .configuration
                .as_ref()
                .and_then(WorldConfiguration::embodied_patch_s2_level)
                .is_some()
        {
            SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION
        } else if self
            .configuration
            .as_ref()
            .is_some_and(WorldConfiguration::is_provisional_execution)
        {
            PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION
        } else if self
            .configuration
            .as_ref()
            .and_then(WorldConfiguration::embodied_patch_s2_level)
            .is_some()
        {
            EMBODIED_POSITION_EVENT_SCHEMA_VERSION
        } else if self.configuration.is_some() {
            EVENT_SCHEMA_VERSION
        } else {
            LEGACY_EVENT_SCHEMA_VERSION
        }
    }

    fn state_hash_schema_version(&self) -> u16 {
        if self.uses_celestial_driver() {
            CELESTIAL_DRIVER_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_organism_execution_kernel() && self.partition_schedule.is_some() {
            SCHEDULED_CAUSAL_STATE_HASH_SCHEMA_VERSION
        } else if self
            .configuration
            .as_ref()
            .is_some_and(WorldConfiguration::is_provisional_execution)
        {
            PROVISIONAL_WORLD_STATE_HASH_SCHEMA_VERSION
        } else if self.partition_schedule.is_some() {
            PARTITIONED_EXECUTION_STATE_HASH_SCHEMA_VERSION
        } else if self
            .configuration
            .as_ref()
            .and_then(WorldConfiguration::embodied_patch_s2_level)
            .is_some()
        {
            EMBODIED_POSITION_STATE_HASH_SCHEMA_VERSION
        } else if self.configuration.is_some() {
            STATE_HASH_SCHEMA_VERSION
        } else {
            LEGACY_STATE_HASH_SCHEMA_VERSION
        }
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
                self.partition_schedule = configuration
                    .partitioned_execution()
                    .map(|execution| {
                        PartitionSchedule::new(execution.partition_s2_level, Vec::new())
                    })
                    .transpose()?;
            }
            DomainEvent::OrganismInitialized {
                organism_id,
                species,
                role,
                birth_category,
                initial_age_ticks,
                location_id,
                embodied_patch,
            } => {
                self.require_status(WorldStatus::Running)?;
                species.validate()?;
                self.validate_initial_embodied_patch(*embodied_patch)?;
                self.insert_organism(OrganismState {
                    organism_id: *organism_id,
                    species: species.clone(),
                    role: *role,
                    birth_category: birth_category.clone(),
                    parent_ids: Vec::new(),
                    initialized_at: self.tick,
                    born_at: None,
                    initial_age_ticks: *initial_age_ticks,
                    age_ticks: self
                        .uses_organism_execution_kernel()
                        .then_some(*initial_age_ticks),
                    location_id: *location_id,
                    embodied_patch: *embodied_patch,
                    death: None,
                })?;
                self.refresh_partition_schedule()?;
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
                let _ = self.resolve_partition_tick()?;
                self.tick = *to;
                self.refresh_partition_schedule()?;
            }
            DomainEvent::OrganismBorn {
                organism_id,
                species,
                role,
                birth_category,
                parent_ids,
                location_id,
                embodied_patch,
            } => {
                self.require_status(WorldStatus::Running)?;
                species.validate()?;
                self.validate_initial_embodied_patch(*embodied_patch)?;
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
                    age_ticks: self.uses_organism_execution_kernel().then_some(0),
                    location_id: *location_id,
                    embodied_patch: *embodied_patch,
                    death: None,
                })?;
                self.refresh_partition_schedule()?;
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
                self.refresh_partition_schedule()?;
            }
            DomainEvent::OrganismPerceived {
                organism_id,
                perception,
            } => {
                self.require_living_organism(*organism_id)?;
                perception
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
            }
            DomainEvent::OrganismActed {
                organism_id,
                action,
            } => {
                self.require_living_organism(*organism_id)?;
                action
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
            }
            DomainEvent::OrganismMoved {
                organism_id,
                from_patch,
                to_patch,
            } => {
                self.require_living_organism(*organism_id)?;
                self.validate_embodied_patch(*to_patch)?;
                let organism = self
                    .organisms
                    .get_mut(organism_id)
                    .ok_or(EngineError::UnknownOrganism(*organism_id))?;
                if organism.embodied_patch != Some(*from_patch) {
                    return Err(EngineError::UnexpectedEmbodiedPatch(*organism_id));
                }
                organism.embodied_patch = Some(*to_patch);
                self.refresh_partition_schedule()?;
            }
            DomainEvent::OrganismAgeAdvanced {
                organism_id,
                from_age_ticks,
                to_age_ticks,
            } => {
                self.require_living_organism(*organism_id)?;
                let expected = from_age_ticks
                    .checked_add(1)
                    .ok_or(EngineError::AgeOverflow(*organism_id))?;
                if to_age_ticks != &expected {
                    return Err(EngineError::InvalidAgeTransition(*organism_id));
                }
                let organism = self
                    .organisms
                    .get_mut(organism_id)
                    .ok_or(EngineError::UnknownOrganism(*organism_id))?;
                if organism.age_ticks != Some(*from_age_ticks) {
                    return Err(EngineError::InvalidAgeTransition(*organism_id));
                }
                organism.age_ticks = Some(*to_age_ticks);
            }
            DomainEvent::CelestialStateRecorded { state } => {
                if !self.uses_celestial_driver() {
                    return Err(EngineError::CelestialStateUnsupported);
                }
                if self.tick == SimTick::ZERO || self.celestial_tick == Some(self.tick) {
                    return Err(EngineError::InvalidCelestialTick);
                }
                if let Some(previous) = self.celestial_state
                    && state.tdb_seconds_since_j2000() <= previous.tdb_seconds_since_j2000()
                {
                    return Err(EngineError::NonMonotoneCelestialTime);
                }
                self.celestial_state = Some(*state);
                self.celestial_tick = Some(self.tick);
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

    fn require_living_organism(&self, organism_id: EntityId) -> Result<(), EngineError> {
        let organism = self
            .organisms
            .get(&organism_id)
            .ok_or(EngineError::UnknownOrganism(organism_id))?;
        if organism.is_alive() {
            Ok(())
        } else {
            Err(EngineError::OrganismAlreadyDead(organism_id))
        }
    }

    fn validate_initial_embodied_patch(
        &self,
        embodied_patch: Option<S2CellId>,
    ) -> Result<(), EngineError> {
        match self
            .configuration
            .as_ref()
            .and_then(WorldConfiguration::embodied_patch_s2_level)
        {
            Some(level) => {
                let patch = embodied_patch.ok_or(EngineError::MissingInitialEmbodiedPatch)?;
                if patch.level() != level {
                    return Err(EngineError::EmbodiedPatchLevelMismatch {
                        expected: level,
                        actual: patch.level(),
                    });
                }
            }
            None if embodied_patch.is_some() => {
                return Err(EngineError::EmbodiedPatchRequiresFullEarthConfiguration);
            }
            None => {}
        }
        Ok(())
    }

    fn validate_embodied_patch(&self, patch: S2CellId) -> Result<(), EngineError> {
        let expected = self
            .configuration
            .as_ref()
            .and_then(WorldConfiguration::embodied_patch_s2_level)
            .ok_or(EngineError::EmbodiedPatchRequiresFullEarthConfiguration)?;
        if patch.level() != expected {
            return Err(EngineError::EmbodiedPatchLevelMismatch {
                expected,
                actual: patch.level(),
            });
        }
        Ok(())
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
        if self.uses_celestial_driver()
            && self.tick != SimTick::ZERO
            && self.celestial_tick != Some(self.tick)
        {
            return Err(EngineError::MissingCelestialState(self.tick));
        }
        for (id, organism) in &self.organisms {
            if id != &organism.organism_id {
                return Err(EngineError::OrganismKeyMismatch(*id));
            }
            organism.species.validate()?;
            self.validate_initial_embodied_patch(organism.embodied_patch)?;
            if organism
                .parent_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            {
                return Err(EngineError::NonCanonicalParentOrder);
            }
        }
        match (self.partition_profile(), &self.partition_schedule) {
            (None, None) => {}
            (None, Some(_)) => {
                return Err(EngineError::PartitionScheduleState(
                    "bounded state unexpectedly contains a partition schedule".to_owned(),
                ));
            }
            (Some(_), None) => {
                return Err(EngineError::PartitionScheduleState(
                    "full-Earth state has no durable partition schedule".to_owned(),
                ));
            }
            (Some((partition_level, _)), Some(actual)) => {
                let expected = self.expected_partition_schedule(partition_level)?;
                if actual != &expected {
                    return Err(EngineError::PartitionScheduleState(
                        "partition schedule does not exactly cover every living embodied organism"
                            .to_owned(),
                    ));
                }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    partition_schedule: Option<&'a PartitionSchedule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    celestial_state: Option<CelestialState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    celestial_tick: Option<SimTick>,
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
        let snapshot_schema_version = if state.uses_celestial_driver() {
            CELESTIAL_DRIVER_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_organism_execution_kernel() && state.partition_schedule.is_some() {
            SCHEDULED_CAUSAL_SNAPSHOT_SCHEMA_VERSION
        } else if state
            .configuration
            .as_ref()
            .is_some_and(WorldConfiguration::is_provisional_execution)
        {
            PROVISIONAL_WORLD_SNAPSHOT_SCHEMA_VERSION
        } else if state.partition_schedule.is_some() {
            PARTITIONED_EXECUTION_SNAPSHOT_SCHEMA_VERSION
        } else if state
            .configuration
            .as_ref()
            .and_then(WorldConfiguration::embodied_patch_s2_level)
            .is_some()
        {
            EMBODIED_POSITION_SNAPSHOT_SCHEMA_VERSION
        } else if state.configuration.is_some() {
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
            LEGACY_SNAPSHOT_SCHEMA_VERSION
                | SNAPSHOT_SCHEMA_VERSION
                | EMBODIED_POSITION_SNAPSHOT_SCHEMA_VERSION
                | PARTITIONED_EXECUTION_SNAPSHOT_SCHEMA_VERSION
                | PROVISIONAL_WORLD_SNAPSHOT_SCHEMA_VERSION
                | SCHEDULED_CAUSAL_SNAPSHOT_SCHEMA_VERSION
                | CELESTIAL_DRIVER_SNAPSHOT_SCHEMA_VERSION
        ) {
            return Err(EngineError::UnsupportedSnapshotSchema(
                self.snapshot_schema_version,
            ));
        }
        let expected_schema_version = if self.state.uses_celestial_driver() {
            CELESTIAL_DRIVER_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_organism_execution_kernel()
            && self.state.partition_schedule.is_some()
        {
            SCHEDULED_CAUSAL_SNAPSHOT_SCHEMA_VERSION
        } else if self
            .state
            .configuration
            .as_ref()
            .is_some_and(WorldConfiguration::is_provisional_execution)
        {
            PROVISIONAL_WORLD_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.partition_schedule.is_some() {
            PARTITIONED_EXECUTION_SNAPSHOT_SCHEMA_VERSION
        } else if self
            .state
            .configuration
            .as_ref()
            .and_then(WorldConfiguration::embodied_patch_s2_level)
            .is_some()
        {
            EMBODIED_POSITION_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.configuration.is_some() {
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
        let is_configured = state.configuration.is_some() || configures_world;
        let configures_provisional_world = batch.events.iter().any(|record| {
            matches!(
                &record.event,
                DomainEvent::WorldConfigured { configuration }
                    if configuration.is_provisional_execution()
            )
        });
        let configures_embodied_world = batch.events.iter().any(|record| {
            matches!(
                &record.event,
                DomainEvent::WorldConfigured { configuration }
                    if configuration.embodied_patch_s2_level().is_some()
            )
        });
        let expected_schema = if state.uses_celestial_driver() {
            CELESTIAL_STATE_EVENT_SCHEMA_VERSION
        } else if state.uses_organism_execution_kernel()
            && (state
                .configuration
                .as_ref()
                .and_then(WorldConfiguration::embodied_patch_s2_level)
                .is_some()
                || configures_embodied_world)
        {
            SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION
        } else if state
            .configuration
            .as_ref()
            .is_some_and(WorldConfiguration::is_provisional_execution)
            || configures_provisional_world
        {
            PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION
        } else if state
            .configuration
            .as_ref()
            .and_then(WorldConfiguration::embodied_patch_s2_level)
            .is_some()
            || configures_embodied_world
        {
            EMBODIED_POSITION_EVENT_SCHEMA_VERSION
        } else if is_configured {
            EVENT_SCHEMA_VERSION
        } else {
            LEGACY_EVENT_SCHEMA_VERSION
        };
        let valid_schema = if expected_schema == CELESTIAL_STATE_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == CELESTIAL_STATE_EVENT_SCHEMA_VERSION
        } else if expected_schema == SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION
        } else if expected_schema == PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION
        } else if expected_schema == EMBODIED_POSITION_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == EMBODIED_POSITION_EVENT_SCHEMA_VERSION
        } else if expected_schema == EVENT_SCHEMA_VERSION {
            matches!(
                batch.event_schema_version,
                CONFIGURED_EVENT_SCHEMA_VERSION | EVENT_SCHEMA_VERSION
            )
        } else {
            batch.event_schema_version == LEGACY_EVENT_SCHEMA_VERSION
        };
        if !valid_schema {
            return Err(EngineError::BatchEventSchemaMismatch {
                expected: expected_schema,
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
    #[error("partition event budget exceeded for {partition:?}: {actual} > {maximum}")]
    PartitionEventBudgetExceeded {
        partition: Option<S2CellId>,
        actual: u64,
        maximum: u64,
    },
    #[error("partition schedule state is inconsistent: {0}")]
    PartitionScheduleState(String),
    #[error("a durable embodied patch requires a full-Earth configuration")]
    EmbodiedPatchRequiresFullEarthConfiguration,
    #[error("full-Earth initial organisms require an embodied patch")]
    MissingInitialEmbodiedPatch,
    #[error("organism {0} has no durable embodied patch")]
    MissingEmbodiedPatch(EntityId),
    #[error("embodied patch level must be {expected}, found {actual}")]
    EmbodiedPatchLevelMismatch { expected: u8, actual: u8 },
    #[error("organism {0} is not located at the movement event's declared source patch")]
    UnexpectedEmbodiedPatch(EntityId),
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
    #[error("organism {0} has an invalid age transition")]
    InvalidAgeTransition(EntityId),
    #[error("organism {0} age tick overflowed")]
    AgeOverflow(EntityId),
    #[error("ruleset-three ticks require one source-backed celestial state")]
    CelestialStateRequired,
    #[error("this ruleset does not accept source-backed celestial states")]
    CelestialStateUnsupported,
    #[error("a celestial state must occur exactly once after a nonzero tick advances")]
    InvalidCelestialTick,
    #[error("celestial source time must strictly advance")]
    NonMonotoneCelestialTime,
    #[error("ruleset-three state at tick {0} has no celestial source state")]
    MissingCelestialState(SimTick),
    #[error("embodied event is invalid: {0}")]
    InvalidEmbodiedEvent(String),
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
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error(transparent)]
    S2(#[from] S2CellIdError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use world_domain::{
        CapacityExhaustionPolicy, CartesianMillimetres, CelestialState, EarthResolutionLevels,
        FullEarthGrid, PartitionedExecution, PerceptionChannel, PersonRepresentation,
        PrimitiveActionKind, PropertyReading, ProvisionalWorldCompositionReference, S2Projection,
        SchedulerKind, SituatedPerception, SpatialGrid, TdbSecondsSinceJ2000,
        WorldDataBundleReference, WorldSeed, WorldStatus,
    };

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
            embodied_patch: None,
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

    fn full_earth_configuration() -> WorldConfiguration {
        WorldConfiguration::new_full_earth(
            300,
            FullEarthGrid {
                physics_crs_epsg: 4_978,
                catalog_crs_epsg: 4_979,
                vertical_crs_epsg: 3_855,
                s2_definition_url: "https://s2geometry.io/devguide/s2cell_hierarchy".to_owned(),
                s2_library_revision: "0123456789abcdef".to_owned(),
                s2_definition_hash: Digest::sha256(b"engine S2 fixture"),
                s2_projection: S2Projection::Quadratic,
                levels: EarthResolutionLevels {
                    planetary_aggregate: 10,
                    regional_ecology: 14,
                    active_landscape: 18,
                    embodied_patch: 23,
                },
                refinement_policy_version: 1,
            },
            WorldDataBundleReference::new(
                2,
                "full-earth-engine-test",
                "0.1.0",
                Digest::sha256(b"full-Earth engine data"),
                "https://data.atinycivilization.com/full-earth-engine-test/0.1.0.json",
                "CC-BY-4.0",
            )
            .expect("valid full-Earth bundle reference"),
            PartitionedExecution {
                scheduler_schema_version: 1,
                scheduler: SchedulerKind::DeterministicEventQueue,
                partition_s2_level: 10,
                person_representation: PersonRepresentation::DurableIndividuals,
                capacity_exhaustion: CapacityExhaustionPolicy::PauseAtCommittedBoundary,
                max_events_per_partition_transition: 10_000,
            },
        )
        .expect("valid full-Earth configuration")
    }

    fn provisional_full_earth_configuration() -> WorldConfiguration {
        let admitted = full_earth_configuration();
        WorldConfiguration::new_provisional_full_earth(
            300,
            admitted.full_earth_grid().expect("full-Earth grid").clone(),
            ProvisionalWorldCompositionReference::new(
                1,
                "full-earth-breadth-first",
                "0.1.0",
                Digest::sha256(b"provisional composition"),
            )
            .expect("valid provisional composition reference"),
            admitted
                .partitioned_execution()
                .expect("partitioned execution")
                .clone(),
        )
        .expect("valid provisional full-Earth configuration")
    }

    fn full_earth_person(world_id: WorldId) -> InitialOrganism {
        let mut person = initial_person(world_id);
        let patch: S2CellId = "0000000000004000".parse().expect("valid L23 S2 cell");
        assert_eq!(patch.level(), 23);
        person.embodied_patch = Some(patch);
        person
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
    fn embodied_perceptions_and_actions_are_replayable_and_never_public_mechanisms() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                world_configuration(),
                vec![initial_person(manifest.world_id)],
            )
            .expect("configured genesis");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("configured genesis batch");
        assert_eq!(genesis.event_schema_version, EVENT_SCHEMA_VERSION);
        let person_id = initial_person(manifest.world_id).organism_id;
        let perception = running
            .plan_perception(
                person_id,
                SituatedPerception {
                    subject_id: None,
                    readings: vec![PropertyReading {
                        channel: PerceptionChannel::Touch,
                        property_code: "surface_roughness".to_owned(),
                        quantized_value: 7,
                        uncertainty: 1,
                    }],
                },
            )
            .expect("label-free perception");
        let (after_perception, perception_batch) = running
            .commit(EventSequence::new(2), genesis.batch_hash, perception)
            .expect("perception batch");
        let action = after_perception
            .plan_action(
                person_id,
                PrimitiveAction {
                    kind: PrimitiveActionKind::ApplyForce,
                    target_id: None,
                    intensity: 3,
                },
            )
            .expect("primitive action");
        let (after_action, action_batch) = after_perception
            .commit(EventSequence::new(3), perception_batch.batch_hash, action)
            .expect("action batch");
        let replayed = replay(manifest, &[genesis, perception_batch, action_batch])
            .expect("embodied history replays");
        assert_eq!(
            replayed.state.state_hash().expect("replay hash"),
            after_action.state_hash().expect("live hash")
        );
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
        configuration.execution = world_domain::ExecutionScale::SingleTransition {
            max_events_per_transition: 2,
        };
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
    fn full_earth_genesis_ticks_and_replays_with_a_durable_partition_schedule() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                full_earth_configuration(),
                vec![full_earth_person(manifest.world_id)],
            )
            .expect("full-Earth genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("full-Earth genesis commit");
        assert_eq!(running.scheduled_work_count(), 1);
        assert_eq!(
            Snapshot::new(running.clone(), genesis.sequence, genesis.batch_hash)
                .expect("partitioned snapshot")
                .snapshot_schema_version,
            SCHEDULED_CAUSAL_SNAPSHOT_SCHEMA_VERSION
        );

        let tick_events = running.plan_next_tick().expect("partitioned tick plan");
        assert!(matches!(
            tick_events.as_slice(),
            [
                DomainEvent::TickAdvanced { .. },
                DomainEvent::OrganismAgeAdvanced {
                    from_age_ticks: 0,
                    to_age_ticks: 1,
                    ..
                }
            ]
        ));
        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("partitioned tick commit");
        assert_eq!(after_tick.tick(), SimTick::new(1));
        assert_eq!(after_tick.scheduled_work_count(), 1);
        let replayed = replay(manifest, &[genesis, tick]).expect("partitioned replay");
        assert_eq!(replayed.state, after_tick);
    }

    #[test]
    fn celestial_ruleset_requires_and_replays_one_source_state_per_tick() {
        let mut manifest = manifest();
        manifest.ruleset_version = CELESTIAL_DRIVER_RULESET_VERSION;
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                full_earth_configuration(),
                vec![full_earth_person(manifest.world_id)],
            )
            .expect("celestial full-Earth genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("celestial full-Earth genesis commit");

        assert!(matches!(
            running.plan_next_tick(),
            Err(EngineError::CelestialStateRequired)
        ));
        let celestial = CelestialState::new(
            TdbSecondsSinceJ2000::new(123),
            CartesianMillimetres::new(1, 2, 3),
            CartesianMillimetres::new(4, 5, 6),
        );
        let tick_events = running
            .plan_next_tick_with_celestial(celestial)
            .expect("celestial tick plan");
        assert!(matches!(
            tick_events.last(),
            Some(DomainEvent::CelestialStateRecorded { state }) if *state == celestial
        ));
        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("celestial tick commit");
        assert_eq!(after_tick.celestial_state(), Some(celestial));
        assert_eq!(
            tick.event_schema_version,
            CELESTIAL_STATE_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            Snapshot::new(after_tick.clone(), tick.sequence, tick.batch_hash)
                .expect("celestial snapshot")
                .snapshot_schema_version,
            CELESTIAL_DRIVER_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(
            replay(manifest, &[genesis, tick])
                .expect("celestial replay")
                .state,
            after_tick
        );
    }

    #[test]
    fn ruleset_one_full_earth_history_keeps_its_empty_schedule_and_schema() {
        let mut manifest = manifest();
        manifest.ruleset_version = LEGACY_RULESET_VERSION;
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                full_earth_configuration(),
                vec![full_earth_person(manifest.world_id)],
            )
            .expect("legacy full-Earth genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("legacy full-Earth genesis commit");
        assert_eq!(
            genesis.event_schema_version,
            EMBODIED_POSITION_EVENT_SCHEMA_VERSION
        );
        assert_eq!(running.scheduled_work_count(), 0);

        let tick_events = running.plan_next_tick().expect("legacy tick plan");
        assert!(matches!(
            tick_events.as_slice(),
            [DomainEvent::TickAdvanced { .. }]
        ));
        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("legacy tick commit");
        assert_eq!(after_tick.scheduled_work_count(), 0);
        assert_eq!(
            replay(manifest, &[genesis, tick])
                .expect("legacy replay")
                .state,
            after_tick
        );
    }

    #[test]
    fn provisional_full_earth_history_has_distinct_event_snapshot_and_hash_schemas() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let configuration = provisional_full_earth_configuration();
        let genesis_events = initial
            .plan_configured_genesis(
                configuration.clone(),
                vec![full_earth_person(manifest.world_id)],
            )
            .expect("provisional genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("provisional genesis commit");

        assert_eq!(
            genesis.event_schema_version,
            SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION
        );
        assert_eq!(running.configuration(), Some(&configuration));
        assert_eq!(
            running.state_hash_schema_version(),
            SCHEDULED_CAUSAL_STATE_HASH_SCHEMA_VERSION
        );
        let snapshot = Snapshot::new(running.clone(), genesis.sequence, genesis.batch_hash)
            .expect("provisional snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            SCHEDULED_CAUSAL_SNAPSHOT_SCHEMA_VERSION
        );

        let tick_events = running.plan_next_tick().expect("provisional tick plan");
        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("provisional tick commit");
        assert_eq!(
            tick.event_schema_version,
            SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION
        );
        let replayed = replay(manifest, &[genesis, tick]).expect("provisional replay");
        assert_eq!(replayed.state, after_tick);
        assert_eq!(replayed.state.tick(), SimTick::new(1));
        assert_eq!(replayed.state.scheduled_work_count(), 1);
    }

    #[test]
    fn manual_full_earth_events_cannot_bypass_embodied_patch_requirement() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let person = initial_person(manifest.world_id);
        assert!(matches!(
            initial.commit(
                EventSequence::new(1),
                Digest::ZERO,
                vec![
                    DomainEvent::WorldStarted { manifest },
                    DomainEvent::WorldConfigured {
                        configuration: full_earth_configuration(),
                    },
                    DomainEvent::OrganismInitialized {
                        organism_id: person.organism_id,
                        species: person.species,
                        role: person.role,
                        birth_category: person.birth_category,
                        initial_age_ticks: person.initial_age_ticks,
                        location_id: person.location_id,
                        embodied_patch: None,
                    },
                ],
            ),
            Err(EngineError::MissingInitialEmbodiedPatch)
        ));
    }

    #[test]
    fn full_earth_state_cannot_drop_its_durable_schedule() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let events = initial
            .plan_configured_genesis(
                full_earth_configuration(),
                vec![full_earth_person(manifest.world_id)],
            )
            .expect("full-Earth genesis plan");
        let (mut running, _) = initial
            .commit(EventSequence::new(1), Digest::ZERO, events)
            .expect("full-Earth genesis");
        running.partition_schedule = None;
        assert!(matches!(
            running.validate(),
            Err(EngineError::PartitionScheduleState(_))
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
