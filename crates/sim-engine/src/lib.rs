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
    ACTION_LEARNING_EVENT_SCHEMA_VERSION, ACTION_VALUE_MAX, ACTION_VALUE_MIN,
    ACTION_VALUE_STATE_SCHEMA_VERSION, ActionValueState, BODILY_REGULATION_EVENT_SCHEMA_VERSION,
    BODY_PROVENANCE_EVENT_SCHEMA_VERSION, BirthCategory, BodilyNeedState, BodilyRegulationState,
    CELESTIAL_STATE_EVENT_SCHEMA_VERSION, CONFIGURED_EVENT_SCHEMA_VERSION, CanonicalHashError,
    CelestialState, DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION, DeathCause, Digest, DomainEvent,
    EMBODIED_POSITION_EVENT_SCHEMA_VERSION, EVENT_SCHEMA_VERSION, EntityId, EventBatch,
    EventBatchError, EventSequence, ExecutionScale, GeographicRoutingError,
    LEGACY_EVENT_SCHEMA_VERSION, MATERIAL_HANDLING_EVENT_SCHEMA_VERSION,
    MATERIAL_INGESTION_EVENT_SCHEMA_VERSION, MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION,
    MaterialIdentity, MetabolicRateCommitment, OralTransferCommitment, OrganismRole,
    PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION, PerceptionChannel, PhysiologicalRegulationCommitment,
    PrimitiveAction, PrimitiveActionKind, PropertyReading, S2CellId, S2CellIdError,
    SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION, SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION,
    SequenceOverflow, SimTick, SituatedPerception, SpeciesIdentity, SpeciesIdentityError,
    TimeOverflow, WorldConfiguration, WorldConfigurationError, WorldId, WorldManifest, WorldStatus,
    s2_edge_neighbors,
};

/// Ruleset one has the original empty full-Earth execution schedule.
pub const LEGACY_RULESET_VERSION: u32 = 1;
/// Ruleset two adds the executable per-organism barrier while preserving
/// ruleset-one replay byte-for-byte.
pub const ORGANISM_EXECUTION_RULESET_VERSION: u32 = 2;
/// Baseline executable ruleset used by the non-production proof fixture. Newly
/// initialized provisional full-Earth worlds select their own latest driver.
pub const RULESET_VERSION: u32 = ORGANISM_EXECUTION_RULESET_VERSION;
/// Ruleset three requires one source-backed celestial input per tick.
pub const CELESTIAL_DRIVER_RULESET_VERSION: u32 = 3;
/// Ruleset four makes the existing body-owned perception and primitive-action
/// contracts execute at the deterministic partition barrier. It is deliberately an
/// integration slice, not a claim that a scientifically admitted ecology exists.
pub const EMBODIED_ACTIVITY_RULESET_VERSION: u32 = 4;
/// Ruleset five admits only source-pinned local physical readings, not weather or
/// ecological conclusions, into the organism execution barrier.
pub const LOCAL_ENVIRONMENT_RULESET_VERSION: u32 = 5;
/// Ruleset six resolves a closed-grammar move into one adjacent S2 patch.
pub const RESOLVED_MOVEMENT_RULESET_VERSION: u32 = 6;
/// Ruleset seven makes bounded direct perceptions durable internal state. It is the
/// substrate for a later deterministic policy and is not an observer projection.
pub const PERSISTENT_PERCEPTION_RULESET_VERSION: u32 = 7;
/// Ruleset eight resolves neutral grasp and release actions against local material.
pub const MATERIAL_HANDLING_RULESET_VERSION: u32 = 8;
/// Ruleset nine delivers neutral emitted signals to living same-patch recipients.
pub const SIGNAL_PROPAGATION_RULESET_VERSION: u32 = 9;
/// Ruleset ten integrates committed metabolic and exposure parameters into bodily
/// pressures and neutral mechanical mortality.
pub const BODILY_REGULATION_RULESET_VERSION: u32 = 10;
/// Ruleset eleven replaces the four-phase integration cadence with a seeded,
/// situated, need-responsive baseline action policy.
pub const DETERMINISTIC_POLICY_RULESET_VERSION: u32 = 11;
/// Ruleset twelve resolves a held material's source-pinned species response into an
/// exact mass transfer and energy/hydration recovery without exposing that profile to
/// the action policy.
pub const MATERIAL_INGESTION_RULESET_VERSION: u32 = 12;
/// Ruleset thirteen records bounded associations between each primitive action and
/// the organism's own total bodily-pressure change, then feeds only that association
/// back into future action weights.
pub const ACTION_LEARNING_RULESET_VERSION: u32 = 13;
pub const LEGACY_SNAPSHOT_SCHEMA_VERSION: u16 = 1;
pub const SNAPSHOT_SCHEMA_VERSION: u16 = 2;
pub const EMBODIED_POSITION_SNAPSHOT_SCHEMA_VERSION: u16 = 3;
pub const PARTITIONED_EXECUTION_SNAPSHOT_SCHEMA_VERSION: u16 = 4;
pub const PROVISIONAL_WORLD_SNAPSHOT_SCHEMA_VERSION: u16 = 5;
pub const SCHEDULED_CAUSAL_SNAPSHOT_SCHEMA_VERSION: u16 = 6;
pub const CELESTIAL_DRIVER_SNAPSHOT_SCHEMA_VERSION: u16 = 7;
pub const BODY_PROVENANCE_SNAPSHOT_SCHEMA_VERSION: u16 = 8;
pub const PERCEPTION_MEMORY_SNAPSHOT_SCHEMA_VERSION: u16 = 9;
pub const MATERIAL_INSTANCE_SNAPSHOT_SCHEMA_VERSION: u16 = 10;
pub const MATERIAL_HANDLING_SNAPSHOT_SCHEMA_VERSION: u16 = 11;
pub const SIGNAL_PROPAGATION_SNAPSHOT_SCHEMA_VERSION: u16 = 12;
pub const BODILY_REGULATION_SNAPSHOT_SCHEMA_VERSION: u16 = 13;
pub const DETERMINISTIC_POLICY_SNAPSHOT_SCHEMA_VERSION: u16 = 14;
pub const MATERIAL_INGESTION_SNAPSHOT_SCHEMA_VERSION: u16 = 15;
pub const ACTION_LEARNING_SNAPSHOT_SCHEMA_VERSION: u16 = 16;
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
const BODY_PROVENANCE_STATE_HASH_SCHEMA_VERSION: u16 = 8;
const PERCEPTION_MEMORY_STATE_HASH_SCHEMA_VERSION: u16 = 9;
const MATERIAL_INSTANCE_STATE_HASH_SCHEMA_VERSION: u16 = 10;
const MATERIAL_HANDLING_STATE_HASH_SCHEMA_VERSION: u16 = 11;
const SIGNAL_PROPAGATION_STATE_HASH_SCHEMA_VERSION: u16 = 12;
const BODILY_REGULATION_STATE_HASH_SCHEMA_VERSION: u16 = 13;
const DETERMINISTIC_POLICY_STATE_HASH_SCHEMA_VERSION: u16 = 14;
const MATERIAL_INGESTION_STATE_HASH_SCHEMA_VERSION: u16 = 15;
const ACTION_LEARNING_STATE_HASH_SCHEMA_VERSION: u16 = 16;
const MAX_PERCEPTION_MEMORY_ENTRIES: usize = 256;

/// A tiny deterministic motor cadence used only by the ruleset-four integration
/// driver. It creates no cultural interpretation and does not claim a metabolic or
/// ecological model; the persistent body/effect rules will replace it by species.
fn motor_phase(organism_id: EntityId, age_ticks: u64) -> u16 {
    ((organism_id
        .as_uuid()
        .as_u128()
        .wrapping_add(u128::from(age_ticks)))
        % 4) as u16
}

fn motor_action_for_phase(phase: u16) -> PrimitiveActionKind {
    match phase {
        0 => PrimitiveActionKind::Rest,
        1 => PrimitiveActionKind::Orient,
        2 => PrimitiveActionKind::Reach,
        _ => PrimitiveActionKind::EmitSignal,
    }
}

fn normal_year_phase(tick: SimTick, tick_duration_seconds: u32) -> usize {
    const NORMAL_PHASE_SECONDS: u64 = 30 * 86_400;
    let elapsed_seconds = tick.get().saturating_mul(u64::from(tick_duration_seconds));
    ((elapsed_seconds / NORMAL_PHASE_SECONDS) % 12) as usize
}

fn normalized_pressure_intensity(load: u64, capacity: u64) -> Result<u16, EngineError> {
    if capacity == 0 {
        return Err(EngineError::PhysiologicalArithmetic(
            "pressure capacity is zero".to_owned(),
        ));
    }
    if load > capacity {
        return Err(EngineError::PhysiologicalArithmetic(
            "pressure load exceeds capacity".to_owned(),
        ));
    }
    let intensity = u128::from(load)
        .checked_mul(u128::from(u16::MAX))
        .ok_or_else(|| {
            EngineError::PhysiologicalArithmetic("pressure numerator overflowed".to_owned())
        })?
        / u128::from(capacity);
    u16::try_from(intensity).map_err(|_| {
        EngineError::PhysiologicalArithmetic("pressure intensity exceeded u16".to_owned())
    })
}

fn capacity_product(left: u64, right: u64, label: &str) -> Result<u64, EngineError> {
    left.checked_mul(right)
        .ok_or_else(|| EngineError::PhysiologicalArithmetic(format!("{label} capacity overflowed")))
}

fn add_load(current: u64, amount: u128, capacity: u64) -> Result<u64, EngineError> {
    let next = u128::from(current)
        .checked_add(amount)
        .ok_or_else(|| EngineError::PhysiologicalArithmetic("body load overflowed".to_owned()))?
        .min(u128::from(capacity));
    u64::try_from(next)
        .map_err(|_| EngineError::PhysiologicalArithmetic("body load exceeds u64".to_owned()))
}

fn integrate_load_with_recovery(
    current: u64,
    exposure: u128,
    recovery: u128,
    capacity: u64,
) -> Result<u64, EngineError> {
    let exposed = u128::from(current)
        .checked_add(exposure)
        .ok_or_else(|| EngineError::PhysiologicalArithmetic("body load overflowed".to_owned()))?;
    let integrated = exposed.saturating_sub(recovery).min(u128::from(capacity));
    u64::try_from(integrated)
        .map_err(|_| EngineError::PhysiologicalArithmetic("body load exceeds u64".to_owned()))
}

fn subtract_load(current: u64, amount: u128) -> u64 {
    if amount >= u128::from(current) {
        0
    } else {
        current - u64::try_from(amount).expect("amount below a u64 current value")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PolicyCandidate {
    action: PrimitiveAction,
    weight: u32,
}

#[derive(Serialize)]
struct PolicyTargetDraw<'a> {
    policy_version: u16,
    world_seed: u64,
    organism_id: EntityId,
    tick: SimTick,
    age_ticks: u64,
    object_ids: &'a [EntityId],
}

#[derive(Serialize)]
struct PolicyMovementDraw {
    policy_version: u16,
    world_seed: u64,
    organism_id: EntityId,
    tick: SimTick,
    age_ticks: u64,
    from_patch: S2CellId,
}

#[derive(Serialize)]
struct PolicyActionDraw<'a> {
    policy_version: u16,
    world_seed: u64,
    organism_id: EntityId,
    tick: SimTick,
    age_ticks: u64,
    bodily_needs: BodilyNeedState,
    candidates: &'a [PolicyCandidate],
}

fn first_digest_u64(digest: Digest) -> u64 {
    let bytes: [u8; 8] = digest.as_bytes()[..8]
        .try_into()
        .expect("SHA-256 has at least eight bytes");
    u64::from_be_bytes(bytes)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct OralRecovery {
    energy_joules: u64,
    hydration_seconds: u64,
}

fn total_bodily_pressure(needs: BodilyNeedState) -> u64 {
    u64::from(needs.energy_deficit)
        + u64::from(needs.hydration_deficit)
        + u64::from(needs.thermal_discomfort)
        + u64::from(needs.pain)
        + u64::from(needs.fatigue)
}

fn action_outcome_reward(from: BodilyNeedState, to: BodilyNeedState) -> i16 {
    let change = i64::try_from(total_bodily_pressure(from)).expect("pressure fits i64")
        - i64::try_from(total_bodily_pressure(to)).expect("pressure fits i64");
    if change == 0 {
        return 0;
    }
    let magnitude = (change.unsigned_abs().saturating_add(1_023) / 1_024).min(32);
    let signed = i16::try_from(magnitude).expect("reward magnitude is at most 32");
    if change.is_positive() {
        signed
    } else {
        -signed
    }
}

fn learned_candidate_weight(base: u32, value: Option<ActionValueState>) -> u32 {
    let Some(value) = value else {
        return base;
    };
    if value.value > 0 {
        base.saturating_add(u32::from(value.value.unsigned_abs()).div_ceil(8))
    } else if value.value < 0 {
        base.saturating_sub(u32::from(value.value.unsigned_abs()).div_ceil(16))
            .max(1)
    } else {
        base
    }
}

fn decimal_to_millicelsius(value: i64, decimal_places: u8) -> Result<i64, EngineError> {
    if decimal_places <= 3 {
        let factor = 10_i128.pow(u32::from(3 - decimal_places));
        return i64::try_from(i128::from(value).checked_mul(factor).ok_or_else(|| {
            EngineError::PhysiologicalArithmetic("temperature scaling overflowed".to_owned())
        })?)
        .map_err(|_| EngineError::PhysiologicalArithmetic("temperature exceeds i64".to_owned()));
    }
    let divisor = 10_i128.pow(u32::from(decimal_places - 3));
    let value = i128::from(value);
    let quotient = value / divisor;
    let remainder = value % divisor;
    let rounded = if remainder.abs().saturating_mul(2) >= divisor {
        quotient + remainder.signum()
    } else {
        quotient
    };
    i64::try_from(rounded)
        .map_err(|_| EngineError::PhysiologicalArithmetic("temperature exceeds i64".to_owned()))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InitialOrganism {
    pub organism_id: EntityId,
    pub species: SpeciesIdentity,
    pub role: OrganismRole,
    pub birth_category: BirthCategory,
    pub initial_age_ticks: u64,
    pub location_id: Option<EntityId>,
    pub embodied_patch: Option<S2CellId>,
    pub metabolic_rate: Option<MetabolicRateCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physiological_regulation: Option<PhysiologicalRegulationCommitment>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metabolic_rate: Option<MetabolicRateCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    physiological_regulation: Option<PhysiologicalRegulationCommitment>,
    #[serde(default, skip_serializing_if = "BodilyRegulationState::is_clear")]
    bodily_regulation: BodilyRegulationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bodily_regulated_at: Option<SimTick>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    perception_memory: Vec<PerceptionMemoryEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    action_values: Vec<ActionValueState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action_values_updated_at: Option<SimTick>,
    death: Option<DeathRecord>,
}

/// The latest direct reading at one subject/channel/property address. The vector
/// is canonically ordered and bounded, avoiding an unbounded event-log duplicate
/// inside every living organism while retaining the input a later policy needs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PerceptionMemoryEntry {
    subject_id: Option<EntityId>,
    channel: PerceptionChannel,
    property_code: String,
    quantized_value: i32,
    uncertainty: u16,
    observed_at: SimTick,
}

/// A canonical physical instance with a citable real-world material identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaterialInstanceState {
    object_id: EntityId,
    material: MaterialIdentity,
    embodied_patch: S2CellId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    held_by: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remaining_mass_milligrams: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    oral_transfer_profiles: Vec<OralTransferCommitment>,
}

impl MaterialInstanceState {
    #[must_use]
    pub const fn object_id(&self) -> EntityId {
        self.object_id
    }

    #[must_use]
    pub const fn embodied_patch(&self) -> S2CellId {
        self.embodied_patch
    }

    #[must_use]
    pub const fn material(&self) -> &MaterialIdentity {
        &self.material
    }

    #[must_use]
    pub const fn held_by(&self) -> Option<EntityId> {
        self.held_by
    }

    #[must_use]
    pub const fn remaining_mass_milligrams(&self) -> Option<u64> {
        self.remaining_mass_milligrams
    }

    fn is_physically_present(&self) -> bool {
        self.remaining_mass_milligrams != Some(0)
    }
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
    pub fn perception_memory_len(&self) -> usize {
        self.perception_memory.len()
    }

    /// Whether this exact direct-reading address already has durable internal
    /// memory. The value itself remains private to the engine.
    #[must_use]
    pub fn has_perception_memory_at(
        &self,
        subject_id: Option<EntityId>,
        channel: PerceptionChannel,
        property_code: &str,
    ) -> bool {
        self.perception_memory
            .binary_search_by(|entry| {
                perception_memory_key(entry).cmp(&(subject_id, channel, property_code))
            })
            .is_ok()
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

    #[must_use]
    pub const fn bodily_needs(&self) -> BodilyNeedState {
        self.bodily_regulation.needs
    }

    #[must_use]
    pub const fn physiological_regulation(&self) -> Option<&PhysiologicalRegulationCommitment> {
        self.physiological_regulation.as_ref()
    }

    fn action_value(&self, action_kind: PrimitiveActionKind) -> Option<ActionValueState> {
        self.action_values
            .binary_search_by_key(&action_kind, |entry| entry.action_kind)
            .ok()
            .map(|index| self.action_values[index])
    }

    fn age_ticks(&self) -> Option<u64> {
        self.age_ticks
    }
}

fn perception_memory_key(
    entry: &PerceptionMemoryEntry,
) -> (Option<EntityId>, PerceptionChannel, &str) {
    (entry.subject_id, entry.channel, &entry.property_code)
}

fn species_identity_key(species: &SpeciesIdentity) -> (&str, &str, &str, &str) {
    (
        &species.catalog,
        &species.identifier,
        &species.scientific_name,
        &species.source_url,
    )
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EngineState {
    manifest: WorldManifest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    configuration: Option<WorldConfiguration>,
    status: WorldStatus,
    tick: SimTick,
    organisms: BTreeMap<EntityId, OrganismState>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    material_instances: BTreeMap<EntityId, MaterialInstanceState>,
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
            material_instances: BTreeMap::new(),
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
    pub fn material_instances(&self) -> impl ExactSizeIterator<Item = &MaterialInstanceState> {
        self.material_instances.values()
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
        if self.uses_bodily_regulation_driver() {
            let baseline = configuration
                .as_ref()
                .and_then(WorldConfiguration::local_environment_baseline)
                .ok_or(EngineError::MissingLocalEnvironmentForRegulation)?;
            if baseline.air_temperature_unit != "degC" {
                return Err(EngineError::UnsupportedTemperatureUnit(
                    baseline.air_temperature_unit.clone(),
                ));
            }
        }
        events.push(DomainEvent::WorldStarted {
            manifest: self.manifest.clone(),
        });
        if let Some(configuration) = configuration {
            events.push(DomainEvent::WorldConfigured { configuration });
        }
        for organism in initial_organisms {
            if let Some(metabolic_rate) = &organism.metabolic_rate {
                metabolic_rate
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                if metabolic_rate.observed_species != organism.species {
                    return Err(EngineError::InvalidEmbodiedEvent(
                        "metabolic-rate commitment species does not match organism".to_owned(),
                    ));
                }
            }
            if let Some(regulation) = &organism.physiological_regulation {
                regulation
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                if regulation.species != organism.species {
                    return Err(EngineError::InvalidEmbodiedEvent(
                        "physiological-regulation commitment species does not match organism"
                            .to_owned(),
                    ));
                }
            }
            if self.uses_bodily_regulation_driver()
                && (organism.metabolic_rate.is_none()
                    || organism.physiological_regulation.is_none())
            {
                return Err(EngineError::MissingPhysiologicalCommitment(
                    organism.organism_id,
                ));
            }
            if !self.uses_bodily_regulation_driver() && organism.physiological_regulation.is_some()
            {
                return Err(EngineError::PhysiologicalCommitmentUnsupported);
            }
            events.push(DomainEvent::OrganismInitialized {
                organism_id: organism.organism_id,
                species: organism.species,
                role: organism.role,
                birth_category: organism.birth_category,
                initial_age_ticks: organism.initial_age_ticks,
                location_id: organism.location_id,
                embodied_patch: organism.embodied_patch,
                metabolic_rate: organism.metabolic_rate,
                physiological_regulation: organism.physiological_regulation,
            });
        }
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
        let mut preview = self.clone();
        preview.apply_events(&events)?;
        if preview.living_people() == 0 {
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
        let mut events = vec![DomainEvent::OrganismActed {
            organism_id,
            action: action.clone(),
        }];
        if self.uses_material_handling_driver() {
            match action.kind {
                PrimitiveActionKind::Grasp => {
                    let object_id = action.target_id.ok_or(EngineError::MissingActionTarget)?;
                    self.validate_grasp(organism_id, object_id)?;
                    events.push(DomainEvent::MaterialInstanceHeld {
                        object_id,
                        holder_id: organism_id,
                    });
                }
                PrimitiveActionKind::Release => {
                    let object_id = action.target_id.ok_or(EngineError::MissingActionTarget)?;
                    let embodied_patch = self.validate_release(organism_id, object_id)?;
                    events.push(DomainEvent::MaterialInstanceReleased {
                        object_id,
                        holder_id: organism_id,
                        embodied_patch,
                    });
                }
                _ => {}
            }
        }
        if self.uses_signal_propagation_driver() && action.kind == PrimitiveActionKind::EmitSignal {
            events.extend(self.local_signal_perceptions(organism_id, action.intensity)?);
        }
        if self.uses_material_ingestion_driver()
            && action.kind == PrimitiveActionKind::Swallow
            && let Some(object_id) = action.target_id
            && let Some((profile_digest, from_mass_milligrams, transferred_mass_milligrams)) =
                self.resolve_oral_transfer(organism_id, object_id)?
        {
            let to_mass_milligrams = from_mass_milligrams
                .checked_sub(transferred_mass_milligrams)
                .ok_or(EngineError::InvalidMaterialOralTransfer(object_id))?;
            events.push(DomainEvent::MaterialOralPortionTransferred {
                object_id,
                organism_id,
                profile_digest,
                from_mass_milligrams,
                transferred_mass_milligrams,
                to_mass_milligrams,
            });
        }
        Ok(events)
    }

    /// Resolve a signal into label-free local sound observations. BTreeMap iteration
    /// fixes recipient order; the signal carries no word, intent, or learned meaning.
    fn local_signal_perceptions(
        &self,
        source_id: EntityId,
        intensity: u16,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        let source_patch = self
            .organisms
            .get(&source_id)
            .and_then(|organism| organism.embodied_patch)
            .ok_or(EngineError::MissingEmbodiedPatch(source_id))?;
        Ok(self
            .organisms
            .values()
            .filter(|recipient| {
                recipient.organism_id != source_id
                    && recipient.is_alive()
                    && recipient.embodied_patch == Some(source_patch)
            })
            .map(|recipient| DomainEvent::OrganismPerceived {
                organism_id: recipient.organism_id,
                perception: SituatedPerception {
                    subject_id: Some(source_id),
                    readings: vec![PropertyReading {
                        channel: PerceptionChannel::Sound,
                        property_code: "signal_amplitude".to_owned(),
                        quantized_value: i32::from(intensity),
                        uncertainty: 0,
                    }],
                },
            })
            .collect())
    }

    fn deterministic_policy_target(
        &self,
        organism: &OrganismState,
        age_ticks: u64,
        object_ids: &[EntityId],
    ) -> Result<Option<EntityId>, EngineError> {
        if object_ids.is_empty() {
            return Ok(None);
        }
        let digest = Digest::canonical(&PolicyTargetDraw {
            policy_version: 1,
            world_seed: self.manifest.seed.get(),
            organism_id: organism.organism_id,
            tick: self.tick.checked_next()?,
            age_ticks,
            object_ids,
        })?;
        let length = u64::try_from(object_ids.len()).map_err(|_| EngineError::TooManyEvents)?;
        let index = usize::try_from(first_digest_u64(digest) % length)
            .map_err(|_| EngineError::TooManyEvents)?;
        Ok(Some(object_ids[index]))
    }

    fn deterministic_policy_candidates(
        &self,
        organism: &OrganismState,
        age_ticks: u64,
    ) -> Result<Vec<PolicyCandidate>, EngineError> {
        let patch = organism
            .embodied_patch
            .ok_or(EngineError::MissingEmbodiedPatch(organism.organism_id))?;
        let held_objects = self
            .material_instances
            .values()
            .filter(|instance| {
                instance.held_by == Some(organism.organism_id) && instance.is_physically_present()
            })
            .map(|instance| instance.object_id)
            .collect::<Vec<_>>();
        let target = if held_objects.is_empty() {
            let patch_leader = self
                .organisms
                .values()
                .filter(|candidate| candidate.is_alive() && candidate.embodied_patch == Some(patch))
                .map(|candidate| candidate.organism_id)
                .min()
                == Some(organism.organism_id);
            if patch_leader {
                let local_objects = self
                    .material_instances
                    .values()
                    .filter(|instance| {
                        instance.held_by.is_none()
                            && instance.embodied_patch == patch
                            && instance.is_physically_present()
                    })
                    .map(|instance| instance.object_id)
                    .collect::<Vec<_>>();
                self.deterministic_policy_target(organism, age_ticks, &local_objects)?
            } else {
                None
            }
        } else {
            self.deterministic_policy_target(organism, age_ticks, &held_objects)?
        };

        let needs = organism.bodily_regulation.needs;
        let oral_drive = u32::from(
            needs
                .energy_deficit
                .max(needs.hydration_deficit)
                .saturating_div(8_192),
        ) + 1;
        let rest_drive = u32::from(needs.fatigue.saturating_div(4_096)) + 1;
        let mut candidates = vec![
            PolicyCandidate {
                action: PrimitiveAction {
                    kind: PrimitiveActionKind::Move,
                    target_id: None,
                    intensity: 1,
                },
                weight: 2,
            },
            PolicyCandidate {
                action: PrimitiveAction {
                    kind: PrimitiveActionKind::Orient,
                    target_id: None,
                    intensity: 1,
                },
                weight: 2,
            },
            PolicyCandidate {
                action: PrimitiveAction {
                    kind: PrimitiveActionKind::Reach,
                    target_id: target,
                    intensity: 1,
                },
                weight: if target.is_some() { oral_drive } else { 1 },
            },
        ];
        if let Some(target_id) = target {
            if held_objects.is_empty() {
                candidates.push(PolicyCandidate {
                    action: PrimitiveAction {
                        kind: PrimitiveActionKind::Grasp,
                        target_id: Some(target_id),
                        intensity: 1,
                    },
                    weight: oral_drive,
                });
            } else {
                for kind in [
                    PrimitiveActionKind::Release,
                    PrimitiveActionKind::ApplyForce,
                    PrimitiveActionKind::Bite,
                    PrimitiveActionKind::Chew,
                    PrimitiveActionKind::Swallow,
                ] {
                    candidates.push(PolicyCandidate {
                        action: PrimitiveAction {
                            kind,
                            target_id: Some(target_id),
                            intensity: 1,
                        },
                        weight: if matches!(
                            kind,
                            PrimitiveActionKind::Bite
                                | PrimitiveActionKind::Chew
                                | PrimitiveActionKind::Swallow
                        ) {
                            oral_drive
                        } else {
                            1
                        },
                    });
                }
            }
        }
        candidates.extend([
            PolicyCandidate {
                action: PrimitiveAction {
                    kind: PrimitiveActionKind::Rest,
                    target_id: None,
                    intensity: 1,
                },
                weight: rest_drive,
            },
            PolicyCandidate {
                action: PrimitiveAction {
                    kind: PrimitiveActionKind::EmitSignal,
                    target_id: None,
                    intensity: 1,
                },
                weight: 2,
            },
        ]);

        if self.uses_action_learning_driver() {
            for candidate in &mut candidates {
                candidate.weight = learned_candidate_weight(
                    candidate.weight,
                    organism.action_value(candidate.action.kind),
                );
            }
        }

        Ok(candidates)
    }

    fn deterministic_policy_action(
        &self,
        organism: &OrganismState,
        age_ticks: u64,
    ) -> Result<PrimitiveAction, EngineError> {
        let candidates = self.deterministic_policy_candidates(organism, age_ticks)?;
        let needs = organism.bodily_regulation.needs;

        let digest = Digest::canonical(&PolicyActionDraw {
            policy_version: if self.uses_action_learning_driver() {
                2
            } else {
                1
            },
            world_seed: self.manifest.seed.get(),
            organism_id: organism.organism_id,
            tick: self.tick.checked_next()?,
            age_ticks,
            bodily_needs: needs,
            candidates: &candidates,
        })?;
        let total_weight = candidates.iter().try_fold(0_u64, |total, candidate| {
            total
                .checked_add(u64::from(candidate.weight))
                .ok_or(EngineError::TooManyEvents)
        })?;
        let mut roll = first_digest_u64(digest) % total_weight;
        let selected = candidates
            .into_iter()
            .find(|candidate| {
                let weight = u64::from(candidate.weight);
                if roll < weight {
                    true
                } else {
                    roll -= weight;
                    false
                }
            })
            .expect("positive candidate weights cover the deterministic roll");
        let mut action = selected.action;
        action.intensity = u16::from(digest.as_bytes()[8] % 4) + 1;
        Ok(action)
    }

    fn next_action_value(
        &self,
        organism: &OrganismState,
        action_kind: PrimitiveActionKind,
        from: BodilyNeedState,
        to: BodilyNeedState,
    ) -> Result<(Option<ActionValueState>, ActionValueState), EngineError> {
        let prior = organism.action_value(action_kind);
        let observations =
            prior.map_or(Ok(1), |prior| {
                prior.observations.checked_add(1).ok_or(
                    EngineError::ActionValueObservationOverflow(organism.organism_id),
                )
            })?;
        let prior_value = prior.map_or(0_i32, |prior| i32::from(prior.value));
        let value = prior_value
            .saturating_add(i32::from(action_outcome_reward(from, to)))
            .clamp(i32::from(ACTION_VALUE_MIN), i32::from(ACTION_VALUE_MAX));
        let next = ActionValueState {
            value_schema_version: ACTION_VALUE_STATE_SCHEMA_VERSION,
            action_kind,
            observations,
            value: i16::try_from(value).expect("bounded action value fits i16"),
        };
        Ok((prior, next))
    }

    fn next_bodily_regulation(
        &self,
        organism: &OrganismState,
        action: &PrimitiveAction,
        oral_recovery: OralRecovery,
    ) -> Result<BodilyRegulationState, EngineError> {
        let configuration = self.configuration.as_ref().ok_or_else(|| {
            EngineError::PhysiologicalArithmetic(
                "bodily regulation requires a world configuration".to_owned(),
            )
        })?;
        let metabolic_rate =
            organism
                .metabolic_rate
                .as_ref()
                .ok_or(EngineError::MissingPhysiologicalCommitment(
                    organism.organism_id,
                ))?;
        let regulation = organism.physiological_regulation.as_ref().ok_or(
            EngineError::MissingPhysiologicalCommitment(organism.organism_id),
        )?;
        metabolic_rate
            .validate()
            .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
        regulation
            .validate()
            .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;

        let tick_seconds = u128::from(configuration.tick_duration_seconds);
        let power_value = u128::try_from(metabolic_rate.measured_power_value).map_err(|_| {
            EngineError::PhysiologicalArithmetic(
                "metabolic power must be a positive integer".to_owned(),
            )
        })?;
        let power_scale = 10_u64.pow(u32::from(metabolic_rate.measured_power_decimal_places));
        let energy_capacity = capacity_product(
            power_scale,
            regulation.usable_energy_reserve_joules,
            "energy",
        )?;
        let energy_consumed = power_value.checked_mul(tick_seconds).ok_or_else(|| {
            EngineError::PhysiologicalArithmetic("energy consumption overflowed".to_owned())
        })?;
        let energy_recovery = u128::from(oral_recovery.energy_joules)
            .checked_mul(u128::from(power_scale))
            .ok_or_else(|| {
                EngineError::PhysiologicalArithmetic("oral energy recovery overflowed".to_owned())
            })?;
        let energy_load = integrate_load_with_recovery(
            organism.bodily_regulation.energy_load_scaled_joules,
            energy_consumed,
            energy_recovery,
            energy_capacity,
        )?;
        let hydration_load = integrate_load_with_recovery(
            organism.bodily_regulation.hydration_load_seconds,
            tick_seconds,
            u128::from(oral_recovery.hydration_seconds),
            regulation.hydration_failure_seconds,
        )?;
        let fatigue_capacity = capacity_product(
            regulation.fatigue_failure_seconds,
            regulation.fatigue_recovery_seconds,
            "fatigue",
        )?;
        let fatigue_load = if action.kind == PrimitiveActionKind::Rest {
            subtract_load(
                organism.bodily_regulation.fatigue_load_second_squared,
                tick_seconds
                    .checked_mul(u128::from(regulation.fatigue_failure_seconds))
                    .ok_or_else(|| {
                        EngineError::PhysiologicalArithmetic(
                            "fatigue recovery overflowed".to_owned(),
                        )
                    })?,
            )
        } else {
            add_load(
                organism.bodily_regulation.fatigue_load_second_squared,
                tick_seconds
                    .checked_mul(u128::from(regulation.fatigue_recovery_seconds))
                    .ok_or_else(|| {
                        EngineError::PhysiologicalArithmetic(
                            "fatigue exposure overflowed".to_owned(),
                        )
                    })?,
                fatigue_capacity,
            )?
        };

        let baseline = configuration.local_environment_baseline().ok_or_else(|| {
            EngineError::PhysiologicalArithmetic(
                "bodily regulation requires a local temperature baseline".to_owned(),
            )
        })?;
        if baseline.air_temperature_unit != "degC" {
            return Err(EngineError::UnsupportedTemperatureUnit(
                baseline.air_temperature_unit.clone(),
            ));
        }
        let temperature = baseline
            .mean_at_normal_phase(normal_year_phase(
                self.tick,
                configuration.tick_duration_seconds,
            ))
            .map_err(|error| EngineError::PhysiologicalArithmetic(error.to_string()))?;
        let temperature =
            decimal_to_millicelsius(temperature, baseline.air_temperature_decimal_places)?;
        let temperature = i128::from(temperature);
        let minimum = i128::from(regulation.thermoneutral_min_millicelsius);
        let maximum = i128::from(regulation.thermoneutral_max_millicelsius);
        let thermal_excess = if temperature < minimum {
            minimum - temperature
        } else if temperature > maximum {
            temperature - maximum
        } else {
            0
        };
        let thermal_capacity = capacity_product(
            regulation.thermal_failure_millicelsius_seconds,
            regulation.thermal_recovery_seconds,
            "thermal",
        )?;
        let thermal_load = if thermal_excess == 0 {
            subtract_load(
                organism
                    .bodily_regulation
                    .thermal_load_millicelsius_second_squared,
                tick_seconds
                    .checked_mul(u128::from(regulation.thermal_failure_millicelsius_seconds))
                    .ok_or_else(|| {
                        EngineError::PhysiologicalArithmetic(
                            "thermal recovery overflowed".to_owned(),
                        )
                    })?,
            )
        } else {
            let thermal_excess = u128::try_from(thermal_excess).map_err(|_| {
                EngineError::PhysiologicalArithmetic(
                    "thermal exposure cannot be negative".to_owned(),
                )
            })?;
            add_load(
                organism
                    .bodily_regulation
                    .thermal_load_millicelsius_second_squared,
                thermal_excess
                    .checked_mul(tick_seconds)
                    .and_then(|value| {
                        value.checked_mul(u128::from(regulation.thermal_recovery_seconds))
                    })
                    .ok_or_else(|| {
                        EngineError::PhysiologicalArithmetic(
                            "thermal exposure overflowed".to_owned(),
                        )
                    })?,
                thermal_capacity,
            )?
        };

        Ok(BodilyRegulationState {
            energy_load_scaled_joules: energy_load,
            hydration_load_seconds: hydration_load,
            fatigue_load_second_squared: fatigue_load,
            thermal_load_millicelsius_second_squared: thermal_load,
            needs: BodilyNeedState {
                energy_deficit: normalized_pressure_intensity(energy_load, energy_capacity)?,
                hydration_deficit: normalized_pressure_intensity(
                    hydration_load,
                    regulation.hydration_failure_seconds,
                )?,
                thermal_discomfort: normalized_pressure_intensity(thermal_load, thermal_capacity)?,
                pain: organism.bodily_regulation.needs.pain,
                fatigue: normalized_pressure_intensity(fatigue_load, fatigue_capacity)?,
            },
        })
    }

    fn validate_bodily_regulation_state(
        organism: &OrganismState,
        state: BodilyRegulationState,
    ) -> Result<(), EngineError> {
        let metabolic_rate =
            organism
                .metabolic_rate
                .as_ref()
                .ok_or(EngineError::MissingPhysiologicalCommitment(
                    organism.organism_id,
                ))?;
        let regulation = organism.physiological_regulation.as_ref().ok_or(
            EngineError::MissingPhysiologicalCommitment(organism.organism_id),
        )?;
        let power_scale = 10_u64.pow(u32::from(metabolic_rate.measured_power_decimal_places));
        let energy_capacity = capacity_product(
            power_scale,
            regulation.usable_energy_reserve_joules,
            "energy",
        )?;
        let fatigue_capacity = capacity_product(
            regulation.fatigue_failure_seconds,
            regulation.fatigue_recovery_seconds,
            "fatigue",
        )?;
        let thermal_capacity = capacity_product(
            regulation.thermal_failure_millicelsius_seconds,
            regulation.thermal_recovery_seconds,
            "thermal",
        )?;
        let expected = BodilyNeedState {
            energy_deficit: normalized_pressure_intensity(
                state.energy_load_scaled_joules,
                energy_capacity,
            )?,
            hydration_deficit: normalized_pressure_intensity(
                state.hydration_load_seconds,
                regulation.hydration_failure_seconds,
            )?,
            thermal_discomfort: normalized_pressure_intensity(
                state.thermal_load_millicelsius_second_squared,
                thermal_capacity,
            )?,
            pain: state.needs.pain,
            fatigue: normalized_pressure_intensity(
                state.fatigue_load_second_squared,
                fatigue_capacity,
            )?,
        };
        if state.needs != expected {
            return Err(EngineError::InvalidBodilyRegulationState(
                organism.organism_id,
            ));
        }
        Ok(())
    }

    fn regulation_death_cause(needs: BodilyNeedState) -> Option<DeathCause> {
        let mechanism = if needs.hydration_deficit == u16::MAX {
            "bodily_regulation_v1_hydration_failure"
        } else if needs.energy_deficit == u16::MAX {
            "bodily_regulation_v1_energy_failure"
        } else if needs.thermal_discomfort == u16::MAX {
            "bodily_regulation_v1_thermal_failure"
        } else {
            return None;
        };
        Some(DeathCause {
            mechanism: mechanism.to_owned(),
        })
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
        self.validate_event_coupling(&events)?;
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
            material_instances: self.material_instances.values().collect(),
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
                        let mut events = vec![Emission::new(
                            partition.partition(),
                            work.key(),
                            0,
                            DomainEvent::OrganismAgeAdvanced {
                                organism_id: organism.organism_id,
                                from_age_ticks,
                                to_age_ticks,
                            },
                        )];
                        if self.uses_embodied_activity_driver() {
                            let phase = motor_phase(organism.organism_id, to_age_ticks);
                            events.push(Emission::new(
                                partition.partition(),
                                work.key(),
                                1,
                                DomainEvent::OrganismPerceived {
                                    organism_id: organism.organism_id,
                                    perception: SituatedPerception {
                                        subject_id: None,
                                        readings: vec![PropertyReading {
                                            channel: PerceptionChannel::Interoception,
                                            property_code: "body_clock_phase".to_owned(),
                                            quantized_value: i32::from(phase),
                                            uncertainty: 0,
                                        }],
                                    },
                                },
                            ));
                            if self.uses_local_environment_driver() {
                                let configuration =
                                    self.configuration.as_ref().ok_or_else(|| {
                                        EngineError::PartitionScheduleState(
                                            "environmental ruleset has no configuration".to_owned(),
                                        )
                                    })?;
                                let baseline = configuration
                                    .local_environment_baseline()
                                    .ok_or_else(|| {
                                        EngineError::PartitionScheduleState(
                                            "environmental ruleset has no local baseline"
                                                .to_owned(),
                                        )
                                    })?;
                                let temperature = baseline
                                    .mean_at_normal_phase(normal_year_phase(
                                        self.tick,
                                        configuration.tick_duration_seconds,
                                    ))
                                    .map_err(|error| {
                                        EngineError::PartitionScheduleState(error.to_string())
                                    })?;
                                let temperature = i32::try_from(temperature).map_err(|_| {
                                    EngineError::PartitionScheduleState(
                                        "temperature does not fit perception range".to_owned(),
                                    )
                                })?;
                                events.push(Emission::new(
                                    partition.partition(),
                                    work.key(),
                                    2,
                                    DomainEvent::OrganismPerceived {
                                        organism_id: organism.organism_id,
                                        perception: SituatedPerception {
                                            subject_id: None,
                                            readings: vec![PropertyReading {
                                                channel: PerceptionChannel::Touch,
                                                property_code: "temperature".to_owned(),
                                                quantized_value: temperature,
                                                uncertainty: 0,
                                            }],
                                        },
                                    },
                                ));
                            }
                            let action_index: u32 = if self.uses_local_environment_driver() {
                                3
                            } else {
                                2
                            };
                            if self.uses_deterministic_policy_driver() {
                                let action =
                                    self.deterministic_policy_action(organism, to_age_ticks)?;
                                let action_kind = action.kind;
                                let resolved_action =
                                    self.plan_action(organism.organism_id, action)?;
                                for (offset, event) in resolved_action.into_iter().enumerate() {
                                    let offset = u32::try_from(offset)
                                        .map_err(|_| EngineError::TooManyEvents)?;
                                    events.push(Emission::new(
                                        partition.partition(),
                                        work.key(),
                                        action_index
                                            .checked_add(offset)
                                            .ok_or(EngineError::TooManyEvents)?,
                                        event,
                                    ));
                                }
                                if action_kind == PrimitiveActionKind::Move {
                                    let from_patch = organism.embodied_patch.ok_or(
                                        EngineError::MissingEmbodiedPatch(organism.organism_id),
                                    )?;
                                    let direction = usize::try_from(
                                        first_digest_u64(Digest::canonical(&PolicyMovementDraw {
                                            policy_version: 1,
                                            world_seed: self.manifest.seed.get(),
                                            organism_id: organism.organism_id,
                                            tick: self.tick.checked_next()?,
                                            age_ticks: to_age_ticks,
                                            from_patch,
                                        })?) % 4,
                                    )
                                    .expect("direction is in 0..4");
                                    let to_patch = s2_edge_neighbors(from_patch)
                                        .map_err(EngineError::from)?[direction];
                                    let emission_index = u32::try_from(events.len())
                                        .map_err(|_| EngineError::TooManyEvents)?;
                                    events.push(Emission::new(
                                        partition.partition(),
                                        work.key(),
                                        emission_index,
                                        DomainEvent::OrganismMoved {
                                            organism_id: organism.organism_id,
                                            from_patch,
                                            to_patch,
                                        },
                                    ));
                                }
                            } else {
                                let action_kind =
                                    if self.uses_signal_propagation_driver() && phase == 2 {
                                        PrimitiveActionKind::EmitSignal
                                    } else if self.uses_resolved_movement_driver() && phase == 3 {
                                        PrimitiveActionKind::Move
                                    } else {
                                        motor_action_for_phase(phase)
                                    };
                                events.push(Emission::new(
                                    partition.partition(),
                                    work.key(),
                                    action_index,
                                    DomainEvent::OrganismActed {
                                        organism_id: organism.organism_id,
                                        action: PrimitiveAction {
                                            kind: action_kind,
                                            target_id: None,
                                            intensity: 1,
                                        },
                                    },
                                ));
                                if action_kind == PrimitiveActionKind::EmitSignal
                                    && self.uses_signal_propagation_driver()
                                {
                                    for (offset, perception) in self
                                        .local_signal_perceptions(organism.organism_id, 1)?
                                        .into_iter()
                                        .enumerate()
                                    {
                                        let offset = u32::try_from(offset)
                                            .map_err(|_| EngineError::TooManyEvents)?;
                                        let emission_index = action_index
                                            .checked_add(1)
                                            .and_then(|index| index.checked_add(offset))
                                            .ok_or(EngineError::TooManyEvents)?;
                                        events.push(Emission::new(
                                            partition.partition(),
                                            work.key(),
                                            emission_index,
                                            perception,
                                        ));
                                    }
                                } else if action_kind == PrimitiveActionKind::Move {
                                    let from_patch = organism.embodied_patch.ok_or(
                                        EngineError::MissingEmbodiedPatch(organism.organism_id),
                                    )?;
                                    let direction = usize::try_from(
                                        (organism.organism_id.as_uuid().as_u128() >> 2) & 3,
                                    )
                                    .expect("direction fits");
                                    let to_patch = s2_edge_neighbors(from_patch)
                                        .map_err(EngineError::from)?[direction];
                                    events.push(Emission::new(
                                        partition.partition(),
                                        work.key(),
                                        action_index + 1,
                                        DomainEvent::OrganismMoved {
                                            organism_id: organism.organism_id,
                                            from_patch,
                                            to_patch,
                                        },
                                    ));
                                }
                            }
                        }
                        Ok(WorkOutput::new(work.key(), events, Vec::new()))
                    })
                    .collect::<Result<Vec<_>, EngineError>>()?;
                Ok(PartitionOutput::new(partition.partition(), work_outputs))
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let resolved = plan.complete(outputs, maximum_events)?;
        let mut events = resolved
            .emissions()
            .iter()
            .map(|emission| emission.event().clone())
            .collect::<Vec<_>>();
        if self.uses_bodily_regulation_driver() {
            // Regulation is a final causal phase after every organism's sensory and
            // motor emissions. This prevents arbitrary subject-key ordering from
            // making a same-tick signal target appear dead before the signal arrives.
            let mut actions = BTreeMap::new();
            let mut oral_recoveries = BTreeMap::new();
            for event in &events {
                if let DomainEvent::OrganismActed {
                    organism_id,
                    action,
                } = event
                    && actions.insert(*organism_id, action.clone()).is_some()
                {
                    return Err(EngineError::DuplicateScheduledAction(*organism_id));
                }
                if let DomainEvent::MaterialOralPortionTransferred {
                    object_id,
                    organism_id,
                    profile_digest,
                    transferred_mass_milligrams,
                    ..
                } = event
                {
                    let organism = self
                        .organisms
                        .get(organism_id)
                        .ok_or(EngineError::UnknownOrganism(*organism_id))?;
                    let instance = self
                        .material_instances
                        .get(object_id)
                        .ok_or(EngineError::UnknownMaterialInstance(*object_id))?;
                    let profile = instance
                        .oral_transfer_profiles
                        .iter()
                        .find(|profile| {
                            profile.species == organism.species
                                && profile.profile_digest == *profile_digest
                        })
                        .ok_or(EngineError::InvalidMaterialOralTransfer(*object_id))?;
                    if *transferred_mass_milligrams != profile.transfer_mass_milligrams
                        || oral_recoveries
                            .insert(
                                *organism_id,
                                OralRecovery {
                                    energy_joules: profile.recoverable_energy_joules,
                                    hydration_seconds: profile.hydration_recovery_seconds,
                                },
                            )
                            .is_some()
                    {
                        return Err(EngineError::InvalidMaterialOralTransfer(*object_id));
                    }
                }
            }
            let mut deaths = Vec::new();
            for organism in self
                .organisms
                .values()
                .filter(|organism| organism.is_alive())
            {
                let action = actions
                    .get(&organism.organism_id)
                    .ok_or(EngineError::MissingScheduledAction(organism.organism_id))?;
                let to = self.next_bodily_regulation(
                    organism,
                    action,
                    oral_recoveries
                        .get(&organism.organism_id)
                        .copied()
                        .unwrap_or_default(),
                )?;
                events.push(DomainEvent::OrganismNeedsChanged {
                    organism_id: organism.organism_id,
                    from: organism.bodily_regulation,
                    to,
                });
                if self.uses_action_learning_driver() {
                    let (from, to_value) = self.next_action_value(
                        organism,
                        action.kind,
                        organism.bodily_regulation.needs,
                        to.needs,
                    )?;
                    events.push(DomainEvent::OrganismActionValueChanged {
                        organism_id: organism.organism_id,
                        from,
                        to: to_value,
                    });
                }
                if let Some(cause) = Self::regulation_death_cause(to.needs) {
                    deaths.push(DomainEvent::OrganismDied {
                        organism_id: organism.organism_id,
                        cause,
                    });
                }
            }
            events.extend(deaths);
        }
        Ok(events)
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

    fn uses_embodied_activity_driver(&self) -> bool {
        self.manifest.ruleset_version >= EMBODIED_ACTIVITY_RULESET_VERSION
    }

    fn uses_local_environment_driver(&self) -> bool {
        self.manifest.ruleset_version >= LOCAL_ENVIRONMENT_RULESET_VERSION
    }

    fn uses_resolved_movement_driver(&self) -> bool {
        self.manifest.ruleset_version >= RESOLVED_MOVEMENT_RULESET_VERSION
    }

    fn uses_persistent_perception_driver(&self) -> bool {
        self.manifest.ruleset_version >= PERSISTENT_PERCEPTION_RULESET_VERSION
    }

    fn uses_material_handling_driver(&self) -> bool {
        self.manifest.ruleset_version >= MATERIAL_HANDLING_RULESET_VERSION
    }

    fn uses_signal_propagation_driver(&self) -> bool {
        self.manifest.ruleset_version >= SIGNAL_PROPAGATION_RULESET_VERSION
    }

    fn uses_bodily_regulation_driver(&self) -> bool {
        self.manifest.ruleset_version >= BODILY_REGULATION_RULESET_VERSION
    }

    fn uses_deterministic_policy_driver(&self) -> bool {
        self.manifest.ruleset_version >= DETERMINISTIC_POLICY_RULESET_VERSION
    }

    fn uses_material_ingestion_driver(&self) -> bool {
        self.manifest.ruleset_version >= MATERIAL_INGESTION_RULESET_VERSION
    }

    fn uses_action_learning_driver(&self) -> bool {
        self.manifest.ruleset_version >= ACTION_LEARNING_RULESET_VERSION
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
            DomainEvent::MaterialInstanceInitialized { embodied_patch, .. } => {
                Some(*embodied_patch)
            }
            DomainEvent::MaterialInstanceHeld { holder_id, .. } => self
                .organisms
                .get(holder_id)
                .or_else(|| resulting_state.organisms.get(holder_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::MaterialInstanceReleased { embodied_patch, .. } => Some(*embodied_patch),
            DomainEvent::MaterialOralPortionTransferred { organism_id, .. } => self
                .organisms
                .get(organism_id)
                .or_else(|| resulting_state.organisms.get(organism_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::OrganismMoved { to_patch, .. } => Some(*to_patch),
            DomainEvent::OrganismDied { organism_id, .. }
            | DomainEvent::OrganismAgeAdvanced { organism_id, .. }
            | DomainEvent::OrganismNeedsChanged { organism_id, .. }
            | DomainEvent::OrganismActionValueChanged { organism_id, .. }
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

    fn validate_event_coupling(&self, events: &[DomainEvent]) -> Result<(), EngineError> {
        let tick_advanced = events
            .iter()
            .any(|event| matches!(event, DomainEvent::TickAdvanced { .. }));
        let tick_advance_index = events
            .iter()
            .position(|event| matches!(event, DomainEvent::TickAdvanced { .. }));
        if self.uses_action_learning_driver()
            && !tick_advanced
            && let Some(organism_id) = events.iter().find_map(|event| match event {
                DomainEvent::OrganismActed { organism_id, .. } => Some(*organism_id),
                _ => None,
            })
        {
            return Err(EngineError::InvalidActionValueTransition(organism_id));
        }
        for (index, event) in events.iter().enumerate() {
            let DomainEvent::MaterialOralPortionTransferred {
                object_id,
                organism_id,
                ..
            } = event
            else {
                continue;
            };
            if !self.uses_material_ingestion_driver() || !tick_advanced {
                return Err(EngineError::InvalidMaterialOralTransfer(*object_id));
            }
            let matching_actions = events[..index]
                .iter()
                .filter(|prior| {
                    matches!(
                        prior,
                        DomainEvent::OrganismActed { organism_id: actor_id, action }
                            if actor_id == organism_id
                                && action.kind == PrimitiveActionKind::Swallow
                                && action.target_id == Some(*object_id)
                    )
                })
                .count();
            let matching_need_transitions = events[index + 1..]
                .iter()
                .filter(|later| {
                    matches!(
                        later,
                        DomainEvent::OrganismNeedsChanged { organism_id: actor_id, .. }
                            if actor_id == organism_id
                    )
                })
                .count();
            if matching_actions != 1 || matching_need_transitions != 1 {
                return Err(EngineError::InvalidMaterialOralTransfer(*object_id));
            }
        }
        for (index, event) in events.iter().enumerate() {
            if let DomainEvent::OrganismActionValueChanged {
                organism_id,
                from,
                to,
            } = event
            {
                if !self.uses_action_learning_driver() || !tick_advanced {
                    return Err(EngineError::InvalidActionValueTransition(*organism_id));
                }
                let matching_actions = events
                    .iter()
                    .enumerate()
                    .filter_map(|(prior_index, prior)| {
                        matches!(
                            prior,
                            DomainEvent::OrganismActed { organism_id: actor_id, action }
                                if actor_id == organism_id && action.kind == to.action_kind
                        )
                        .then_some(prior_index)
                    })
                    .collect::<Vec<_>>();
                let matching_needs = events
                    .iter()
                    .enumerate()
                    .filter_map(|(prior_index, prior)| {
                        matches!(
                            prior,
                            DomainEvent::OrganismNeedsChanged { organism_id: actor_id, .. }
                                if actor_id == organism_id
                        )
                        .then_some(prior_index)
                    })
                    .collect::<Vec<_>>();
                if matching_actions.len() != 1 || matching_needs.len() != 1 {
                    return Err(EngineError::InvalidActionValueTransition(*organism_id));
                }
                let action_index = matching_actions[0];
                let needs_index = matching_needs[0];
                let Some(tick_index) = tick_advance_index else {
                    return Err(EngineError::InvalidActionValueTransition(*organism_id));
                };
                if !(tick_index < action_index && action_index < needs_index && needs_index < index)
                {
                    return Err(EngineError::InvalidActionValueTransition(*organism_id));
                }
                let DomainEvent::OrganismActed { action, .. } = &events[action_index] else {
                    unreachable!("action index was selected from action events")
                };
                let DomainEvent::OrganismNeedsChanged {
                    from: body_from,
                    to: body_to,
                    ..
                } = &events[needs_index]
                else {
                    unreachable!("needs index was selected from body transitions")
                };
                let organism = self
                    .organisms
                    .get(organism_id)
                    .ok_or(EngineError::UnknownOrganism(*organism_id))?;
                let expected =
                    self.next_action_value(organism, action.kind, body_from.needs, body_to.needs)?;
                if expected.0 != *from || expected.1 != *to {
                    return Err(EngineError::InvalidActionValueTransition(*organism_id));
                }
            }
            if self.uses_action_learning_driver()
                && tick_advanced
                && let DomainEvent::OrganismActed {
                    organism_id,
                    action,
                } = event
            {
                let matching_updates = events[index + 1..]
                    .iter()
                    .filter(|later| {
                        matches!(
                            later,
                            DomainEvent::OrganismActionValueChanged {
                                organism_id: actor_id,
                                to,
                                ..
                            } if actor_id == organism_id && to.action_kind == action.kind
                        )
                    })
                    .count();
                if matching_updates != 1 {
                    return Err(EngineError::InvalidActionValueTransition(*organism_id));
                }
            }
        }
        Ok(())
    }

    fn event_schema_version(&self) -> u16 {
        if self.uses_action_learning_driver() {
            ACTION_LEARNING_EVENT_SCHEMA_VERSION
        } else if self.uses_material_ingestion_driver() {
            MATERIAL_INGESTION_EVENT_SCHEMA_VERSION
        } else if self.uses_deterministic_policy_driver() {
            DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION
        } else if self.uses_bodily_regulation_driver() {
            BODILY_REGULATION_EVENT_SCHEMA_VERSION
        } else if self.uses_signal_propagation_driver() {
            SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION
        } else if self.uses_material_handling_driver() {
            MATERIAL_HANDLING_EVENT_SCHEMA_VERSION
        } else if !self.material_instances.is_empty() {
            MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION
        } else if self.has_metabolic_rate_commitments() {
            BODY_PROVENANCE_EVENT_SCHEMA_VERSION
        } else if self.uses_celestial_driver() {
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

    fn has_metabolic_rate_commitments(&self) -> bool {
        self.organisms
            .values()
            .any(|organism| organism.metabolic_rate.is_some())
    }

    fn has_perception_memory(&self) -> bool {
        self.organisms
            .values()
            .any(|organism| !organism.perception_memory.is_empty())
    }

    fn state_hash_schema_version(&self) -> u16 {
        if self.uses_action_learning_driver() {
            ACTION_LEARNING_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_material_ingestion_driver() {
            MATERIAL_INGESTION_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_deterministic_policy_driver() {
            DETERMINISTIC_POLICY_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_bodily_regulation_driver() {
            BODILY_REGULATION_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_signal_propagation_driver() {
            SIGNAL_PROPAGATION_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_material_handling_driver() {
            MATERIAL_HANDLING_STATE_HASH_SCHEMA_VERSION
        } else if !self.material_instances.is_empty() {
            MATERIAL_INSTANCE_STATE_HASH_SCHEMA_VERSION
        } else if self.has_perception_memory() {
            PERCEPTION_MEMORY_STATE_HASH_SCHEMA_VERSION
        } else if self.has_metabolic_rate_commitments() {
            BODY_PROVENANCE_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_celestial_driver() {
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
                if self.tick != SimTick::ZERO
                    || !self.organisms.is_empty()
                    || !self.material_instances.is_empty()
                {
                    return Err(EngineError::InvalidGenesisState);
                }
                self.status = WorldStatus::Running;
            }
            DomainEvent::WorldConfigured { configuration } => {
                self.require_status(WorldStatus::Running)?;
                if self.configuration.is_some() {
                    return Err(EngineError::WorldAlreadyConfigured);
                }
                if self.tick != SimTick::ZERO
                    || !self.organisms.is_empty()
                    || !self.material_instances.is_empty()
                {
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
                metabolic_rate,
                physiological_regulation,
            } => {
                self.require_status(WorldStatus::Running)?;
                species.validate()?;
                self.validate_initial_embodied_patch(*embodied_patch)?;
                if let Some(metabolic_rate) = metabolic_rate {
                    metabolic_rate
                        .validate()
                        .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                    if metabolic_rate.observed_species != *species {
                        return Err(EngineError::InvalidEmbodiedEvent(
                            "metabolic-rate commitment species does not match organism".to_owned(),
                        ));
                    }
                }
                if self.uses_bodily_regulation_driver()
                    && (metabolic_rate.is_none() || physiological_regulation.is_none())
                {
                    return Err(EngineError::MissingPhysiologicalCommitment(*organism_id));
                }
                if !self.uses_bodily_regulation_driver() && physiological_regulation.is_some() {
                    return Err(EngineError::PhysiologicalCommitmentUnsupported);
                }
                if let Some(regulation) = physiological_regulation {
                    regulation
                        .validate()
                        .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                    if regulation.species != *species {
                        return Err(EngineError::InvalidEmbodiedEvent(
                            "physiological-regulation commitment species does not match organism"
                                .to_owned(),
                        ));
                    }
                }
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
                    metabolic_rate: metabolic_rate.clone(),
                    physiological_regulation: physiological_regulation.clone(),
                    bodily_regulation: BodilyRegulationState::default(),
                    bodily_regulated_at: None,
                    perception_memory: Vec::new(),
                    action_values: Vec::new(),
                    action_values_updated_at: None,
                    death: None,
                })?;
                self.refresh_partition_schedule()?;
            }
            DomainEvent::MaterialInstanceInitialized {
                object_id,
                material,
                embodied_patch,
                initial_mass_milligrams,
                oral_transfer_profiles,
            } => {
                self.require_status(WorldStatus::Running)?;
                material
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                self.validate_embodied_patch(*embodied_patch)?;
                if self
                    .material_instances
                    .insert(
                        *object_id,
                        MaterialInstanceState {
                            object_id: *object_id,
                            material: material.clone(),
                            embodied_patch: *embodied_patch,
                            held_by: None,
                            remaining_mass_milligrams: *initial_mass_milligrams,
                            oral_transfer_profiles: oral_transfer_profiles.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(EngineError::DuplicateMaterialInstance(*object_id));
                }
            }
            DomainEvent::MaterialInstanceHeld {
                object_id,
                holder_id,
            } => {
                self.require_living_organism(*holder_id)?;
                self.validate_grasp(*holder_id, *object_id)?;
                self.material_instances
                    .get_mut(object_id)
                    .expect("validated material instance")
                    .held_by = Some(*holder_id);
            }
            DomainEvent::MaterialInstanceReleased {
                object_id,
                holder_id,
                embodied_patch,
            } => {
                self.require_living_organism(*holder_id)?;
                let expected_patch = self.validate_release(*holder_id, *object_id)?;
                if *embodied_patch != expected_patch {
                    return Err(EngineError::InvalidMaterialReleasePatch(*object_id));
                }
                let instance = self
                    .material_instances
                    .get_mut(object_id)
                    .expect("validated material instance");
                instance.held_by = None;
                instance.embodied_patch = *embodied_patch;
            }
            DomainEvent::MaterialOralPortionTransferred {
                object_id,
                organism_id,
                profile_digest,
                from_mass_milligrams,
                transferred_mass_milligrams,
                to_mass_milligrams,
            } => {
                if !self.uses_material_ingestion_driver() {
                    return Err(EngineError::MaterialIngestionUnsupported);
                }
                self.require_living_organism(*organism_id)?;
                let organism_species = self
                    .organisms
                    .get(organism_id)
                    .expect("living organism presence checked")
                    .species
                    .clone();
                let instance = self
                    .material_instances
                    .get_mut(object_id)
                    .ok_or(EngineError::UnknownMaterialInstance(*object_id))?;
                let profile = instance
                    .oral_transfer_profiles
                    .iter()
                    .find(|profile| {
                        profile.species == organism_species
                            && profile.profile_digest == *profile_digest
                    })
                    .ok_or(EngineError::InvalidMaterialOralTransfer(*object_id))?;
                let expected_to = from_mass_milligrams
                    .checked_sub(*transferred_mass_milligrams)
                    .ok_or(EngineError::InvalidMaterialOralTransfer(*object_id))?;
                if instance.held_by != Some(*organism_id)
                    || instance.remaining_mass_milligrams != Some(*from_mass_milligrams)
                    || *transferred_mass_milligrams != profile.transfer_mass_milligrams
                    || *to_mass_milligrams != expected_to
                {
                    return Err(EngineError::InvalidMaterialOralTransfer(*object_id));
                }
                instance.remaining_mass_milligrams = Some(*to_mass_milligrams);
                if *to_mass_milligrams == 0 {
                    instance.held_by = None;
                }
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
                metabolic_rate,
                physiological_regulation,
            } => {
                self.require_status(WorldStatus::Running)?;
                species.validate()?;
                self.validate_initial_embodied_patch(*embodied_patch)?;
                if let Some(metabolic_rate) = metabolic_rate {
                    metabolic_rate
                        .validate()
                        .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                    if metabolic_rate.observed_species != *species {
                        return Err(EngineError::InvalidEmbodiedEvent(
                            "metabolic-rate commitment species does not match organism".to_owned(),
                        ));
                    }
                }
                if self.uses_bodily_regulation_driver()
                    && (metabolic_rate.is_none() || physiological_regulation.is_none())
                {
                    return Err(EngineError::MissingPhysiologicalCommitment(*organism_id));
                }
                if !self.uses_bodily_regulation_driver() && physiological_regulation.is_some() {
                    return Err(EngineError::PhysiologicalCommitmentUnsupported);
                }
                if let Some(regulation) = physiological_regulation {
                    regulation
                        .validate()
                        .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                    if regulation.species != *species {
                        return Err(EngineError::InvalidEmbodiedEvent(
                            "physiological-regulation commitment species does not match organism"
                                .to_owned(),
                        ));
                    }
                }
                if parent_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
                    return Err(EngineError::NonCanonicalParentOrder);
                }
                if parent_ids.is_empty() {
                    return Err(EngineError::ParentlessBirth);
                }
                if let Some(missing) = parent_ids
                    .iter()
                    .find(|parent_id| !self.organisms.contains_key(parent_id))
                {
                    return Err(EngineError::UnknownParent(*missing));
                }
                if parent_ids.iter().any(|parent_id| {
                    let parent = self
                        .organisms
                        .get(parent_id)
                        .expect("parent presence checked above");
                    parent.species != *species || parent.role != *role
                }) {
                    return Err(EngineError::IncompatibleBirthLineage);
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
                    metabolic_rate: metabolic_rate.clone(),
                    physiological_regulation: physiological_regulation.clone(),
                    bodily_regulation: BodilyRegulationState::default(),
                    bodily_regulated_at: None,
                    perception_memory: Vec::new(),
                    action_values: Vec::new(),
                    action_values_updated_at: None,
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
                let embodied_patch = organism.embodied_patch;
                if let Some(embodied_patch) = embodied_patch {
                    for instance in self.material_instances.values_mut() {
                        if instance.held_by == Some(*organism_id) {
                            instance.held_by = None;
                            instance.embodied_patch = embodied_patch;
                        }
                    }
                }
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
                if self.uses_persistent_perception_driver() {
                    let organism = self
                        .organisms
                        .get_mut(organism_id)
                        .ok_or(EngineError::UnknownOrganism(*organism_id))?;
                    for reading in &perception.readings {
                        let entry = PerceptionMemoryEntry {
                            subject_id: perception.subject_id,
                            channel: reading.channel,
                            property_code: reading.property_code.clone(),
                            quantized_value: reading.quantized_value,
                            uncertainty: reading.uncertainty,
                            observed_at: self.tick,
                        };
                        match organism.perception_memory.binary_search_by(|existing| {
                            perception_memory_key(existing).cmp(&perception_memory_key(&entry))
                        }) {
                            Ok(index) => organism.perception_memory[index] = entry,
                            Err(index) => {
                                if organism.perception_memory.len() >= MAX_PERCEPTION_MEMORY_ENTRIES
                                {
                                    return Err(EngineError::PerceptionMemoryCapacity(
                                        *organism_id,
                                    ));
                                }
                                organism.perception_memory.insert(index, entry);
                            }
                        }
                    }
                }
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
                for instance in self.material_instances.values_mut() {
                    if instance.held_by == Some(*organism_id) {
                        instance.embodied_patch = *to_patch;
                    }
                }
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
            DomainEvent::OrganismNeedsChanged {
                organism_id,
                from,
                to,
            } => {
                if !self.uses_bodily_regulation_driver() {
                    return Err(EngineError::PhysiologicalCommitmentUnsupported);
                }
                self.require_living_organism(*organism_id)?;
                if from == to {
                    return Err(EngineError::InvalidBodilyRegulationState(*organism_id));
                }
                let organism = self
                    .organisms
                    .get(organism_id)
                    .ok_or(EngineError::UnknownOrganism(*organism_id))?;
                if organism.bodily_regulation != *from
                    || organism.bodily_regulated_at == Some(self.tick)
                {
                    return Err(EngineError::InvalidBodilyRegulationTransition(*organism_id));
                }
                Self::validate_bodily_regulation_state(organism, *to)?;
                let organism = self
                    .organisms
                    .get_mut(organism_id)
                    .expect("body presence checked above");
                organism.bodily_regulation = *to;
                organism.bodily_regulated_at = Some(self.tick);
            }
            DomainEvent::OrganismActionValueChanged {
                organism_id,
                from,
                to,
            } => {
                if !self.uses_action_learning_driver() {
                    return Err(EngineError::ActionLearningUnsupported);
                }
                self.require_living_organism(*organism_id)?;
                to.validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                let organism = self
                    .organisms
                    .get_mut(organism_id)
                    .expect("living organism presence checked");
                let expected_observations = match from {
                    Some(from) => from
                        .observations
                        .checked_add(1)
                        .ok_or(EngineError::ActionValueObservationOverflow(*organism_id))?,
                    None => 1,
                };
                if organism.action_values_updated_at == Some(self.tick)
                    || organism.action_value(to.action_kind) != *from
                    || to.observations != expected_observations
                    || from.is_some_and(|from| from.action_kind != to.action_kind)
                {
                    return Err(EngineError::InvalidActionValueTransition(*organism_id));
                }
                match organism
                    .action_values
                    .binary_search_by_key(&to.action_kind, |entry| entry.action_kind)
                {
                    Ok(index) => organism.action_values[index] = *to,
                    Err(index) => organism.action_values.insert(index, *to),
                }
                organism.action_values_updated_at = Some(self.tick);
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

    fn validate_grasp(&self, holder_id: EntityId, object_id: EntityId) -> Result<(), EngineError> {
        let holder_patch = self
            .organisms
            .get(&holder_id)
            .and_then(|holder| holder.embodied_patch)
            .ok_or(EngineError::MissingEmbodiedPatch(holder_id))?;
        let instance = self
            .material_instances
            .get(&object_id)
            .ok_or(EngineError::UnknownMaterialInstance(object_id))?;
        if instance.held_by.is_some() {
            return Err(EngineError::MaterialInstanceAlreadyHeld(object_id));
        }
        if !instance.is_physically_present() {
            return Err(EngineError::MaterialInstanceDepleted(object_id));
        }
        if instance.embodied_patch != holder_patch {
            return Err(EngineError::NonLocalMaterialAction(object_id));
        }
        Ok(())
    }

    fn validate_release(
        &self,
        holder_id: EntityId,
        object_id: EntityId,
    ) -> Result<S2CellId, EngineError> {
        let holder_patch = self
            .organisms
            .get(&holder_id)
            .and_then(|holder| holder.embodied_patch)
            .ok_or(EngineError::MissingEmbodiedPatch(holder_id))?;
        let instance = self
            .material_instances
            .get(&object_id)
            .ok_or(EngineError::UnknownMaterialInstance(object_id))?;
        if instance.held_by != Some(holder_id) {
            return Err(EngineError::MaterialInstanceNotHeldByActor(object_id));
        }
        Ok(holder_patch)
    }

    fn resolve_oral_transfer(
        &self,
        organism_id: EntityId,
        object_id: EntityId,
    ) -> Result<Option<(Digest, u64, u64)>, EngineError> {
        let organism = self
            .organisms
            .get(&organism_id)
            .ok_or(EngineError::UnknownOrganism(organism_id))?;
        let instance = self
            .material_instances
            .get(&object_id)
            .ok_or(EngineError::UnknownMaterialInstance(object_id))?;
        if instance.held_by != Some(organism_id) {
            return Err(EngineError::MaterialInstanceNotHeldByActor(object_id));
        }
        let Some(profile) = instance
            .oral_transfer_profiles
            .iter()
            .find(|profile| profile.species == organism.species)
        else {
            return Ok(None);
        };
        let from_mass_milligrams = instance
            .remaining_mass_milligrams
            .ok_or(EngineError::MissingMaterialMass(object_id))?;
        if from_mass_milligrams < profile.transfer_mass_milligrams {
            return Ok(None);
        }
        Ok(Some((
            profile.profile_digest,
            from_mass_milligrams,
            profile.transfer_mass_milligrams,
        )))
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self.manifest.ruleset_version == 0 {
            return Err(EngineError::ZeroRulesetVersion);
        }
        if let Some(configuration) = &self.configuration {
            configuration.validate()?;
        }
        if self.status == WorldStatus::Initializing
            && (self.tick != SimTick::ZERO
                || !self.organisms.is_empty()
                || !self.material_instances.is_empty())
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
        if self.uses_bodily_regulation_driver() {
            let baseline = self
                .configuration
                .as_ref()
                .and_then(WorldConfiguration::local_environment_baseline)
                .ok_or(EngineError::MissingLocalEnvironmentForRegulation)?;
            if baseline.air_temperature_unit != "degC" {
                return Err(EngineError::UnsupportedTemperatureUnit(
                    baseline.air_temperature_unit.clone(),
                ));
            }
        }
        for (id, organism) in &self.organisms {
            if id != &organism.organism_id {
                return Err(EngineError::OrganismKeyMismatch(*id));
            }
            organism.species.validate()?;
            self.validate_initial_embodied_patch(organism.embodied_patch)?;
            if let Some(metabolic_rate) = &organism.metabolic_rate {
                metabolic_rate
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                if metabolic_rate.observed_species != organism.species {
                    return Err(EngineError::InvalidEmbodiedEvent(
                        "metabolic-rate commitment species does not match organism".to_owned(),
                    ));
                }
            }
            if self.uses_bodily_regulation_driver() {
                let regulation = organism.physiological_regulation.as_ref().ok_or(
                    EngineError::MissingPhysiologicalCommitment(organism.organism_id),
                )?;
                regulation
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                if regulation.species != organism.species || organism.metabolic_rate.is_none() {
                    return Err(EngineError::MissingPhysiologicalCommitment(
                        organism.organism_id,
                    ));
                }
                Self::validate_bodily_regulation_state(organism, organism.bodily_regulation)?;
                if organism.is_alive()
                    && self.tick != SimTick::ZERO
                    && organism.born_at != Some(self.tick)
                    && organism.bodily_regulated_at != Some(self.tick)
                {
                    return Err(EngineError::MissingBodilyRegulationTransition(
                        organism.organism_id,
                    ));
                }
                if organism.is_alive()
                    && Self::regulation_death_cause(organism.bodily_regulation.needs).is_some()
                {
                    return Err(EngineError::FatalBodilyRegulationState(
                        organism.organism_id,
                    ));
                }
                if let Some(expected) =
                    Self::regulation_death_cause(organism.bodily_regulation.needs)
                    && organism.death.as_ref().map(|death| &death.cause) != Some(&expected)
                {
                    return Err(EngineError::InvalidRegulationDeathCause(
                        organism.organism_id,
                    ));
                }
            } else if organism.physiological_regulation.is_some()
                || !organism.bodily_regulation.is_clear()
                || organism.bodily_regulated_at.is_some()
            {
                return Err(EngineError::PhysiologicalCommitmentUnsupported);
            }
            if organism.perception_memory.len() > MAX_PERCEPTION_MEMORY_ENTRIES
                || organism
                    .perception_memory
                    .windows(2)
                    .any(|pair| perception_memory_key(&pair[0]) >= perception_memory_key(&pair[1]))
            {
                return Err(EngineError::InvalidPerceptionMemory(organism.organism_id));
            }
            if self.uses_action_learning_driver() {
                if organism
                    .action_values
                    .windows(2)
                    .any(|pair| pair[0].action_kind >= pair[1].action_kind)
                    || organism
                        .action_values
                        .iter()
                        .any(|entry| entry.validate().is_err())
                    || organism
                        .action_values_updated_at
                        .is_some_and(|updated_at| updated_at > self.tick)
                {
                    return Err(EngineError::InvalidActionValueState(organism.organism_id));
                }
                if organism.is_alive()
                    && self.tick != SimTick::ZERO
                    && organism.born_at != Some(self.tick)
                    && organism.action_values_updated_at != Some(self.tick)
                {
                    return Err(EngineError::MissingActionValueTransition(
                        organism.organism_id,
                    ));
                }
            } else if !organism.action_values.is_empty()
                || organism.action_values_updated_at.is_some()
            {
                return Err(EngineError::ActionLearningUnsupported);
            }
            if organism
                .parent_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            {
                return Err(EngineError::NonCanonicalParentOrder);
            }
        }
        for (id, instance) in &self.material_instances {
            if id != &instance.object_id {
                return Err(EngineError::MaterialInstanceKeyMismatch(*id));
            }
            instance
                .material
                .validate()
                .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
            self.validate_embodied_patch(instance.embodied_patch)?;
            if !self.uses_material_ingestion_driver()
                && (instance.remaining_mass_milligrams.is_some()
                    || !instance.oral_transfer_profiles.is_empty())
            {
                return Err(EngineError::MaterialIngestionUnsupported);
            }
            if instance.remaining_mass_milligrams.is_none()
                && !instance.oral_transfer_profiles.is_empty()
            {
                return Err(EngineError::MissingMaterialMass(*id));
            }
            if instance.remaining_mass_milligrams == Some(0) && instance.held_by.is_some() {
                return Err(EngineError::MaterialInstanceDepleted(*id));
            }
            let mut previous_species_key = None;
            for profile in &instance.oral_transfer_profiles {
                profile
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                if profile.material != instance.material
                    || instance.remaining_mass_milligrams.is_none()
                {
                    return Err(EngineError::InvalidMaterialOralTransfer(*id));
                }
                let species_key = species_identity_key(&profile.species);
                if previous_species_key.is_some_and(|previous| previous >= species_key) {
                    return Err(EngineError::InvalidMaterialOralTransfer(*id));
                }
                previous_species_key = Some(species_key);
            }
            if let Some(holder_id) = instance.held_by {
                let holder = self
                    .organisms
                    .get(&holder_id)
                    .ok_or(EngineError::UnknownOrganism(holder_id))?;
                if !holder.is_alive() || holder.embodied_patch != Some(instance.embodied_patch) {
                    return Err(EngineError::MaterialInstanceNotHeldByActor(*id));
                }
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    material_instances: Vec<&'a MaterialInstanceState>,
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
        let snapshot_schema_version = if state.uses_action_learning_driver() {
            ACTION_LEARNING_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_material_ingestion_driver() {
            MATERIAL_INGESTION_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_deterministic_policy_driver() {
            DETERMINISTIC_POLICY_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_bodily_regulation_driver() {
            BODILY_REGULATION_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_signal_propagation_driver() {
            SIGNAL_PROPAGATION_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_material_handling_driver() {
            MATERIAL_HANDLING_SNAPSHOT_SCHEMA_VERSION
        } else if !state.material_instances.is_empty() {
            MATERIAL_INSTANCE_SNAPSHOT_SCHEMA_VERSION
        } else if state.has_perception_memory() {
            PERCEPTION_MEMORY_SNAPSHOT_SCHEMA_VERSION
        } else if state.has_metabolic_rate_commitments() {
            BODY_PROVENANCE_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_celestial_driver() {
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
                | BODY_PROVENANCE_SNAPSHOT_SCHEMA_VERSION
                | PERCEPTION_MEMORY_SNAPSHOT_SCHEMA_VERSION
                | MATERIAL_INSTANCE_SNAPSHOT_SCHEMA_VERSION
                | MATERIAL_HANDLING_SNAPSHOT_SCHEMA_VERSION
                | SIGNAL_PROPAGATION_SNAPSHOT_SCHEMA_VERSION
                | BODILY_REGULATION_SNAPSHOT_SCHEMA_VERSION
                | DETERMINISTIC_POLICY_SNAPSHOT_SCHEMA_VERSION
                | MATERIAL_INGESTION_SNAPSHOT_SCHEMA_VERSION
                | ACTION_LEARNING_SNAPSHOT_SCHEMA_VERSION
        ) {
            return Err(EngineError::UnsupportedSnapshotSchema(
                self.snapshot_schema_version,
            ));
        }
        let expected_schema_version = if self.state.uses_action_learning_driver() {
            ACTION_LEARNING_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_material_ingestion_driver() {
            MATERIAL_INGESTION_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_deterministic_policy_driver() {
            DETERMINISTIC_POLICY_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_bodily_regulation_driver() {
            BODILY_REGULATION_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_signal_propagation_driver() {
            SIGNAL_PROPAGATION_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_material_handling_driver() {
            MATERIAL_HANDLING_SNAPSHOT_SCHEMA_VERSION
        } else if !self.state.material_instances.is_empty() {
            MATERIAL_INSTANCE_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.has_perception_memory() {
            PERCEPTION_MEMORY_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.has_metabolic_rate_commitments() {
            BODY_PROVENANCE_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_celestial_driver() {
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
        let batch_has_metabolic_rate_commitment = batch.events.iter().any(|record| {
            matches!(
                &record.event,
                DomainEvent::OrganismInitialized {
                    metabolic_rate: Some(_),
                    ..
                } | DomainEvent::OrganismBorn {
                    metabolic_rate: Some(_),
                    ..
                }
            )
        });
        let batch_has_material_instance = batch.events.iter().any(|record| {
            matches!(
                &record.event,
                DomainEvent::MaterialInstanceInitialized { .. }
            )
        });
        let batch_has_material_handling = batch.events.iter().any(|record| {
            matches!(
                &record.event,
                DomainEvent::MaterialInstanceHeld { .. }
                    | DomainEvent::MaterialInstanceReleased { .. }
            )
        });
        let expected_schema = if state.uses_action_learning_driver() {
            ACTION_LEARNING_EVENT_SCHEMA_VERSION
        } else if state.uses_material_ingestion_driver() {
            MATERIAL_INGESTION_EVENT_SCHEMA_VERSION
        } else if state.uses_deterministic_policy_driver() {
            DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION
        } else if state.uses_bodily_regulation_driver() {
            BODILY_REGULATION_EVENT_SCHEMA_VERSION
        } else if state.uses_signal_propagation_driver() {
            SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION
        } else if state.uses_material_handling_driver() || batch_has_material_handling {
            MATERIAL_HANDLING_EVENT_SCHEMA_VERSION
        } else if !state.material_instances.is_empty() || batch_has_material_instance {
            MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION
        } else if state.has_metabolic_rate_commitments() || batch_has_metabolic_rate_commitment {
            BODY_PROVENANCE_EVENT_SCHEMA_VERSION
        } else if state.uses_celestial_driver() {
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
        let valid_schema = if expected_schema == ACTION_LEARNING_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == ACTION_LEARNING_EVENT_SCHEMA_VERSION
        } else if expected_schema == MATERIAL_INGESTION_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == MATERIAL_INGESTION_EVENT_SCHEMA_VERSION
        } else if expected_schema == DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION
        } else if expected_schema == BODILY_REGULATION_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == BODILY_REGULATION_EVENT_SCHEMA_VERSION
        } else if expected_schema == SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION
        } else if expected_schema == MATERIAL_HANDLING_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == MATERIAL_HANDLING_EVENT_SCHEMA_VERSION
        } else if expected_schema == MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION
        } else if expected_schema == BODY_PROVENANCE_EVENT_SCHEMA_VERSION {
            batch.event_schema_version == BODY_PROVENANCE_EVENT_SCHEMA_VERSION
        } else if expected_schema == CELESTIAL_STATE_EVENT_SCHEMA_VERSION {
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
        state.validate_event_coupling(&events)?;
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
    #[error("material instance {0} already exists")]
    DuplicateMaterialInstance(EntityId),
    #[error("material instance {0} does not exist")]
    UnknownMaterialInstance(EntityId),
    #[error("a grasp or release action requires a material target")]
    MissingActionTarget,
    #[error("material instance {0} is already held")]
    MaterialInstanceAlreadyHeld(EntityId),
    #[error("material instance {0} is not at the actor's current patch")]
    NonLocalMaterialAction(EntityId),
    #[error("material instance {0} is not held by the acting organism")]
    MaterialInstanceNotHeldByActor(EntityId),
    #[error("material instance {0} release patch does not match the holder")]
    InvalidMaterialReleasePatch(EntityId),
    #[error("material ingestion is unsupported by this ruleset")]
    MaterialIngestionUnsupported,
    #[error("material instance {0} has no retained physical mass")]
    MissingMaterialMass(EntityId),
    #[error("material instance {0} is physically depleted")]
    MaterialInstanceDepleted(EntityId),
    #[error("material instance {0} has an invalid oral mass transfer")]
    InvalidMaterialOralTransfer(EntityId),
    #[error("organism map key {0} does not match its value")]
    OrganismKeyMismatch(EntityId),
    #[error("material instance map key {0} does not match its value")]
    MaterialInstanceKeyMismatch(EntityId),
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
    #[error("organism {0} perception memory is invalid")]
    InvalidPerceptionMemory(EntityId),
    #[error("organism {0} perception memory has reached its fixed capacity")]
    PerceptionMemoryCapacity(EntityId),
    #[error("organism {0} lacks its atomic metabolic or physiological commitment")]
    MissingPhysiologicalCommitment(EntityId),
    #[error("physiological regulation is unsupported by this ruleset")]
    PhysiologicalCommitmentUnsupported,
    #[error("bodily regulation requires a tick-zero local environment baseline")]
    MissingLocalEnvironmentForRegulation,
    #[error("bodily regulation does not support temperature unit {0:?}")]
    UnsupportedTemperatureUnit(String),
    #[error("physiological arithmetic failed: {0}")]
    PhysiologicalArithmetic(String),
    #[error("organism {0} has an invalid bodily-regulation state")]
    InvalidBodilyRegulationState(EntityId),
    #[error("organism {0} has an invalid or duplicate bodily-regulation transition")]
    InvalidBodilyRegulationTransition(EntityId),
    #[error("organism {0} is missing its bodily-regulation transition for this tick")]
    MissingBodilyRegulationTransition(EntityId),
    #[error("living organism {0} exhausted a fatal regulation budget without dying")]
    FatalBodilyRegulationState(EntityId),
    #[error("organism {0} has a mortality cause inconsistent with its regulation limit")]
    InvalidRegulationDeathCause(EntityId),
    #[error("organism {0} emitted more than one scheduled action in one tick")]
    DuplicateScheduledAction(EntityId),
    #[error("organism {0} emitted no scheduled action before bodily regulation")]
    MissingScheduledAction(EntityId),
    #[error("action learning is unsupported by this ruleset")]
    ActionLearningUnsupported,
    #[error("organism {0} has an invalid action-value state")]
    InvalidActionValueState(EntityId),
    #[error("organism {0} has an invalid or duplicate action-value transition")]
    InvalidActionValueTransition(EntityId),
    #[error("organism {0} is missing its action-value transition for this tick")]
    MissingActionValueTransition(EntityId),
    #[error("organism {0} action-value observation count overflowed")]
    ActionValueObservationOverflow(EntityId),
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
    #[error("a canonical birth must identify at least one known parent")]
    ParentlessBirth,
    #[error("birth species and participation tier must match every parent")]
    IncompatibleBirthLineage,
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
    #[error(transparent)]
    Geographic(#[from] GeographicRoutingError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use world_domain::{
        CapacityExhaustionPolicy, CartesianMillimetres, CelestialState, EarthResolutionLevels,
        FullEarthGrid, PartitionedExecution, PerceptionChannel, PersonRepresentation,
        PrimitiveActionKind, PropertyReading, ProvisionalLocalEnvironmentBaseline,
        ProvisionalWorldCompositionReference, S2Projection, SchedulerKind, SituatedPerception,
        SpatialGrid, TdbSecondsSinceJ2000, WorldDataBundleReference, WorldSeed, WorldStatus,
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
            metabolic_rate: None,
            physiological_regulation: None,
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

    fn environmental_provisional_full_earth_configuration() -> WorldConfiguration {
        let provisional = provisional_full_earth_configuration();
        let active_patch: S2CellId = "0000000000004000".parse().expect("L23 patch");
        WorldConfiguration::new_provisional_full_earth_with_environment_baseline(
            300,
            provisional.full_earth_grid().expect("grid").clone(),
            provisional
                .provisional_world_composition()
                .expect("composition")
                .clone(),
            provisional
                .partitioned_execution()
                .expect("execution")
                .clone(),
            ProvisionalLocalEnvironmentBaseline {
                status: "provisional-evidence-only".to_owned(),
                source_evidence_digest: Digest::sha256(b"local evidence"),
                evidence_patch: active_patch.ancestor(10).expect("L10 ancestor"),
                active_patch,
                air_temperature_unit: "degC".to_owned(),
                air_temperature_decimal_places: 1,
                air_temperature_normal_minimum: [1; 12],
                air_temperature_normal_mean: [2; 12],
                air_temperature_normal_maximum: [3; 12],
            },
        )
        .expect("environmental provisional config")
    }

    fn full_earth_person(world_id: WorldId) -> InitialOrganism {
        let mut person = initial_person(world_id);
        let patch: S2CellId = "0000000000004000".parse().expect("valid L23 S2 cell");
        assert_eq!(patch.level(), 23);
        person.embodied_patch = Some(patch);
        person
    }

    fn regulated_full_earth_person(
        world_id: WorldId,
        organism_id: u128,
        usable_energy_reserve_joules: u64,
        hydration_failure_seconds: u64,
    ) -> InitialOrganism {
        let mut person = full_earth_person(world_id);
        person.organism_id = EntityId::from_uuid(Uuid::from_u128(organism_id));
        person.metabolic_rate = Some(MetabolicRateCommitment {
            commitment_schema_version: world_domain::METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION,
            profile_set_digest: Digest::sha256(b"regulated fixture metabolic profiles"),
            observed_species: person.species.clone(),
            source_record_id: "fixture-rate".to_owned(),
            source_record_digest: Digest::sha256(b"regulated fixture metabolic row"),
            measured_power_value: 1,
            measured_power_decimal_places: 0,
        });
        person.physiological_regulation = Some(PhysiologicalRegulationCommitment {
            commitment_schema_version:
                world_domain::PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION,
            profile_id: "regulated-fixture-v1".to_owned(),
            profile_digest: Digest::sha256(b"explicit regulated fixture assumptions"),
            species: person.species.clone(),
            evidence_basis: world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption,
            usable_energy_reserve_joules,
            hydration_failure_seconds,
            fatigue_failure_seconds: 600,
            fatigue_recovery_seconds: 600,
            thermoneutral_min_millicelsius: -1_000,
            thermoneutral_max_millicelsius: 1_000,
            thermal_failure_millicelsius_seconds: 600_000,
            thermal_recovery_seconds: 600,
        });
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
    fn source_pinned_metabolic_commitment_is_hash_chained_and_replayable() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let mut person = initial_person(manifest.world_id);
        person.metabolic_rate = Some(MetabolicRateCommitment {
            commitment_schema_version: 1,
            profile_set_digest: Digest::sha256(b"retained metabolic profile set"),
            observed_species: person.species.clone(),
            source_record_id: "retained-row-1".to_owned(),
            source_record_digest: Digest::sha256(b"retained metabolic source row"),
            measured_power_value: 125,
            measured_power_decimal_places: 3,
        });
        let genesis_events = initial
            .plan_configured_genesis(world_configuration(), vec![person])
            .expect("committed genesis");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("source-pinned genesis batch");
        assert_eq!(
            genesis.event_schema_version,
            BODY_PROVENANCE_EVENT_SCHEMA_VERSION
        );
        let tick_events = running.plan_next_tick().expect("tick plan");
        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("source-pinned tick batch");
        assert_eq!(
            tick.event_schema_version,
            BODY_PROVENANCE_EVENT_SCHEMA_VERSION
        );
        let snapshot = Snapshot::new(after_tick.clone(), tick.sequence, tick.batch_hash)
            .expect("source-pinned snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            BODY_PROVENANCE_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(
            replay(manifest, &[genesis, tick]).expect("replay").state,
            after_tick
        );
    }

    #[test]
    fn persistent_perceptions_are_bounded_hash_chained_and_replayable() {
        let mut manifest = manifest();
        manifest.ruleset_version = PERSISTENT_PERCEPTION_RULESET_VERSION;
        let initial = EngineState::new(manifest.clone());
        let person = initial_person(manifest.world_id);
        let person_id = person.organism_id;
        let (running, genesis) = initial
            .commit(
                EventSequence::new(1),
                Digest::ZERO,
                initial
                    .plan_configured_genesis(world_configuration(), vec![person])
                    .expect("genesis plan"),
            )
            .expect("genesis");
        let first_reading = SituatedPerception {
            subject_id: None,
            readings: vec![PropertyReading {
                channel: PerceptionChannel::Touch,
                property_code: "temperature".to_owned(),
                quantized_value: 12,
                uncertainty: 1,
            }],
        };
        let (after_first, first_batch) = running
            .commit(
                EventSequence::new(2),
                genesis.batch_hash,
                running
                    .plan_perception(person_id, first_reading)
                    .expect("first perception"),
            )
            .expect("first perception batch");
        assert_eq!(after_first.organisms[&person_id].perception_memory_len(), 1);
        let replacement = SituatedPerception {
            subject_id: None,
            readings: vec![PropertyReading {
                channel: PerceptionChannel::Touch,
                property_code: "temperature".to_owned(),
                quantized_value: 13,
                uncertainty: 0,
            }],
        };
        let (after_replacement, replacement_batch) = after_first
            .commit(
                EventSequence::new(3),
                first_batch.batch_hash,
                after_first
                    .plan_perception(person_id, replacement)
                    .expect("replacement perception"),
            )
            .expect("replacement perception batch");
        assert_eq!(
            after_replacement.organisms[&person_id].perception_memory_len(),
            1
        );
        let snapshot = Snapshot::new(
            after_replacement.clone(),
            replacement_batch.sequence,
            replacement_batch.batch_hash,
        )
        .expect("snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            PERCEPTION_MEMORY_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(
            replay(manifest, &[genesis, first_batch, replacement_batch])
                .expect("replay")
                .state,
            after_replacement
        );
    }

    #[test]
    fn birth_lineage_cannot_cross_species_or_participation_tiers() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let parent = initial_person(manifest.world_id);
        let parent_id = parent.organism_id;
        let (running, genesis) = initial
            .commit(
                EventSequence::new(1),
                Digest::ZERO,
                initial.plan_genesis(vec![parent]).expect("genesis plan"),
            )
            .expect("genesis");
        let other_species = SpeciesIdentity::new(
            "gbif",
            "5219173",
            "Canis lupus",
            "https://www.gbif.org/species/5219173",
        )
        .expect("species");
        let birth = DomainEvent::OrganismBorn {
            organism_id: EntityId::deterministic(manifest.world_id, b"invalid-cross-species-birth"),
            species: other_species,
            role: OrganismRole::Fauna,
            birth_category: BirthCategory::new("unspecified").expect("category"),
            parent_ids: vec![parent_id],
            location_id: None,
            embodied_patch: None,
            metabolic_rate: None,
            physiological_regulation: None,
        };
        assert!(matches!(
            running.commit(EventSequence::new(2), genesis.batch_hash, vec![birth]),
            Err(EngineError::IncompatibleBirthLineage)
        ));
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
    fn cited_material_instance_is_replayed_at_its_full_earth_patch() {
        let manifest = manifest();
        let initial = EngineState::new(manifest.clone());
        let patch: S2CellId = "0000000000004000".parse().expect("L23 patch");
        let object_id = EntityId::deterministic(manifest.world_id, b"water-instance");
        let events = vec![
            DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            },
            DomainEvent::WorldConfigured {
                configuration: full_earth_configuration(),
            },
            DomainEvent::MaterialInstanceInitialized {
                object_id,
                material: MaterialIdentity::new(
                    "pubchem",
                    "962",
                    "water",
                    "https://pubchem.ncbi.nlm.nih.gov/compound/962",
                )
                .expect("citable material"),
                embodied_patch: patch,
                initial_mass_milligrams: None,
                oral_transfer_profiles: Vec::new(),
            },
        ];
        let (running, batch) = initial
            .commit(EventSequence::new(1), Digest::ZERO, events)
            .expect("material genesis commit");
        assert_eq!(
            batch.event_schema_version,
            MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION
        );
        assert_eq!(running.material_instances().len(), 1);
        assert_eq!(
            running
                .material_instances()
                .next()
                .expect("instance")
                .embodied_patch(),
            patch
        );
        let snapshot = Snapshot::new(running.clone(), batch.sequence, batch.batch_hash)
            .expect("material snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            MATERIAL_INSTANCE_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot.verify_integrity().expect("snapshot verifies");
        let replayed = replay(manifest, &[batch]).expect("replay material event");
        assert_eq!(replayed.state, running);
    }

    #[test]
    fn material_handling_requires_a_local_object_and_replays() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x108));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012817),
            MATERIAL_HANDLING_RULESET_VERSION,
        );
        let patch: S2CellId = "0000000000004000".parse().expect("L23 patch");
        let mut person = initial_person(world_id);
        person.embodied_patch = Some(patch);
        let object_id = EntityId::deterministic(world_id, b"handled-water-instance");
        let initial = EngineState::new(manifest.clone());
        let mut genesis_events = initial
            .plan_configured_genesis(full_earth_configuration(), vec![person.clone()])
            .expect("genesis plan");
        genesis_events.push(DomainEvent::MaterialInstanceInitialized {
            object_id,
            material: MaterialIdentity::new(
                "pubchem",
                "962",
                "water",
                "https://pubchem.ncbi.nlm.nih.gov/compound/962",
            )
            .expect("citable material"),
            embodied_patch: patch,
            initial_mass_milligrams: None,
            oral_transfer_profiles: Vec::new(),
        });
        let (after_genesis, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("material genesis");
        assert_eq!(
            genesis.event_schema_version,
            MATERIAL_HANDLING_EVENT_SCHEMA_VERSION
        );

        let grasp = after_genesis
            .plan_action(
                person.organism_id,
                PrimitiveAction {
                    kind: PrimitiveActionKind::Grasp,
                    target_id: Some(object_id),
                    intensity: 1,
                },
            )
            .expect("local grasp plan");
        assert!(matches!(
            grasp.as_slice(),
            [
                DomainEvent::OrganismActed { .. },
                DomainEvent::MaterialInstanceHeld { .. }
            ]
        ));
        let (after_grasp, grasp_batch) = after_genesis
            .commit(EventSequence::new(2), genesis.batch_hash, grasp)
            .expect("grasp commit");
        assert_eq!(
            after_grasp
                .material_instances()
                .next()
                .expect("material")
                .held_by(),
            Some(person.organism_id)
        );
        let moved_to = s2_edge_neighbors(patch).expect("neighbor patches")[0];
        let movement = after_grasp
            .plan_movement(person.organism_id, moved_to)
            .expect("movement plan");
        let (after_movement, movement_batch) = after_grasp
            .commit(EventSequence::new(3), grasp_batch.batch_hash, movement)
            .expect("movement commit");
        assert_eq!(
            after_movement
                .material_instances()
                .next()
                .expect("material")
                .embodied_patch(),
            moved_to
        );
        let release = after_movement
            .plan_action(
                person.organism_id,
                PrimitiveAction {
                    kind: PrimitiveActionKind::Release,
                    target_id: Some(object_id),
                    intensity: 1,
                },
            )
            .expect("release plan");
        let (after_release, release_batch) = after_movement
            .commit(EventSequence::new(4), movement_batch.batch_hash, release)
            .expect("release commit");
        assert_eq!(
            after_release
                .material_instances()
                .next()
                .expect("material")
                .held_by(),
            None
        );
        let replayed = replay(
            manifest,
            &[genesis, grasp_batch, movement_batch, release_batch],
        )
        .expect("handling replay");
        assert_eq!(replayed.state, after_release);
    }

    #[test]
    fn local_signals_reach_only_same_patch_living_recipients_and_replay() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x109));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012818),
            SIGNAL_PROPAGATION_RULESET_VERSION,
        );
        let patch: S2CellId = "0000000000004000".parse().expect("L23 patch");
        let remote_patch = s2_edge_neighbors(patch).expect("neighbors")[0];
        let mut source = full_earth_person(world_id);
        source.organism_id = EntityId::from_uuid(Uuid::from_u128(0x101));
        let mut local = full_earth_person(world_id);
        local.organism_id = EntityId::from_uuid(Uuid::from_u128(0x102));
        let mut remote = full_earth_person(world_id);
        remote.organism_id = EntityId::from_uuid(Uuid::from_u128(0x103));
        remote.embodied_patch = Some(remote_patch);
        let mut dead_local = full_earth_person(world_id);
        dead_local.organism_id = EntityId::from_uuid(Uuid::from_u128(0x104));

        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![
                    source.clone(),
                    local.clone(),
                    remote.clone(),
                    dead_local.clone(),
                ],
            )
            .expect("signal genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("signal genesis");
        assert_eq!(
            genesis.event_schema_version,
            SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION
        );
        let death_events = running
            .plan_death(
                dead_local.organism_id,
                DeathCause {
                    mechanism: "test_fixture".to_owned(),
                },
            )
            .expect("local death plan");
        let (after_death, death_batch) = running
            .commit(EventSequence::new(2), genesis.batch_hash, death_events)
            .expect("local death commit");

        let signal_events = after_death
            .plan_action(
                source.organism_id,
                PrimitiveAction {
                    kind: PrimitiveActionKind::EmitSignal,
                    target_id: None,
                    intensity: 7,
                },
            )
            .expect("signal plan");
        assert!(matches!(
            signal_events.as_slice(),
            [
                DomainEvent::OrganismActed { organism_id, .. },
                DomainEvent::OrganismPerceived {
                    organism_id: recipient_id,
                    perception: SituatedPerception {
                        subject_id: Some(subject_id),
                        ..
                    },
                }
            ] if *organism_id == source.organism_id
                && *recipient_id == local.organism_id
                && *subject_id == source.organism_id
        ));
        let (after_signal, signal_batch) = after_death
            .commit(EventSequence::new(3), death_batch.batch_hash, signal_events)
            .expect("signal commit");
        let downgraded_signal = EventBatch::new(
            MATERIAL_HANDLING_EVENT_SCHEMA_VERSION,
            signal_batch.world_id,
            signal_batch.sequence,
            signal_batch.tick,
            signal_batch.ruleset_version,
            signal_batch.previous_hash,
            signal_batch
                .events
                .iter()
                .map(|record| record.event.clone())
                .collect(),
            signal_batch.post_state_hash,
        )
        .expect("internally valid pre-signal-schema batch");
        assert!(matches!(
            replay(
                manifest.clone(),
                &[genesis.clone(), death_batch.clone(), downgraded_signal]
            ),
            Err(EngineError::BatchEventSchemaMismatch {
                expected: SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION,
                actual: MATERIAL_HANDLING_EVENT_SCHEMA_VERSION,
            })
        ));
        let local_state = after_signal
            .organisms()
            .find(|organism| organism.organism_id() == local.organism_id)
            .expect("local recipient");
        assert!(local_state.has_perception_memory_at(
            Some(source.organism_id),
            PerceptionChannel::Sound,
            "signal_amplitude"
        ));
        let remote_state = after_signal
            .organisms()
            .find(|organism| organism.organism_id() == remote.organism_id)
            .expect("remote organism");
        assert!(!remote_state.has_perception_memory_at(
            Some(source.organism_id),
            PerceptionChannel::Sound,
            "signal_amplitude"
        ));
        let snapshot = Snapshot::new(
            after_signal.clone(),
            signal_batch.sequence,
            signal_batch.batch_hash,
        )
        .expect("signal snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            SIGNAL_PROPAGATION_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("signal snapshot verifies");
        let replayed = replay(
            manifest.clone(),
            &[genesis.clone(), death_batch.clone(), signal_batch],
        )
        .expect("signal replay");
        assert_eq!(replayed.state, after_signal);

        let celestial = CelestialState::new(
            TdbSecondsSinceJ2000::new(123),
            CartesianMillimetres::new(1, 2, 3),
            CartesianMillimetres::new(4, 5, 6),
        );
        let scheduled_events = after_death
            .plan_next_tick_with_celestial(celestial)
            .expect("scheduled signal tick");
        assert!(scheduled_events.iter().any(|event| matches!(
            event,
            DomainEvent::OrganismActed { organism_id, action }
                if *organism_id == source.organism_id
                    && action.kind == PrimitiveActionKind::EmitSignal
        )));
        assert!(scheduled_events.iter().any(|event| matches!(
            event,
            DomainEvent::OrganismPerceived { organism_id, perception }
                if *organism_id == local.organism_id
                    && perception.subject_id == Some(source.organism_id)
                    && perception.readings[0].channel == PerceptionChannel::Sound
                    && perception.readings[0].property_code == "signal_amplitude"
        )));
        let (after_tick, tick_batch) = after_death
            .commit(
                EventSequence::new(3),
                death_batch.batch_hash,
                scheduled_events,
            )
            .expect("scheduled signal commit");
        assert_eq!(
            replay(manifest, &[genesis, death_batch, tick_batch])
                .expect("scheduled signal replay")
                .state,
            after_tick
        );
    }

    #[test]
    fn bodily_regulation_requires_atomic_commitments_and_a_supported_environment() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x110));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012818),
            BODILY_REGULATION_RULESET_VERSION,
        );
        let initial = EngineState::new(manifest.clone());
        assert!(matches!(
            initial.plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![full_earth_person(world_id)]
            ),
            Err(EngineError::MissingPhysiologicalCommitment(_))
        ));

        let person = regulated_full_earth_person(world_id, 0x201, 600, 600);
        assert!(matches!(
            initial.plan_configured_genesis(
                provisional_full_earth_configuration(),
                vec![person.clone()]
            ),
            Err(EngineError::MissingLocalEnvironmentForRegulation)
        ));
        let mut unsupported_environment = environmental_provisional_full_earth_configuration();
        unsupported_environment
            .local_environment_baseline
            .as_mut()
            .expect("environment baseline")
            .air_temperature_unit = "K".to_owned();
        assert!(matches!(
            initial.plan_configured_genesis(unsupported_environment, vec![person.clone()]),
            Err(EngineError::UnsupportedTemperatureUnit(unit)) if unit == "K"
        ));

        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![person.clone()],
            )
            .expect("atomic regulation genesis");
        assert!(matches!(
            genesis_events.last(),
            Some(DomainEvent::OrganismInitialized {
                metabolic_rate: Some(_),
                physiological_regulation: Some(_),
                ..
            })
        ));
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("regulated genesis commit");
        let unsupported_birth = DomainEvent::OrganismBorn {
            organism_id: EntityId::from_uuid(Uuid::from_u128(0x202)),
            species: person.species.clone(),
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("female").expect("category"),
            parent_ids: vec![person.organism_id],
            location_id: None,
            embodied_patch: person.embodied_patch,
            metabolic_rate: None,
            physiological_regulation: None,
        };
        assert!(matches!(
            running.commit(
                EventSequence::new(2),
                genesis.batch_hash,
                vec![unsupported_birth]
            ),
            Err(EngineError::MissingPhysiologicalCommitment(_))
        ));
    }

    #[test]
    fn bodily_regulation_uses_exact_loads_and_archives_on_the_fatal_tick() {
        assert_eq!(
            normalized_pressure_intensity(300, 100_000_000).expect("small exact load"),
            0,
            "small loads must not round up once per tick"
        );
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x111));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012818),
            BODILY_REGULATION_RULESET_VERSION,
        );
        let person = regulated_full_earth_person(world_id, 0x201, 600, 600);
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![person.clone()],
            )
            .expect("regulated genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("regulated genesis");
        assert_eq!(
            genesis.event_schema_version,
            BODILY_REGULATION_EVENT_SCHEMA_VERSION
        );

        let celestial_one = CelestialState::new(
            TdbSecondsSinceJ2000::new(300),
            CartesianMillimetres::new(1, 2, 3),
            CartesianMillimetres::new(4, 5, 6),
        );
        let tick_one_events = running
            .plan_next_tick_with_celestial(celestial_one)
            .expect("first regulated tick");
        let first_transition = tick_one_events
            .iter()
            .find_map(|event| match event {
                DomainEvent::OrganismNeedsChanged {
                    organism_id, to, ..
                } if *organism_id == person.organism_id => Some(*to),
                _ => None,
            })
            .expect("first body transition");
        assert_eq!(first_transition.energy_load_scaled_joules, 300);
        assert_eq!(first_transition.hydration_load_seconds, 300);
        assert_eq!(first_transition.needs.energy_deficit, 32_767);
        assert_eq!(first_transition.needs.hydration_deficit, 32_767);
        assert!(
            !tick_one_events
                .iter()
                .any(|event| matches!(event, DomainEvent::OrganismDied { .. }))
        );
        let (after_one, tick_one) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_one_events)
            .expect("first regulated commit");
        let snapshot = Snapshot::new(after_one.clone(), tick_one.sequence, tick_one.batch_hash)
            .expect("regulated snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            BODILY_REGULATION_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot.verify_integrity().expect("regulated snapshot");

        let celestial_two = CelestialState::new(
            TdbSecondsSinceJ2000::new(600),
            CartesianMillimetres::new(2, 3, 4),
            CartesianMillimetres::new(5, 6, 7),
        );
        let tick_two_events = after_one
            .plan_next_tick_with_celestial(celestial_two)
            .expect("fatal regulated tick");
        let first_death = tick_two_events
            .iter()
            .position(|event| matches!(event, DomainEvent::OrganismDied { .. }))
            .expect("automatic death");
        assert!(tick_two_events[..first_death].iter().any(|event| matches!(
            event,
            DomainEvent::OrganismNeedsChanged { organism_id, to, .. }
                if *organism_id == person.organism_id
                    && to.needs.energy_deficit == u16::MAX
                    && to.needs.hydration_deficit == u16::MAX
        )));
        assert!(matches!(
            &tick_two_events[first_death],
            DomainEvent::OrganismDied { organism_id, cause }
                if *organism_id == person.organism_id
                    && cause.mechanism == "bodily_regulation_v1_hydration_failure"
        ));
        assert!(matches!(
            tick_two_events.as_slice(),
            [.., DomainEvent::WorldExtinct, DomainEvent::WorldArchived]
        ));
        let (archived, tick_two) = after_one
            .commit(EventSequence::new(3), tick_one.batch_hash, tick_two_events)
            .expect("fatal regulated commit");
        assert_eq!(archived.status(), WorldStatus::Archived);
        assert_eq!(archived.living_people(), 0);
        assert_eq!(archived.scheduled_work_count(), 0);
        assert_eq!(
            replay(
                manifest.clone(),
                &[genesis.clone(), tick_one.clone(), tick_two]
            )
            .expect("regulated replay")
            .state,
            archived
        );

        let downgraded = EventBatch::new(
            SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            BODILY_REGULATION_RULESET_VERSION,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"downgraded body state"),
        )
        .expect("internally valid pre-regulation batch");
        assert!(matches!(
            replay(manifest, &[downgraded]),
            Err(EngineError::BatchEventSchemaMismatch {
                expected: BODILY_REGULATION_EVENT_SCHEMA_VERSION,
                actual: SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION,
            })
        ));
    }

    #[test]
    fn rest_recovers_exact_fatigue_load_without_erasing_other_needs() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x112));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012818),
            BODILY_REGULATION_RULESET_VERSION,
        );
        // 0x202 executes phase 3 at age one, then phase 0 (rest) at age two.
        let person = regulated_full_earth_person(world_id, 0x202, 10_000_000, 1_000_000);
        let initial = EngineState::new(manifest);
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![person.clone()],
            )
            .expect("fatigue genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("fatigue genesis");
        let first_events = running
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(300),
                CartesianMillimetres::new(1, 2, 3),
                CartesianMillimetres::new(4, 5, 6),
            ))
            .expect("active tick");
        let (after_active, first_batch) = running
            .commit(EventSequence::new(2), genesis.batch_hash, first_events)
            .expect("active tick commit");
        let active_needs = after_active
            .organisms()
            .find(|organism| organism.organism_id() == person.organism_id)
            .expect("person")
            .bodily_needs();
        assert_eq!(active_needs.fatigue, 32_767);
        assert!(active_needs.energy_deficit > 0);

        let rest_events = after_active
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(600),
                CartesianMillimetres::new(2, 3, 4),
                CartesianMillimetres::new(5, 6, 7),
            ))
            .expect("rest tick");
        assert!(rest_events.iter().any(|event| matches!(
            event,
            DomainEvent::OrganismActed { organism_id, action }
                if *organism_id == person.organism_id
                    && action.kind == PrimitiveActionKind::Rest
        )));
        let (after_rest, _) = after_active
            .commit(EventSequence::new(3), first_batch.batch_hash, rest_events)
            .expect("rest tick commit");
        let rested_needs = after_rest
            .organisms()
            .find(|organism| organism.organism_id() == person.organism_id)
            .expect("person")
            .bodily_needs();
        assert_eq!(rested_needs.fatigue, 0);
        assert!(rested_needs.energy_deficit > active_needs.energy_deficit);
    }

    #[test]
    fn fatal_tick_delivers_all_local_signals_before_any_automatic_death() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x113));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012818),
            BODILY_REGULATION_RULESET_VERSION,
        );
        let source = regulated_full_earth_person(world_id, 0x201, 300, 300);
        let recipient = regulated_full_earth_person(world_id, 0x204, 300, 300);
        let initial = EngineState::new(manifest);
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![source.clone(), recipient.clone()],
            )
            .expect("fatal signal genesis");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("fatal signal genesis commit");
        let events = running
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(300),
                CartesianMillimetres::new(1, 2, 3),
                CartesianMillimetres::new(4, 5, 6),
            ))
            .expect("fatal signal tick");
        let first_death = events
            .iter()
            .position(|event| matches!(event, DomainEvent::OrganismDied { .. }))
            .expect("automatic death");
        assert!(events[..first_death].iter().any(|event| matches!(
            event,
            DomainEvent::OrganismPerceived { organism_id, perception }
                if *organism_id == recipient.organism_id
                    && perception.subject_id == Some(source.organism_id)
                    && perception.readings[0].property_code == "signal_amplitude"
        )));
        assert_eq!(
            events[..first_death]
                .iter()
                .filter(|event| matches!(event, DomainEvent::OrganismNeedsChanged { .. }))
                .count(),
            2
        );
        let (archived, _) = running
            .commit(EventSequence::new(2), genesis.batch_hash, events)
            .expect("fatal signals commit atomically");
        assert_eq!(archived.status(), WorldStatus::Archived);
        assert_eq!(archived.living_people(), 0);
    }

    #[test]
    fn seeded_policy_is_diverse_need_responsive_and_strictly_local() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x114));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012819),
            DETERMINISTIC_POLICY_RULESET_VERSION,
        );
        let leader = regulated_full_earth_person(world_id, 0x201, 10_000_000, 1_000_000);
        let follower = regulated_full_earth_person(world_id, 0x202, 10_000_000, 1_000_000);
        let local_object = EntityId::deterministic(world_id, b"policy-local-water");
        let remote_object = EntityId::deterministic(world_id, b"policy-remote-water");
        let patch = leader.embodied_patch.expect("leader patch");
        let remote_patch = s2_edge_neighbors(patch).expect("neighbor patches")[0];
        let initial = EngineState::new(manifest);
        let mut genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![leader.clone(), follower.clone()],
            )
            .expect("policy genesis plan");
        for (object_id, embodied_patch) in [(local_object, patch), (remote_object, remote_patch)] {
            genesis_events.push(DomainEvent::MaterialInstanceInitialized {
                object_id,
                material: MaterialIdentity::new(
                    "pubchem",
                    "962",
                    "water",
                    "https://pubchem.ncbi.nlm.nih.gov/compound/962",
                )
                .expect("citable water"),
                embodied_patch,
                initial_mass_milligrams: None,
                oral_transfer_profiles: Vec::new(),
            });
        }
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("policy genesis");
        assert_eq!(
            genesis.event_schema_version,
            DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION
        );

        let leader_state = running
            .organisms
            .get(&leader.organism_id)
            .expect("leader state");
        let follower_state = running
            .organisms
            .get(&follower.organism_id)
            .expect("follower state");
        let leader_candidates = running
            .deterministic_policy_candidates(leader_state, 1)
            .expect("leader candidates");
        assert!(leader_candidates.iter().all(|candidate| {
            candidate.action.target_id.is_none() || candidate.action.target_id == Some(local_object)
        }));
        assert!(leader_candidates.iter().any(|candidate| {
            candidate.action.kind == PrimitiveActionKind::Grasp
                && candidate.action.target_id == Some(local_object)
        }));
        let follower_candidates = running
            .deterministic_policy_candidates(follower_state, 1)
            .expect("follower candidates");
        assert!(
            follower_candidates
                .iter()
                .all(|candidate| candidate.action.target_id.is_none()),
            "the deterministic patch conflict rule gives only the lowest identity an unheld target"
        );

        let mut pressured_leader = leader_state.clone();
        pressured_leader.bodily_regulation.needs = BodilyNeedState {
            energy_deficit: u16::MAX,
            hydration_deficit: u16::MAX,
            fatigue: u16::MAX,
            ..BodilyNeedState::default()
        };
        let pressured_candidates = running
            .deterministic_policy_candidates(&pressured_leader, 1)
            .expect("pressured candidates");
        let weight = |kind| {
            pressured_candidates
                .iter()
                .find(|candidate| candidate.action.kind == kind)
                .map(|candidate| candidate.weight)
                .expect("candidate kind")
        };
        assert_eq!(weight(PrimitiveActionKind::Reach), 8);
        assert_eq!(weight(PrimitiveActionKind::Grasp), 8);
        assert_eq!(weight(PrimitiveActionKind::Rest), 16);

        let mut pressured_follower = follower_state.clone();
        pressured_follower.bodily_regulation.needs = pressured_leader.bodily_regulation.needs;
        let no_target_candidates = running
            .deterministic_policy_candidates(&pressured_follower, 1)
            .expect("no-target candidates");
        assert_eq!(
            no_target_candidates
                .iter()
                .find(|candidate| candidate.action.kind == PrimitiveActionKind::Reach)
                .expect("reach remains available")
                .weight,
            1,
            "need pressure must not reveal a solution that is not locally reachable"
        );

        let mut kinds = Vec::new();
        for age_ticks in 1..=128 {
            let first = running
                .deterministic_policy_action(leader_state, age_ticks)
                .expect("first deterministic draw");
            let second = running
                .deterministic_policy_action(leader_state, age_ticks)
                .expect("second deterministic draw");
            assert_eq!(first, second);
            assert!((1..=4).contains(&first.intensity));
            assert!(first.target_id.is_none() || first.target_id == Some(local_object));
            kinds.push(first.kind);
        }
        kinds.sort_unstable();
        kinds.dedup();
        assert!(
            kinds.len() >= 5,
            "the seeded policy must not collapse back to the four-step integration cadence"
        );
    }

    #[test]
    fn ruleset_eleven_tick_snapshot_and_replay_share_the_policy_boundary() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x115));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012820),
            DETERMINISTIC_POLICY_RULESET_VERSION,
        );
        let person = regulated_full_earth_person(world_id, 0x301, 10_000_000, 1_000_000);
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![person.clone()],
            )
            .expect("policy genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("policy genesis");
        assert_eq!(
            genesis.event_schema_version,
            DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            running.state_hash_schema_version(),
            DETERMINISTIC_POLICY_STATE_HASH_SCHEMA_VERSION
        );
        let genesis_snapshot = Snapshot::new(running.clone(), genesis.sequence, genesis.batch_hash)
            .expect("policy genesis snapshot");
        assert_eq!(
            genesis_snapshot.snapshot_schema_version,
            DETERMINISTIC_POLICY_SNAPSHOT_SCHEMA_VERSION
        );
        genesis_snapshot
            .verify_integrity()
            .expect("policy genesis snapshot verifies");

        let expected_action = running
            .deterministic_policy_action(
                running
                    .organisms
                    .get(&person.organism_id)
                    .expect("person state"),
                1,
            )
            .expect("policy action");
        let tick_events = running
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(300),
                CartesianMillimetres::new(1, 2, 3),
                CartesianMillimetres::new(4, 5, 6),
            ))
            .expect("policy tick");
        assert!(tick_events.iter().any(|event| matches!(
            event,
            DomainEvent::OrganismActed { organism_id, action }
                if *organism_id == person.organism_id && *action == expected_action
        )));
        assert_eq!(
            tick_events
                .iter()
                .filter(|event| matches!(event, DomainEvent::OrganismNeedsChanged { .. }))
                .count(),
            1
        );
        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("policy tick commit");
        assert_eq!(
            tick.event_schema_version,
            DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION
        );
        let complete =
            replay(manifest.clone(), &[genesis.clone(), tick.clone()]).expect("policy replay");
        assert_eq!(complete.state, after_tick);
        assert_eq!(
            replay_from_snapshot(&genesis_snapshot, &[tick])
                .expect("policy snapshot plus tail")
                .state,
            after_tick
        );
        let after_snapshot = Snapshot::new(
            after_tick.clone(),
            complete.through_sequence,
            complete.last_event_hash,
        )
        .expect("policy tick snapshot");
        assert_eq!(
            after_snapshot.snapshot_schema_version,
            DETERMINISTIC_POLICY_SNAPSHOT_SCHEMA_VERSION
        );

        let downgraded = EventBatch::new(
            BODILY_REGULATION_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            DETERMINISTIC_POLICY_RULESET_VERSION,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"downgraded policy state"),
        )
        .expect("internally valid pre-policy batch");
        assert!(matches!(
            replay(manifest, &[downgraded]),
            Err(EngineError::BatchEventSchemaMismatch {
                expected: DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION,
                actual: BODILY_REGULATION_EVENT_SCHEMA_VERSION,
            })
        ));
    }

    #[test]
    fn ruleset_twelve_transfers_mass_and_recovers_needs_without_policy_labels() {
        assert_eq!(
            integrate_load_with_recovery(900, 300, 400, 1_000).expect("exact boundary recovery"),
            800,
            "recovery must apply before the one final capacity clamp"
        );
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x116));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012821),
            MATERIAL_INGESTION_RULESET_VERSION,
        );
        let base_person = regulated_full_earth_person(world_id, 0x401, 10_000_000, 1_000_000);
        let object_id = EntityId::deterministic(world_id, b"oral-transfer-water");
        let material = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("citable water");
        let profile = OralTransferCommitment {
            commitment_schema_version: world_domain::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
            profile_id: "water-human-engineering-fixture-v1".to_owned(),
            profile_digest: Digest::sha256(b"ruleset twelve water response fixture"),
            material: material.clone(),
            species: base_person.species.clone(),
            evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
            transfer_mass_milligrams: 250_000,
            recoverable_energy_joules: 100,
            hydration_recovery_seconds: 200,
        };
        let build_held_state = |initial_age_ticks| {
            let mut person = base_person.clone();
            person.initial_age_ticks = initial_age_ticks;
            let initial = EngineState::new(manifest.clone());
            let mut genesis_events = initial
                .plan_configured_genesis(
                    environmental_provisional_full_earth_configuration(),
                    vec![person.clone()],
                )
                .expect("ingestion genesis plan");
            genesis_events.push(DomainEvent::MaterialInstanceInitialized {
                object_id,
                material: material.clone(),
                embodied_patch: person.embodied_patch.expect("person patch"),
                initial_mass_milligrams: Some(250_000),
                oral_transfer_profiles: vec![profile.clone()],
            });
            let (running, genesis) = initial
                .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
                .expect("ingestion genesis");
            let grasp_events = running
                .plan_action(
                    person.organism_id,
                    PrimitiveAction {
                        kind: PrimitiveActionKind::Grasp,
                        target_id: Some(object_id),
                        intensity: 1,
                    },
                )
                .expect("grasp portion");
            let (held, grasp) = running
                .commit(EventSequence::new(2), genesis.batch_hash, grasp_events)
                .expect("grasp commit");
            (person, held, genesis, grasp)
        };

        let (probe_person, probe_state, _, _) = build_held_state(0);
        let probe_organism = probe_state
            .organisms
            .get(&probe_person.organism_id)
            .expect("probe organism");
        let swallow_age = (1..=256)
            .find(|age_ticks| {
                probe_state
                    .deterministic_policy_action(probe_organism, *age_ticks)
                    .is_ok_and(|action| action.kind == PrimitiveActionKind::Swallow)
            })
            .expect("seeded policy eventually selects the innate swallow motion");

        let (person, held, genesis, grasp) = build_held_state(swallow_age - 1);
        assert_eq!(
            genesis.event_schema_version,
            MATERIAL_INGESTION_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            held.state_hash_schema_version(),
            MATERIAL_INGESTION_STATE_HASH_SCHEMA_VERSION
        );
        let held_organism = held
            .organisms
            .get(&person.organism_id)
            .expect("held-state organism");
        let profiled_candidates = held
            .deterministic_policy_candidates(held_organism, swallow_age)
            .expect("profiled policy candidates");
        let mut hidden_profile_state = held.clone();
        hidden_profile_state
            .material_instances
            .get_mut(&object_id)
            .expect("material instance")
            .oral_transfer_profiles
            .clear();
        let hidden_profile_candidates = hidden_profile_state
            .deterministic_policy_candidates(
                hidden_profile_state
                    .organisms
                    .get(&person.organism_id)
                    .expect("hidden-profile organism"),
                swallow_age,
            )
            .expect("hidden-profile policy candidates");
        assert_eq!(
            profiled_candidates, hidden_profile_candidates,
            "the action policy must not inspect the material response profile"
        );
        let mut sub_portion_state = held.clone();
        sub_portion_state
            .material_instances
            .get_mut(&object_id)
            .expect("sub-portion material")
            .remaining_mass_milligrams = Some(1);
        sub_portion_state
            .validate()
            .expect("a conserved remainder smaller than one profile portion remains valid");
        assert_eq!(
            sub_portion_state
                .resolve_oral_transfer(person.organism_id, object_id)
                .expect("sub-portion resolution"),
            None,
            "a whole portion must fit before any mass or recovery transfers"
        );

        let manual_swallow = held
            .plan_action(
                person.organism_id,
                PrimitiveAction {
                    kind: PrimitiveActionKind::Swallow,
                    target_id: Some(object_id),
                    intensity: 1,
                },
            )
            .expect("resolved manual swallow");
        assert!(matches!(
            held.commit(EventSequence::new(3), grasp.batch_hash, manual_swallow),
            Err(EngineError::InvalidMaterialOralTransfer(id)) if id == object_id
        ));

        let tick_events = held
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(300),
                CartesianMillimetres::new(1, 2, 3),
                CartesianMillimetres::new(4, 5, 6),
            ))
            .expect("ingestion tick");
        let action_index = tick_events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::OrganismActed { organism_id, action }
                        if *organism_id == person.organism_id
                            && action.kind == PrimitiveActionKind::Swallow
                            && action.target_id == Some(object_id)
                )
            })
            .expect("scheduled swallow action");
        let transfer_index = tick_events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::MaterialOralPortionTransferred {
                        object_id: transferred_object,
                        organism_id,
                        from_mass_milligrams: 250_000,
                        transferred_mass_milligrams: 250_000,
                        to_mass_milligrams: 0,
                        ..
                    } if *transferred_object == object_id && *organism_id == person.organism_id
                )
            })
            .expect("resolved oral mass transfer");
        let needs_index = tick_events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::OrganismNeedsChanged { organism_id, to, .. }
                        if *organism_id == person.organism_id
                            && to.energy_load_scaled_joules == 200
                            && to.hydration_load_seconds == 100
                )
            })
            .expect("exact energy and hydration recovery");
        assert!(action_index < transfer_index && transfer_index < needs_index);

        let (after_tick, tick) = held
            .commit(EventSequence::new(3), grasp.batch_hash, tick_events)
            .expect("ingestion tick commit");
        let consumed = after_tick
            .material_instances
            .get(&object_id)
            .expect("retained depleted material identity");
        assert_eq!(consumed.remaining_mass_milligrams(), Some(0));
        assert_eq!(consumed.held_by(), None);
        assert!(
            after_tick
                .deterministic_policy_candidates(
                    after_tick
                        .organisms
                        .get(&person.organism_id)
                        .expect("post-transfer organism"),
                    swallow_age + 1,
                )
                .expect("post-transfer candidates")
                .iter()
                .all(|candidate| candidate.action.target_id != Some(object_id))
        );
        let snapshot = Snapshot::new(after_tick.clone(), tick.sequence, tick.batch_hash)
            .expect("ingestion snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            MATERIAL_INGESTION_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("ingestion snapshot verifies");
        let mut downgraded_snapshot = snapshot;
        downgraded_snapshot.snapshot_schema_version = DETERMINISTIC_POLICY_SNAPSHOT_SCHEMA_VERSION;
        assert!(matches!(
            downgraded_snapshot.verify_integrity(),
            Err(EngineError::SnapshotSchemaMismatch {
                expected: MATERIAL_INGESTION_SNAPSHOT_SCHEMA_VERSION,
                actual: DETERMINISTIC_POLICY_SNAPSHOT_SCHEMA_VERSION,
            })
        ));
        assert_eq!(
            replay(manifest.clone(), &[genesis.clone(), grasp.clone(), tick])
                .expect("ingestion replay")
                .state,
            after_tick
        );

        let downgraded = EventBatch::new(
            DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            MATERIAL_INGESTION_RULESET_VERSION,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"downgraded ingestion state"),
        )
        .expect("internally valid pre-ingestion batch");
        assert!(matches!(
            replay(manifest, &[downgraded]),
            Err(EngineError::BatchEventSchemaMismatch {
                expected: MATERIAL_INGESTION_EVENT_SCHEMA_VERSION,
                actual: DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION,
            })
        ));
    }

    #[test]
    fn ruleset_thirteen_learns_only_from_bodily_pressure_and_replays() {
        let relieved = BodilyNeedState {
            energy_deficit: 50_000,
            hydration_deficit: 40_000,
            fatigue: 10_000,
            ..BodilyNeedState::default()
        };
        let after_relief = BodilyNeedState {
            energy_deficit: 20_000,
            hydration_deficit: 10_000,
            fatigue: 11_000,
            ..BodilyNeedState::default()
        };
        assert_eq!(action_outcome_reward(relieved, after_relief), 32);
        assert_eq!(action_outcome_reward(after_relief, relieved), -32);
        assert_eq!(
            learned_candidate_weight(
                2,
                Some(ActionValueState {
                    value_schema_version: ACTION_VALUE_STATE_SCHEMA_VERSION,
                    action_kind: PrimitiveActionKind::Swallow,
                    observations: 2,
                    value: 64,
                })
            ),
            10
        );
        assert_eq!(
            learned_candidate_weight(
                2,
                Some(ActionValueState {
                    value_schema_version: ACTION_VALUE_STATE_SCHEMA_VERSION,
                    action_kind: PrimitiveActionKind::Swallow,
                    observations: 2,
                    value: -64,
                })
            ),
            1
        );

        let world_id = WorldId::from_uuid(Uuid::from_u128(0x117));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012822),
            ACTION_LEARNING_RULESET_VERSION,
        );
        let person = regulated_full_earth_person(world_id, 0x501, 10_000_000, 1_000_000);
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![person.clone()],
            )
            .expect("learning genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("learning genesis");
        assert_eq!(
            genesis.event_schema_version,
            ACTION_LEARNING_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            running.state_hash_schema_version(),
            ACTION_LEARNING_STATE_HASH_SCHEMA_VERSION
        );

        let organism = running
            .organisms
            .get(&person.organism_id)
            .expect("learning organism");
        let (no_prior, first_swallow_value) = running
            .next_action_value(
                organism,
                PrimitiveActionKind::Swallow,
                relieved,
                after_relief,
            )
            .expect("first action outcome");
        assert_eq!(no_prior, None);
        assert_eq!(first_swallow_value.observations, 1);
        assert_eq!(first_swallow_value.value, 32);
        let (_, first_rest_value) = running
            .next_action_value(organism, PrimitiveActionKind::Rest, relieved, after_relief)
            .expect("same bodily outcome for another primitive act");
        assert_eq!(
            first_swallow_value.value, first_rest_value.value,
            "the update must not encode which action is supposed to solve a need"
        );

        let unscheduled_action = running
            .plan_action(
                person.organism_id,
                PrimitiveAction {
                    kind: PrimitiveActionKind::Orient,
                    target_id: None,
                    intensity: 1,
                },
            )
            .expect("primitive action plan");
        assert!(matches!(
            running.commit(
                EventSequence::new(2),
                genesis.batch_hash,
                unscheduled_action
            ),
            Err(EngineError::InvalidActionValueTransition(id)) if id == person.organism_id
        ));

        let expected_action = running
            .deterministic_policy_action(organism, 1)
            .expect("learning policy action");
        let events = running
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(300),
                CartesianMillimetres::new(1, 2, 3),
                CartesianMillimetres::new(4, 5, 6),
            ))
            .expect("learning tick");
        let action_index = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::OrganismActed { organism_id, action }
                        if *organism_id == person.organism_id && *action == expected_action
                )
            })
            .expect("scheduled primitive action");
        let needs_index = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::OrganismNeedsChanged { organism_id, .. }
                        if *organism_id == person.organism_id
                )
            })
            .expect("same-tick body outcome");
        let learned = events
            .iter()
            .enumerate()
            .find_map(|(index, event)| match event {
                DomainEvent::OrganismActionValueChanged {
                    organism_id,
                    from: None,
                    to,
                } if *organism_id == person.organism_id => Some((index, *to)),
                _ => None,
            })
            .expect("same-tick action-value observation");
        assert!(action_index < needs_index && needs_index < learned.0);
        assert_eq!(learned.1.action_kind, expected_action.kind);
        assert_eq!(learned.1.observations, 1);
        let mut reordered = events.clone();
        reordered.swap(action_index, needs_index);
        assert!(matches!(
            running.commit(EventSequence::new(2), genesis.batch_hash, reordered),
            Err(EngineError::InvalidActionValueTransition(id)) if id == person.organism_id
        ));
        let body_to = events
            .iter()
            .find_map(|event| match event {
                DomainEvent::OrganismNeedsChanged { to, .. } => Some(*to),
                _ => None,
            })
            .expect("body transition");
        assert_eq!(
            learned.1.value,
            action_outcome_reward(BodilyNeedState::default(), body_to.needs)
        );

        let mut fabricated = events.clone();
        let fabricated_value = fabricated
            .iter_mut()
            .find_map(|event| match event {
                DomainEvent::OrganismActionValueChanged {
                    organism_id, to, ..
                } if *organism_id == person.organism_id => Some(&mut to.value),
                _ => None,
            })
            .expect("fabricated action-value target");
        *fabricated_value = if *fabricated_value < ACTION_VALUE_MAX {
            *fabricated_value + 1
        } else {
            *fabricated_value - 1
        };
        assert!(matches!(
            running.commit(EventSequence::new(2), genesis.batch_hash, fabricated),
            Err(EngineError::InvalidActionValueTransition(id)) if id == person.organism_id
        ));

        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, events)
            .expect("learning tick commit");
        let learned_organism = after_tick
            .organisms
            .get(&person.organism_id)
            .expect("learned organism");
        assert_eq!(
            learned_organism.action_value(expected_action.kind),
            Some(learned.1)
        );
        assert_eq!(
            learned_organism.action_values_updated_at,
            Some(SimTick::new(1))
        );
        let snapshot = Snapshot::new(after_tick.clone(), tick.sequence, tick.batch_hash)
            .expect("learning snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            ACTION_LEARNING_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("learning snapshot verifies");
        assert_eq!(
            replay(manifest.clone(), &[genesis.clone(), tick])
                .expect("learning replay")
                .state,
            after_tick
        );

        let downgraded = EventBatch::new(
            MATERIAL_INGESTION_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            ACTION_LEARNING_RULESET_VERSION,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"downgraded learning state"),
        )
        .expect("internally valid pre-learning batch");
        assert!(matches!(
            replay(manifest, &[downgraded]),
            Err(EngineError::BatchEventSchemaMismatch {
                expected: ACTION_LEARNING_EVENT_SCHEMA_VERSION,
                actual: MATERIAL_INGESTION_EVENT_SCHEMA_VERSION,
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
    fn embodied_activity_ruleset_emits_ordered_perception_and_action_at_each_barrier() {
        let mut manifest = manifest();
        manifest.ruleset_version = EMBODIED_ACTIVITY_RULESET_VERSION;
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                full_earth_configuration(),
                vec![full_earth_person(manifest.world_id)],
            )
            .expect("activity full-Earth genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("activity full-Earth genesis commit");
        let celestial = CelestialState::new(
            TdbSecondsSinceJ2000::new(123),
            CartesianMillimetres::new(1, 2, 3),
            CartesianMillimetres::new(4, 5, 6),
        );
        let events = running
            .plan_next_tick_with_celestial(celestial)
            .expect("activity tick plan");
        assert!(matches!(
            events.as_slice(),
            [
                DomainEvent::TickAdvanced { .. },
                DomainEvent::OrganismAgeAdvanced { .. },
                DomainEvent::OrganismPerceived { perception, .. },
                DomainEvent::OrganismActed { action, .. },
                DomainEvent::CelestialStateRecorded { state },
            ] if perception.readings[0].property_code == "body_clock_phase"
                && action.target_id.is_none()
                && action.intensity == 1
                && *state == celestial
        ));
        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, events)
            .expect("activity full-Earth tick commit");
        assert_eq!(after_tick.scheduled_work_count(), 1);
        assert_eq!(
            replay(manifest, &[genesis, tick])
                .expect("activity replay")
                .state,
            after_tick
        );
    }

    #[test]
    fn local_environment_ruleset_replays_source_bound_temperature_perceptions() {
        let mut manifest = manifest();
        manifest.ruleset_version = LOCAL_ENVIRONMENT_RULESET_VERSION;
        let initial = EngineState::new(manifest.clone());
        let (running, genesis) = initial
            .commit(
                EventSequence::new(1),
                Digest::ZERO,
                initial
                    .plan_configured_genesis(
                        environmental_provisional_full_earth_configuration(),
                        vec![full_earth_person(manifest.world_id)],
                    )
                    .expect("genesis"),
            )
            .expect("commit genesis");
        let celestial = CelestialState::new(
            TdbSecondsSinceJ2000::new(123),
            CartesianMillimetres::new(1, 2, 3),
            CartesianMillimetres::new(4, 5, 6),
        );
        let events = running
            .plan_next_tick_with_celestial(celestial)
            .expect("tick");
        assert!(events.iter().any(|event| matches!(event, DomainEvent::OrganismPerceived { perception, .. } if perception.readings[0].property_code == "temperature" && perception.readings[0].quantized_value == 2)));
        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, events)
            .expect("commit tick");
        assert_eq!(
            replay(manifest, &[genesis, tick]).expect("replay").state,
            after_tick
        );
    }

    #[test]
    fn resolved_movement_ruleset_replays_one_face_safe_relocation() {
        let mut manifest = manifest();
        manifest.ruleset_version = RESOLVED_MOVEMENT_RULESET_VERSION;
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![full_earth_person(manifest.world_id)],
            )
            .expect("genesis");
        let (mut state, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("commit genesis");
        let mut batches = vec![genesis];
        let mut moved = false;
        for sequence in 2..=5 {
            let events = state
                .plan_next_tick_with_celestial(CelestialState::new(
                    TdbSecondsSinceJ2000::new(i128::from(sequence)),
                    CartesianMillimetres::new(1, 2, 3),
                    CartesianMillimetres::new(4, 5, 6),
                ))
                .expect("tick");
            if let Some(DomainEvent::OrganismMoved {
                from_patch,
                to_patch,
                ..
            }) = events
                .iter()
                .find(|event| matches!(event, DomainEvent::OrganismMoved { .. }))
            {
                assert!(
                    s2_edge_neighbors(*from_patch)
                        .expect("neighbors")
                        .contains(to_patch)
                );
                moved = true;
            }
            let previous = batches.last().expect("previous batch").batch_hash;
            let (next, batch) = state
                .commit(EventSequence::new(sequence), previous, events)
                .expect("commit tick");
            state = next;
            batches.push(batch);
        }
        assert!(moved, "four motor phases include one move");
        assert_eq!(replay(manifest, &batches).expect("replay").state, state);
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
                        metabolic_rate: None,
                        physiological_regulation: None,
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
