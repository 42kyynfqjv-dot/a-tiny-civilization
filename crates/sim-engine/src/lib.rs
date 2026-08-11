//! Pure deterministic planning, state transitions, snapshots, and replay.

#[allow(dead_code)]
mod partition;
#[allow(dead_code)]
mod refinement;
#[allow(dead_code)]
mod spatial;

use std::collections::{BTreeMap, BTreeSet};

use partition::{
    Emission, PartitionOutput, PartitionSchedule, ScheduledWork, SchedulerError, SubjectKey,
    WorkKey, WorkOutput,
};
pub use partition::{PartitionCapacityProbe, run_partition_capacity_probe};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    ACTION_LEARNING_EVENT_SCHEMA_VERSION, ACTION_VALUE_MAX, ACTION_VALUE_MIN,
    ACTION_VALUE_STATE_SCHEMA_VERSION, ADULT_BODY_MASS_EVENT_SCHEMA_VERSION, ActionValueState,
    AdultBodyMassCommitment, BODILY_REGULATION_EVENT_SCHEMA_VERSION,
    BODY_PROVENANCE_EVENT_SCHEMA_VERSION, BirthCategory, BodilyNeedState, BodilyRegulationState,
    CANCER_BURDEN_EVENT_SCHEMA_VERSION, CANCER_RESEARCH_COHORT_EVENT_SCHEMA_VERSION,
    CANCER_RESEARCH_INITIAL_AFFECTED_RESIDENTS, CANCER_RESEARCH_INITIAL_RESIDENTS,
    CELESTIAL_STATE_EVENT_SCHEMA_VERSION, COGNITION_EVENT_SCHEMA_VERSION,
    COMPETITIVE_SIGNAL_ASSOCIATION_SCHEMA_VERSION,
    COMPETITIVE_SIGNAL_LEARNING_EVENT_SCHEMA_VERSION, CONFIGURED_EVENT_SCHEMA_VERSION,
    CancerBurdenState, CancerBurdenTransition, CanonicalHashError, CelestialState,
    CognitionDeadlineInput, CognitionInputOutcome, CognitionReading, CognitionRequestSelection,
    CognitionUnavailableReason, DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION, DeathCause, Digest,
    DomainEvent, EMBODIED_POSITION_EVENT_SCHEMA_VERSION, EVENT_SCHEMA_VERSION, EntityId,
    EventBatch, EventBatchError, EventSequence, ExecutionScale, GeographicRoutingError,
    HERITABLE_ACTION_KINDS, HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION,
    HERITABLE_DISPOSITION_SCHEMA_VERSION, HERITABLE_PROBABILITY_SCALE, HeritableActionWeight,
    HeritableDisposition, HeritableDispositionProfile, LEGACY_EVENT_SCHEMA_VERSION,
    LOCAL_ATMOSPHERIC_FLUX_EVENT_SCHEMA_VERSION, LOCAL_WEATHER_EVENT_SCHEMA_VERSION,
    MASS_SCALED_METABOLISM_EVENT_SCHEMA_VERSION, MATERIAL_HANDLING_EVENT_SCHEMA_VERSION,
    MATERIAL_INGESTION_EVENT_SCHEMA_VERSION, MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION,
    MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION, MATERIAL_SURFACE_REGIONS_EVENT_SCHEMA_VERSION,
    MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION, MAX_COGNITION_SELECTION_READINGS,
    MOVEMENT_DIRECTION_LEARNING_EVENT_SCHEMA_VERSION, MOVEMENT_DIRECTION_VALUE_SCHEMA_VERSION,
    MaterialIdentity, MaterialReservoirCommitment, MetabolicRateCommitment,
    MovementDirectionValueState, OralTransferCommitment, OrganismRole,
    PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION, PerceptionChannel, PhysiologicalRegulationCommitment,
    PrimitiveAction, PrimitiveActionKind, PropertyReading,
    REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION, REPRODUCTIVE_PROBABILITY_SCALE,
    ReproductiveDevelopmentEnd, ReproductivePhysiologyCommitment, S2CellId, S2CellIdError,
    SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION, SELECTABLE_MOVEMENT_EVENT_SCHEMA_VERSION,
    SIGNAL_ACTION_ASSOCIATION_EVENT_SCHEMA_VERSION, SIGNAL_ACTION_ASSOCIATION_SCHEMA_VERSION,
    SIGNAL_MOTOR_ASSOCIATION_EVENT_SCHEMA_VERSION, SIGNAL_MOTOR_ASSOCIATION_SCHEMA_VERSION,
    SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION, SOCIAL_LEARNING_EVENT_SCHEMA_VERSION,
    SequenceOverflow, SignalActionAssociationState, SimTick, SituatedPerception, SpeciesIdentity,
    SpeciesIdentityError, TERRAIN_MOVEMENT_EVENT_SCHEMA_VERSION,
    TOPSOIL_MOVEMENT_EVENT_SCHEMA_VERSION, TimeOverflow, WorldConfiguration,
    WorldConfigurationError, WorldExperimentCommitment, WorldId, WorldManifest, WorldManifestError,
    WorldSeed, WorldStatus, decode_s2_face_ij, s2_edge_neighbors,
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
/// Ruleset fourteen adds deterministic species-bound reproductive physiology and
/// private development whose only safe public outcome is an ordinary birth.
pub const REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION: u32 = 14;
/// Ruleset fifteen gives every organism an immutable, species-bound inherited
/// disposition over the neutral action grammar. Learned state remains life-local.
pub const HERITABLE_DISPOSITION_RULESET_VERSION: u32 = 15;
/// Ruleset sixteen adds deterministic world-total external-cognition request
/// selection and pending deadline state. Remote services remain optional inputs.
pub const COGNITION_RULESET_VERSION: u32 = 16;
/// Ruleset seventeen adds bounded, spatially anchored real-material reservoirs,
/// ordered shared transfers, and deterministic replenishment.
pub const MATERIAL_RESERVOIR_RULESET_VERSION: u32 = 17;
/// Ruleset eighteen lets one organism directly witness at most one co-located
/// organism's label-free primitive action per tick and retain a bounded tendency.
pub const SOCIAL_LEARNING_RULESET_VERSION: u32 = 18;
/// Ruleset nineteen lets primitive force leave a bounded, directly perceptible trace
/// on a held object without assigning it an artifact, symbol, tool, or use label.
pub const MATERIAL_SURFACE_TRACE_RULESET_VERSION: u32 = 19;
/// Ruleset twenty retains eight independently addressable, label-free contact
/// regions on each material object. Regions are physical motor coordinates only.
pub const MATERIAL_SURFACE_REGIONS_RULESET_VERSION: u32 = 20;
/// Ruleset twenty-one gives neutral local sound eight selectable physical
/// intensities. The values carry no token, word, meaning, or purpose.
pub const ACOUSTIC_VARIATION_RULESET_VERSION: u32 = 21;
/// Ruleset twenty-two lets an organism privately associate a sound amplitude heard
/// from another organism with that organism's directly witnessed next action.
pub const SIGNAL_ACTION_ASSOCIATION_RULESET_VERSION: u32 = 22;
/// Ruleset twenty-three makes the four adjacent movement motor directions
/// selectable without exposing a map, place, or destination label.
pub const SELECTABLE_MOVEMENT_RULESET_VERSION: u32 = 23;
/// Ruleset twenty-four lets an organism retain bounded bodily-outcome experience
/// independently for each adjacent movement motor coordinate.
pub const MOVEMENT_DIRECTION_LEARNING_RULESET_VERSION: u32 = 24;
/// Ruleset twenty-five lets a heard amplitude predict an exact movement motor
/// coordinate when that coordinate was directly witnessed after the sound.
pub const SIGNAL_MOTOR_ASSOCIATION_RULESET_VERSION: u32 = 25;
/// Ruleset twenty-six reserves scarce external cognition for people. Fauna keep
/// the complete deterministic embodied policy, learning, communication, and
/// reproduction paths but cannot consume the civilization's model budget.
pub const PERSON_COGNITION_RULESET_VERSION: u32 = 26;
/// Ruleset twenty-seven derives replay-stable, smoothly interpolated local
/// physical weather readings from the source-bound ERA5 normal-period contract.
pub const LOCAL_WEATHER_RULESET_VERSION: u32 = 27;
/// Ruleset twenty-eight exposes label-free local water-flux and air-motion
/// magnitudes from the source-bound atmospheric contract.
pub const LOCAL_ATMOSPHERIC_FLUX_RULESET_VERSION: u32 = 28;
/// Ruleset twenty-nine makes moving across the provisional local terrain range
/// increase private bodily fatigue without exposing terrain or altitude labels.
pub const TERRAIN_MOVEMENT_RULESET_VERSION: u32 = 29;
/// Ruleset thirty additionally applies the source-bound topsoil coarse-fragment
/// median to private movement fatigue without exposing a soil or surface label.
pub const TOPSOIL_MOVEMENT_RULESET_VERSION: u32 = 30;
/// Ruleset thirty-one replaces universal organism energy quantities with
/// body-mass-scaled, source-addressed commitments fixed at genesis.
pub const MASS_SCALED_METABOLISM_RULESET_VERSION: u32 = 31;
/// Ruleset thirty-two makes each exact adult-mass commitment durable canonical
/// organism state so later physical couplings never infer it from derived power.
pub const ADULT_BODY_MASS_STATE_RULESET_VERSION: u32 = 32;
/// Ruleset thirty-three prevents reproductive pair formation between close
/// genealogical relatives through first cousins. The relation is private
/// physiology state, not an in-world kinship label or learned social rule.
pub const CLOSE_KIN_EXCLUSION_RULESET_VERSION: u32 = 33;
/// Ruleset thirty-four lets an organism imitate a directly heard physical signal
/// form and reuse a privately learned form when its associated motor action is
/// strongly weighted by the organism's current embodied context. It adds no word,
/// referent, intention, or observer-authored meaning.
pub const SIGNAL_CONVENTION_REUSE_RULESET_VERSION: u32 = 34;
/// Ruleset thirty-five replaces exact-cell-only hearing and social attention with a
/// bounded local landscape neighborhood, and gives movement a neutral tendency to
/// reduce distance to the nearest directly heard signal source. Reproduction
/// still requires exact embodied contact and remains private physiology.
pub const LOCAL_INTERACTION_RULESET_VERSION: u32 = 35;
/// Ruleset thirty-six replaces cumulative positive-only signal co-occurrence with
/// grounded prediction-error learning: directly supported mappings strengthen,
/// one strongest incompatible hypothesis weakens, coordinated imitation receives
/// additional reinforcement, and only distinctive mappings bias later behavior.
pub const GROUNDED_PREDICTIVE_COGNITION_RULESET_VERSION: u32 = 36;
/// Ruleset thirty-seven admits an explicitly artificial Cancer World bootstrap:
/// fluent English speech/literacy/publication, private cancer-state awareness,
/// abundant survival resources, and one overriding cure objective. It does not
/// itself encode oncology facts, experiments, treatments, or successful outcomes.
pub const CANCER_RESEARCH_WORLD_RULESET_VERSION: u32 = 37;
/// Ruleset thirty-eight adds deterministic private cancer-burden state and one
/// replayable progression transition per simulated day. Its provisional numeric
/// parameters remain implementation assumptions pending scientific validation.
pub const CANCER_BIOLOGY_RULESET_VERSION: u32 = 38;
/// The already-running public ruleset-33 world receives the stateless ruleset-34
/// policy driver at this disclosed boundary. Earlier ruleset-33 transitions retain
/// their exact candidate set and replay behavior.
pub const RULESET_33_SIGNAL_CONVENTION_ACTIVATION_TICK: u64 = 65_000;
/// The running public ruleset-33 world receives the stateless local-interaction
/// driver at this disclosed boundary. Earlier transitions remain byte-for-byte
/// replayable under their original exact-cell behavior.
pub const RULESET_33_LOCAL_INTERACTION_ACTIVATION_TICK: u64 = 75_000;
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
pub const REPRODUCTIVE_PHYSIOLOGY_SNAPSHOT_SCHEMA_VERSION: u16 = 17;
pub const HERITABLE_DISPOSITION_SNAPSHOT_SCHEMA_VERSION: u16 = 18;
pub const COGNITION_SNAPSHOT_SCHEMA_VERSION: u16 = 19;
pub const MATERIAL_RESERVOIR_SNAPSHOT_SCHEMA_VERSION: u16 = 20;
pub const SOCIAL_LEARNING_SNAPSHOT_SCHEMA_VERSION: u16 = 21;
pub const MATERIAL_SURFACE_TRACE_SNAPSHOT_SCHEMA_VERSION: u16 = 22;
pub const MATERIAL_SURFACE_REGIONS_SNAPSHOT_SCHEMA_VERSION: u16 = 23;
pub const SIGNAL_ACTION_ASSOCIATION_SNAPSHOT_SCHEMA_VERSION: u16 = 24;
pub const MOVEMENT_DIRECTION_LEARNING_SNAPSHOT_SCHEMA_VERSION: u16 = 25;
pub const SIGNAL_MOTOR_ASSOCIATION_SNAPSHOT_SCHEMA_VERSION: u16 = 26;
pub const LOCAL_WEATHER_SNAPSHOT_SCHEMA_VERSION: u16 = 27;
pub const LOCAL_ATMOSPHERIC_FLUX_SNAPSHOT_SCHEMA_VERSION: u16 = 28;
pub const TERRAIN_MOVEMENT_SNAPSHOT_SCHEMA_VERSION: u16 = 29;
pub const TOPSOIL_MOVEMENT_SNAPSHOT_SCHEMA_VERSION: u16 = 30;
pub const MASS_SCALED_METABOLISM_SNAPSHOT_SCHEMA_VERSION: u16 = 31;
pub const ADULT_BODY_MASS_SNAPSHOT_SCHEMA_VERSION: u16 = 32;
pub const WORLD_EXPERIMENT_SNAPSHOT_SCHEMA_VERSION: u16 = 33;
pub const CANCER_RESEARCH_COHORT_SNAPSHOT_SCHEMA_VERSION: u16 = 34;
pub const CANCER_BURDEN_SNAPSHOT_SCHEMA_VERSION: u16 = 35;
/// External work receives this fixed simulated-time window. Wall-clock latency can
/// decide only whether the result is present by the deadline, never move the deadline.
pub const COGNITION_RESPONSE_WINDOW_TICKS: u64 = 60;
pub const COGNITION_MEMORY_MAX_TOKENS: u32 = 512;
pub const COGNITION_MODEL_MAX_OUTPUT_TOKENS: u16 = 32;
const COGNITION_REQUEST_ORDINAL: u32 = 0;
const COGNITION_ACTION_WEIGHT_BONUS: u32 = 2;
const SIGNAL_IMITATION_WEIGHT_BONUS: u32 = 16;
const SIGNAL_CONTEXT_REUSE_MAX_BONUS: u32 = 24;
const SIGNAL_PREDICTION_REINFORCEMENT: i16 = 4;
const SIGNAL_COORDINATION_REINFORCEMENT: i16 = 8;
const SIGNAL_PREDICTION_INHIBITION: i16 = 2;
const LOCAL_COHESION_WEIGHT_BONUS: u32 = 24;
const MAX_LOCAL_SIGNAL_RECIPIENTS: usize = 8;
const COGNITION_MEMORY_QUERY_V1: &str =
    "recent direct experiences matching current bodily pressure and situated property readings";
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
const REPRODUCTIVE_PHYSIOLOGY_STATE_HASH_SCHEMA_VERSION: u16 = 17;
const HERITABLE_DISPOSITION_STATE_HASH_SCHEMA_VERSION: u16 = 18;
const COGNITION_STATE_HASH_SCHEMA_VERSION: u16 = 19;
const MATERIAL_RESERVOIR_STATE_HASH_SCHEMA_VERSION: u16 = 20;
const SOCIAL_LEARNING_STATE_HASH_SCHEMA_VERSION: u16 = 21;
const MATERIAL_SURFACE_TRACE_STATE_HASH_SCHEMA_VERSION: u16 = 22;
const MATERIAL_SURFACE_REGIONS_STATE_HASH_SCHEMA_VERSION: u16 = 23;
const SIGNAL_ACTION_ASSOCIATION_STATE_HASH_SCHEMA_VERSION: u16 = 24;
const LOCAL_WEATHER_STATE_HASH_SCHEMA_VERSION: u16 = 27;
const LOCAL_ATMOSPHERIC_FLUX_STATE_HASH_SCHEMA_VERSION: u16 = 28;
const TERRAIN_MOVEMENT_STATE_HASH_SCHEMA_VERSION: u16 = 29;
const TOPSOIL_MOVEMENT_STATE_HASH_SCHEMA_VERSION: u16 = 30;
const MASS_SCALED_METABOLISM_STATE_HASH_SCHEMA_VERSION: u16 = 31;
const ADULT_BODY_MASS_STATE_HASH_SCHEMA_VERSION: u16 = 32;
const CANCER_RESEARCH_COHORT_STATE_HASH_SCHEMA_VERSION: u16 = 34;
const CANCER_BURDEN_STATE_HASH_SCHEMA_VERSION: u16 = 35;
const MATERIAL_SURFACE_REGION_COUNT: usize = 8;
const SIGNAL_INTENSITY_VARIANT_COUNT: u16 = world_domain::SIGNAL_FORM_VARIANT_COUNT as u16;
const MAX_SIGNAL_ACTION_ASSOCIATIONS: usize =
    world_domain::SIGNAL_FORM_VARIANT_COUNT as usize * HERITABLE_ACTION_KINDS.len();
const MAX_SIGNAL_MOTOR_ASSOCIATIONS: usize =
    world_domain::SIGNAL_FORM_VARIANT_COUNT as usize * (HERITABLE_ACTION_KINDS.len() + 3);
const MAX_MATERIAL_SURFACE_TRACE_UNITS: u32 = i32::MAX.unsigned_abs();
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

fn surface_region_property_code(contact_region: u8) -> String {
    format!("surface_region_{contact_region}")
}

fn surface_region_perception(
    object_id: EntityId,
    contact_region: u8,
    region_trace_units: u32,
    total_trace_units: u32,
) -> SituatedPerception {
    SituatedPerception {
        subject_id: Some(object_id),
        readings: vec![
            PropertyReading {
                channel: PerceptionChannel::Touch,
                property_code: surface_region_property_code(contact_region),
                quantized_value: i32::try_from(region_trace_units)
                    .expect("surface-region trace is bounded to i32"),
                uncertainty: 0,
            },
            PropertyReading {
                channel: PerceptionChannel::Touch,
                property_code: "surface_trace".to_owned(),
                quantized_value: i32::try_from(total_trace_units)
                    .expect("surface trace is bounded to i32"),
                uncertainty: 0,
            },
        ],
    }
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

const fn is_zero_u32(value: &u32) -> bool {
    *value == 0
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

/// Apply the provisional L10 relief range as a dimensionless movement-load
/// multiplier relative to one kilometre. This never constructs a slope, path, or
/// terrain label and uses only the source-bound extrema committed at genesis.
fn terrain_adjusted_movement_exposure(
    baseline_exposure: u128,
    minimum_millimetres: i64,
    maximum_millimetres: i64,
) -> Result<u128, EngineError> {
    const REFERENCE_RELIEF_MILLIMETRES: u128 = 1_000_000;
    let spread = i128::from(maximum_millimetres)
        .checked_sub(i128::from(minimum_millimetres))
        .filter(|spread| *spread >= 0)
        .ok_or_else(|| {
            EngineError::PhysiologicalArithmetic(
                "terrain relief range is inverted or overflowed".to_owned(),
            )
        })?;
    let spread = u128::try_from(spread).expect("nonnegative i128 fits u128");
    let additional = baseline_exposure.checked_mul(spread).ok_or_else(|| {
        EngineError::PhysiologicalArithmetic(
            "terrain movement exposure numerator overflowed".to_owned(),
        )
    })? / REFERENCE_RELIEF_MILLIMETRES;
    baseline_exposure.checked_add(additional).ok_or_else(|| {
        EngineError::PhysiologicalArithmetic("terrain movement exposure overflowed".to_owned())
    })
}

/// Apply the median coarse-fragment volume fraction from the schema-pinned
/// SoilGrids property order. The source domain is cubic centimetres per cubic
/// decimetre, so 1,000 is the complete volume. This remains a provisional
/// regional movement-load approximation and creates no organism-facing label.
fn topsoil_adjusted_movement_exposure(
    baseline_exposure: u128,
    topsoil_source_quantiles: &[[i16; 3]; 9],
) -> Result<u128, EngineError> {
    const COARSE_FRAGMENT_PROPERTY_INDEX: usize = 2;
    const COMPLETE_VOLUME_SOURCE_UNITS: u128 = 1_000;
    let median = topsoil_source_quantiles[COARSE_FRAGMENT_PROPERTY_INDEX][1];
    let median = u128::try_from(median).map_err(|_| {
        EngineError::PhysiologicalArithmetic(
            "topsoil coarse-fragment median is negative".to_owned(),
        )
    })?;
    if median > COMPLETE_VOLUME_SOURCE_UNITS {
        return Err(EngineError::PhysiologicalArithmetic(
            "topsoil coarse-fragment median exceeds complete volume".to_owned(),
        ));
    }
    let additional = baseline_exposure.checked_mul(median).ok_or_else(|| {
        EngineError::PhysiologicalArithmetic(
            "topsoil movement exposure numerator overflowed".to_owned(),
        )
    })? / COMPLETE_VOLUME_SOURCE_UNITS;
    baseline_exposure.checked_add(additional).ok_or_else(|| {
        EngineError::PhysiologicalArithmetic("topsoil movement exposure overflowed".to_owned())
    })
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CognitionMotorPreference {
    action_kind: PrimitiveActionKind,
    contact_region: Option<u8>,
    signal_intensity: Option<u8>,
    movement_direction: Option<u8>,
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

#[derive(Serialize)]
struct SocialAttentionDraw {
    social_attention_version: u16,
    world_seed: u64,
    observer_id: EntityId,
    tick: SimTick,
    co_located_actor_digest: Digest,
}

#[derive(Serialize)]
struct CognitionSubjectDraw<'a> {
    driver_version: u16,
    world_seed: u64,
    selected_at_tick: SimTick,
    living_organism_ids: &'a [EntityId],
}

#[derive(Serialize)]
struct ReproductiveDraw<'a> {
    driver_version: u16,
    stream: &'static str,
    world_seed: u64,
    tick: SimTick,
    parent_ids: &'a [EntityId],
}

#[derive(Serialize)]
struct HeritableDispositionDraw<'a> {
    driver_version: u16,
    stream: &'static str,
    world_seed: u64,
    derived_at: SimTick,
    organism_id: EntityId,
    parent_ids: &'a [EntityId],
    profile_fingerprint: Digest,
    action_kind: PrimitiveActionKind,
}

#[derive(Serialize)]
struct LocalTemperatureDraw {
    driver_version: u16,
    world_seed: u64,
    source_normals_digest: Digest,
    day_index: u64,
}

#[derive(Serialize)]
struct LocalWaterFluxDraw {
    driver_version: u16,
    world_seed: u64,
    source_normals_digest: Digest,
    normal_phase: u8,
    paired_day_index: u64,
}

fn first_digest_u64(digest: Digest) -> u64 {
    let bytes: [u8; 8] = digest.as_bytes()[..8]
        .try_into()
        .expect("SHA-256 has at least eight bytes");
    u64::from_be_bytes(bytes)
}

fn second_digest_u64(digest: Digest) -> u64 {
    let bytes: [u8; 8] = digest.as_bytes()[8..16]
        .try_into()
        .expect("SHA-256 has at least sixteen bytes");
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

fn associated_candidate_weight(base: u32, value: Option<SignalActionAssociationState>) -> u32 {
    value.map_or(base, |value| {
        base.saturating_add(u32::from(value.value.unsigned_abs()).div_ceil(8))
    })
}

fn context_reuse_candidate_weight(
    base: u32,
    context_weight: u32,
    association: Option<SignalActionAssociationState>,
) -> u32 {
    association.map_or(base, |association| {
        let confidence = u32::from(association.value.unsigned_abs()).div_ceil(8)
            + association.observations.div_ceil(4)
            + context_weight.min(8);
        base.saturating_add(confidence.min(SIGNAL_CONTEXT_REUSE_MAX_BONUS))
    })
}

fn signal_convention_candidate_weight(
    base: u32,
    signal_intensity: u8,
    recent_signal: Option<u8>,
    context_weight: u32,
    association: Option<SignalActionAssociationState>,
) -> u32 {
    let imitative = if recent_signal == Some(signal_intensity) {
        base.saturating_add(SIGNAL_IMITATION_WEIGHT_BONUS)
    } else {
        base
    };
    context_reuse_candidate_weight(
        imitative,
        context_weight,
        association.filter(|association| association.signal_intensity == signal_intensity),
    )
}

fn competitive_signal_convention_candidate_weight(
    base: u32,
    signal_intensity: u8,
    recent_signal: Option<u8>,
    distinctive_strength: Option<u32>,
) -> u32 {
    let imitative = if recent_signal == Some(signal_intensity) {
        base.saturating_add(SIGNAL_IMITATION_WEIGHT_BONUS)
    } else {
        base
    };
    imitative.saturating_add(
        distinctive_strength
            .unwrap_or(0)
            .min(SIGNAL_CONTEXT_REUSE_MAX_BONUS),
    )
}

fn signal_convention_reuse_active(ruleset_version: u32, tick: SimTick) -> bool {
    ruleset_version >= SIGNAL_CONVENTION_REUSE_RULESET_VERSION
        || (ruleset_version == CLOSE_KIN_EXCLUSION_RULESET_VERSION
            && tick.get() >= RULESET_33_SIGNAL_CONVENTION_ACTIVATION_TICK)
}

fn local_interaction_active(ruleset_version: u32, tick: SimTick) -> bool {
    ruleset_version >= LOCAL_INTERACTION_RULESET_VERSION
        || (ruleset_version == CLOSE_KIN_EXCLUSION_RULESET_VERSION
            && tick.get() >= RULESET_33_LOCAL_INTERACTION_ACTIVATION_TICK)
}

fn apply_local_cohesion_weights(
    patch: S2CellId,
    target_patch: S2CellId,
    candidates: &mut [PolicyCandidate],
) -> Result<(), EngineError> {
    if target_patch == patch {
        return Ok(());
    }
    let current_distance = EngineState::patch_grid_distance(patch, target_patch);
    let neighbors = s2_edge_neighbors(patch)?;
    for candidate in candidates {
        let Some(direction) = candidate.action.movement_direction else {
            continue;
        };
        if EngineState::patch_grid_distance(neighbors[usize::from(direction)], target_patch)
            < current_distance
        {
            candidate.weight = candidate
                .weight
                .checked_add(LOCAL_COHESION_WEIGHT_BONUS)
                .ok_or(EngineError::TooManyEvents)?;
        }
    }
    Ok(())
}

fn inherited_candidate_weight(
    base: u32,
    disposition: &HeritableDisposition,
    action_kind: PrimitiveActionKind,
) -> Result<u32, EngineError> {
    let inherited = disposition
        .action_weight(action_kind)
        .ok_or(EngineError::InvalidHeritableDisposition)?;
    let numerator = u64::from(base)
        .checked_mul(u64::from(inherited))
        .ok_or(EngineError::TooManyEvents)?;
    u32::try_from(numerator).map_err(|_| EngineError::TooManyEvents)
}

fn bounded_mutated_weight(
    weight: u16,
    magnitude: u16,
    increase: bool,
    profile: &HeritableDispositionProfile,
) -> u16 {
    if increase {
        weight.saturating_add(magnitude)
    } else {
        weight.saturating_sub(magnitude)
    }
    .clamp(profile.minimum_action_weight, profile.maximum_action_weight)
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
    pub adult_body_mass: Option<AdultBodyMassCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physiological_regulation: Option<PhysiologicalRegulationCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reproductive_physiology: Option<ReproductivePhysiologyCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heritable_disposition_profile: Option<HeritableDispositionProfile>,
}

/// One explicitly planned real-material instance committed with genesis.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InitialMaterialInstance {
    pub object_id: EntityId,
    pub material: MaterialIdentity,
    pub embodied_patch: S2CellId,
    pub initial_mass_milligrams: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oral_transfer_profiles: Vec<OralTransferCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reservoir: Option<MaterialReservoirCommitment>,
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
    adult_body_mass: Option<AdultBodyMassCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    physiological_regulation: Option<PhysiologicalRegulationCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reproductive_physiology: Option<ReproductivePhysiologyCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reproductive_available_at: Option<SimTick>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    heritable_disposition_profile: Option<HeritableDispositionProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    heritable_disposition: Option<HeritableDisposition>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    movement_direction_values: Vec<MovementDirectionValueState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    movement_direction_values_updated_at: Option<SimTick>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    social_action_values: Vec<ActionValueState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    social_action_values_updated_at: Option<SimTick>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    signal_action_associations: Vec<SignalActionAssociationState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signal_action_associations_updated_at: Option<SimTick>,
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
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    surface_trace_units: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    surface_region_trace_units: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    oral_transfer_profiles: Vec<OralTransferCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reservoir: Option<MaterialReservoirCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reservoir_settled_at: Option<SimTick>,
}

/// Private canonical state for one development that may later resolve as a birth.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PendingReproductiveDevelopment {
    development_id: EntityId,
    offspring_id: EntityId,
    species: SpeciesIdentity,
    role: OrganismRole,
    birth_category: BirthCategory,
    parent_ids: Vec<EntityId>,
    developing_parent_id: EntityId,
    profile_digest: Digest,
    started_at: SimTick,
    due_tick: SimTick,
    parents_available_at: SimTick,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    heritable_disposition_profile: Option<HeritableDispositionProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    offspring_heritable_disposition: Option<HeritableDisposition>,
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

    #[must_use]
    pub const fn surface_trace_units(&self) -> u32 {
        self.surface_trace_units
    }

    #[must_use]
    pub fn surface_region_trace_units(&self) -> &[u32] {
        &self.surface_region_trace_units
    }

    #[must_use]
    pub const fn reservoir(&self) -> Option<&MaterialReservoirCommitment> {
        self.reservoir.as_ref()
    }

    fn is_physically_present(&self) -> bool {
        self.remaining_mass_milligrams != Some(0)
    }

    fn is_accessible_from(&self, embodied_patch: S2CellId) -> bool {
        match &self.reservoir {
            Some(reservoir) => {
                self.held_by.is_none() && reservoir.coverage_patch.contains(embodied_patch)
            }
            None => {
                self.is_physically_present()
                    && self.held_by.is_none()
                    && self.embodied_patch == embodied_patch
            }
        }
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

    fn social_action_value(&self, action_kind: PrimitiveActionKind) -> Option<ActionValueState> {
        self.social_action_values
            .binary_search_by_key(&action_kind, |entry| entry.action_kind)
            .ok()
            .map(|index| self.social_action_values[index])
    }

    fn movement_direction_value(
        &self,
        movement_direction: u8,
    ) -> Option<MovementDirectionValueState> {
        self.movement_direction_values
            .binary_search_by_key(&movement_direction, |entry| entry.movement_direction)
            .ok()
            .map(|index| self.movement_direction_values[index])
    }

    fn signal_action_association(
        &self,
        signal_intensity: u8,
        action_kind: PrimitiveActionKind,
        movement_direction: Option<u8>,
    ) -> Option<SignalActionAssociationState> {
        self.signal_action_associations
            .binary_search_by_key(
                &(signal_intensity, action_kind, movement_direction),
                |entry| {
                    (
                        entry.signal_intensity,
                        entry.action_kind,
                        entry.movement_direction,
                    )
                },
            )
            .ok()
            .map(|index| self.signal_action_associations[index])
    }

    /// Returns a mapping only when it is the unique strongest prediction for the
    /// heard form. A raw co-occurrence therefore cannot bias behavior merely by
    /// having accumulated alongside every incompatible behavior.
    fn distinctive_signal_prediction(
        &self,
        signal_intensity: u8,
        action_kind: PrimitiveActionKind,
        movement_direction: Option<u8>,
    ) -> Option<SignalActionAssociationState> {
        let association =
            self.signal_action_association(signal_intensity, action_kind, movement_direction)?;
        let competing_maximum = self
            .signal_action_associations
            .iter()
            .filter(|entry| {
                entry.signal_intensity == signal_intensity
                    && (entry.action_kind, entry.movement_direction)
                        != (action_kind, movement_direction)
            })
            .map(|entry| entry.value)
            .max()
            .unwrap_or(0);
        (association.value > competing_maximum).then_some(association)
    }

    /// Measures how distinct one form is both from alternative meanings for that
    /// form and from alternative forms for the same situated motor prediction.
    /// This soft two-way competition leaves room for change but prevents every
    /// form from receiving the same saturated production bonus.
    fn signal_convention_strength(
        &self,
        signal_intensity: u8,
        action_kind: PrimitiveActionKind,
        movement_direction: Option<u8>,
    ) -> Option<u32> {
        let association =
            self.distinctive_signal_prediction(signal_intensity, action_kind, movement_direction)?;
        let alternative_form_maximum = self
            .signal_action_associations
            .iter()
            .filter(|entry| {
                entry.signal_intensity != signal_intensity
                    && (entry.action_kind, entry.movement_direction)
                        == (action_kind, movement_direction)
            })
            .map(|entry| entry.value)
            .max()
            .unwrap_or(0);
        let meaning_margin = association.value.saturating_sub(
            self.signal_action_associations
                .iter()
                .filter(|entry| {
                    entry.signal_intensity == signal_intensity
                        && (entry.action_kind, entry.movement_direction)
                            != (action_kind, movement_direction)
                })
                .map(|entry| entry.value)
                .max()
                .unwrap_or(0),
        );
        let form_margin = association.value.saturating_sub(alternative_form_maximum);
        (form_margin > 0).then_some(
            u32::from(meaning_margin.unsigned_abs()) + u32::from(form_margin.unsigned_abs()),
        )
    }

    fn recent_signal_from(&self, actor_id: EntityId, at_tick: SimTick) -> Option<u8> {
        self.perception_memory
            .iter()
            .find(|entry| {
                entry.subject_id == Some(actor_id)
                    && entry.channel == PerceptionChannel::Sound
                    && entry.property_code == "signal_amplitude"
                    && entry.observed_at == at_tick
            })
            .and_then(|entry| u8::try_from(entry.quantized_value).ok())
            .filter(|intensity| (1..=world_domain::SIGNAL_FORM_VARIANT_COUNT).contains(intensity))
    }

    fn recent_signal(&self, at_tick: SimTick) -> Option<u8> {
        self.perception_memory
            .iter()
            .find(|entry| {
                entry.subject_id.is_some()
                    && entry.channel == PerceptionChannel::Sound
                    && entry.property_code == "signal_amplitude"
                    && entry.observed_at == at_tick
            })
            .and_then(|entry| u8::try_from(entry.quantized_value).ok())
            .filter(|intensity| (1..=world_domain::SIGNAL_FORM_VARIANT_COUNT).contains(intensity))
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

fn seeded_cancer_research_cohort<'a>(
    seed: WorldSeed,
    people: impl Iterator<Item = (EntityId, &'a str)>,
) -> Result<Vec<EntityId>, EngineError> {
    let mut strata: BTreeMap<&str, Vec<(Digest, EntityId)>> = BTreeMap::new();
    for (resident_id, birth_category) in people {
        let rank = Digest::canonical(&("cancer-research-initial-cohort-v1", seed, resident_id))?;
        strata
            .entry(birth_category)
            .or_default()
            .push((rank, resident_id));
    }
    let total = strata.values().map(Vec::len).sum::<usize>();
    if total != CANCER_RESEARCH_INITIAL_RESIDENTS as usize
        || strata
            .values()
            .any(|stratum| !stratum.len().is_multiple_of(2))
    {
        return Err(EngineError::InvalidCancerResearchInitialCohort);
    }
    let mut affected = Vec::with_capacity(CANCER_RESEARCH_INITIAL_AFFECTED_RESIDENTS as usize);
    for stratum in strata.values_mut() {
        stratum.sort_unstable();
        affected.extend(
            stratum
                .iter()
                .take(stratum.len() / 2)
                .map(|(_, resident_id)| *resident_id),
        );
    }
    affected.sort_unstable();
    if affected.len() != CANCER_RESEARCH_INITIAL_AFFECTED_RESIDENTS as usize {
        return Err(EngineError::InvalidCancerResearchInitialCohort);
    }
    Ok(affected)
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pending_reproductive_developments: BTreeMap<EntityId, PendingReproductiveDevelopment>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pending_cognition_requests: BTreeMap<Uuid, CognitionRequestSelection>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    initial_cancer_research_cohort: BTreeSet<EntityId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    cancer_burdens: BTreeMap<EntityId, CancerBurdenState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    partition_schedule: Option<PartitionSchedule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    celestial_state: Option<CelestialState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    celestial_tick: Option<SimTick>,
}

type LocalOrganismIndex = BTreeMap<S2CellId, Vec<EntityId>>;

impl EngineState {
    fn uses_world_experiment_bootstrap(&self) -> bool {
        self.manifest.experiment.is_some()
    }

    fn uses_cancer_biology_driver(&self) -> bool {
        self.uses_world_experiment_bootstrap()
            && self.manifest.ruleset_version >= CANCER_BIOLOGY_RULESET_VERSION
    }

    fn uses_adult_body_mass_state_driver(&self) -> bool {
        self.manifest.ruleset_version >= ADULT_BODY_MASS_STATE_RULESET_VERSION
    }

    fn uses_mass_scaled_metabolism_driver(&self) -> bool {
        self.manifest.ruleset_version >= MASS_SCALED_METABOLISM_RULESET_VERSION
    }

    fn uses_local_weather_driver(&self) -> bool {
        self.manifest.ruleset_version >= LOCAL_WEATHER_RULESET_VERSION
    }

    fn uses_local_atmospheric_flux_driver(&self) -> bool {
        self.manifest.ruleset_version >= LOCAL_ATMOSPHERIC_FLUX_RULESET_VERSION
    }

    fn uses_terrain_movement_driver(&self) -> bool {
        self.manifest.ruleset_version >= TERRAIN_MOVEMENT_RULESET_VERSION
    }

    fn uses_topsoil_movement_driver(&self) -> bool {
        self.manifest.ruleset_version >= TOPSOIL_MOVEMENT_RULESET_VERSION
    }

    fn uses_close_kin_exclusion_driver(&self) -> bool {
        self.manifest.ruleset_version >= CLOSE_KIN_EXCLUSION_RULESET_VERSION
    }

    fn uses_local_interaction_driver(&self) -> bool {
        local_interaction_active(self.manifest.ruleset_version, self.tick)
    }

    fn local_interaction_level(&self) -> Result<u8, EngineError> {
        self.configuration
            .as_ref()
            .and_then(WorldConfiguration::full_earth_grid)
            .map(|grid| grid.levels.active_landscape)
            .ok_or_else(|| {
                EngineError::InvalidEmbodiedEvent(
                    "local interaction requires full-Earth landscape geometry".to_owned(),
                )
            })
    }

    fn patches_share_local_vicinity(
        &self,
        left: S2CellId,
        right: S2CellId,
    ) -> Result<bool, EngineError> {
        if left == right {
            return Ok(true);
        }
        let level = self.local_interaction_level()?;
        let left = left.ancestor(level)?;
        let right = right.ancestor(level)?;
        Ok(left == right || s2_edge_neighbors(left)?.contains(&right))
    }

    fn local_organism_index(&self) -> Result<LocalOrganismIndex, EngineError> {
        let level = self.local_interaction_level()?;
        let mut by_landscape_cell = LocalOrganismIndex::new();
        for organism in self
            .organisms
            .values()
            .filter(|organism| organism.is_alive())
        {
            let patch = organism
                .embodied_patch
                .ok_or(EngineError::MissingEmbodiedPatch(organism.organism_id))?;
            by_landscape_cell
                .entry(patch.ancestor(level)?)
                .or_default()
                .push(organism.organism_id);
        }
        Ok(by_landscape_cell)
    }

    fn local_vicinity_organisms<'a>(
        &self,
        index: &'a LocalOrganismIndex,
        patch: S2CellId,
    ) -> Result<Vec<&'a EntityId>, EngineError> {
        let landscape_cell = patch.ancestor(self.local_interaction_level()?)?;
        let mut cells = Vec::with_capacity(5);
        cells.push(landscape_cell);
        cells.extend(s2_edge_neighbors(landscape_cell)?);
        cells.sort_unstable();
        cells.dedup();

        let mut organism_ids = cells
            .into_iter()
            .filter_map(|cell| index.get(&cell))
            .flat_map(|organisms| organisms.iter())
            .collect::<Vec<_>>();
        organism_ids.sort_unstable();
        Ok(organism_ids)
    }

    fn patch_grid_distance(left: S2CellId, right: S2CellId) -> u64 {
        let left = decode_s2_face_ij(left);
        let right = decode_s2_face_ij(right);
        if left.face != right.face || left.level != right.level {
            return u64::MAX;
        }
        u64::from(left.i.abs_diff(right.i)) + u64::from(left.j.abs_diff(right.j))
    }

    fn local_temperature_at_tick(
        &self,
        configuration: &WorldConfiguration,
    ) -> Result<(i64, u8), EngineError> {
        if !self.uses_local_weather_driver() {
            let baseline = configuration.local_environment_baseline().ok_or_else(|| {
                EngineError::PhysiologicalArithmetic(
                    "local temperature requires an environment baseline".to_owned(),
                )
            })?;
            return Ok((
                baseline
                    .mean_at_normal_phase(normal_year_phase(
                        self.tick,
                        configuration.tick_duration_seconds,
                    ))
                    .map_err(|error| EngineError::PhysiologicalArithmetic(error.to_string()))?,
                baseline.air_temperature_decimal_places,
            ));
        }

        const DAY_SECONDS: u64 = 86_400;
        let weather = configuration.local_weather_baseline().ok_or_else(|| {
            EngineError::PhysiologicalArithmetic(
                "local-weather ruleset requires a weather baseline".to_owned(),
            )
        })?;
        let elapsed_seconds = self
            .tick
            .get()
            .checked_mul(u64::from(configuration.tick_duration_seconds))
            .ok_or_else(|| {
                EngineError::PhysiologicalArithmetic(
                    "local-weather elapsed time overflowed".to_owned(),
                )
            })?;
        let day_index = elapsed_seconds / DAY_SECONDS;
        let second_in_day = elapsed_seconds % DAY_SECONDS;
        let anchor = |day: u64| -> Result<i64, EngineError> {
            let phase = usize::try_from((day / 30) % 12).expect("normal phase fits usize");
            let (minimum, _, maximum) = weather
                .temperature_range_at_normal_phase(phase)
                .map_err(|error| EngineError::PhysiologicalArithmetic(error.to_string()))?;
            let span = i128::from(maximum)
                .checked_sub(i128::from(minimum))
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| {
                    EngineError::PhysiologicalArithmetic(
                        "local-weather temperature range overflowed".to_owned(),
                    )
                })?;
            let span = u128::try_from(span).map_err(|_| {
                EngineError::PhysiologicalArithmetic(
                    "local-weather temperature range is negative".to_owned(),
                )
            })?;
            let digest = Digest::canonical(&LocalTemperatureDraw {
                driver_version: 1,
                world_seed: self.manifest.seed.get(),
                source_normals_digest: weather.source_normals_digest,
                day_index: day,
            })?;
            let draw = (u128::from(first_digest_u64(digest)) << 64
                | u128::from(second_digest_u64(digest)))
                % span;
            i64::try_from(
                i128::from(minimum)
                    + i128::try_from(draw).map_err(|_| {
                        EngineError::PhysiologicalArithmetic(
                            "local-weather draw exceeds signed range".to_owned(),
                        )
                    })?,
            )
            .map_err(|_| {
                EngineError::PhysiologicalArithmetic(
                    "local-weather temperature exceeds i64".to_owned(),
                )
            })
        };
        let current = anchor(day_index)?;
        let next = anchor(day_index.checked_add(1).ok_or_else(|| {
            EngineError::PhysiologicalArithmetic("local-weather day overflowed".to_owned())
        })?)?;
        let delta = i128::from(next) - i128::from(current);
        // Interpolate in source fixed point. Integer division truncates toward zero;
        // this exact policy is versioned by LocalTemperatureDraw::driver_version.
        let interpolated =
            i128::from(current) + delta * i128::from(second_in_day) / i128::from(DAY_SECONDS);
        Ok((
            i64::try_from(interpolated).map_err(|_| {
                EngineError::PhysiologicalArithmetic(
                    "interpolated local temperature exceeds i64".to_owned(),
                )
            })?,
            weather.air_temperature_decimal_places,
        ))
    }

    fn local_atmospheric_flux_at_tick(
        &self,
        configuration: &WorldConfiguration,
    ) -> Result<(i64, i64), EngineError> {
        if !self.uses_local_atmospheric_flux_driver() {
            return Err(EngineError::LocalAtmosphericFluxUnsupported);
        }
        const DAY_SECONDS: u64 = 86_400;
        let weather = configuration.local_weather_baseline().ok_or_else(|| {
            EngineError::PhysiologicalArithmetic(
                "local atmospheric flux requires a weather baseline".to_owned(),
            )
        })?;
        let elapsed_seconds = self
            .tick
            .get()
            .checked_mul(u64::from(configuration.tick_duration_seconds))
            .ok_or_else(|| {
                EngineError::PhysiologicalArithmetic(
                    "local atmospheric flux elapsed time overflowed".to_owned(),
                )
            })?;
        let day_index = elapsed_seconds / DAY_SECONDS;
        let phase = usize::try_from((day_index / 30) % 12).expect("normal phase fits usize");
        let (water_flux_mean, eastward_air_mean, northward_air_mean) = weather
            .flux_means_at_normal_phase(phase)
            .map_err(|error| EngineError::PhysiologicalArithmetic(error.to_string()))?;

        let paired_total = i128::from(water_flux_mean).checked_mul(2).ok_or_else(|| {
            EngineError::PhysiologicalArithmetic("paired water-flux total overflowed".to_owned())
        })?;
        let span = u128::try_from(paired_total.checked_add(1).ok_or_else(|| {
            EngineError::PhysiologicalArithmetic("water-flux draw span overflowed".to_owned())
        })?)
        .map_err(|_| {
            EngineError::PhysiologicalArithmetic("water-flux mean is negative".to_owned())
        })?;
        let digest = Digest::canonical(&LocalWaterFluxDraw {
            driver_version: 1,
            world_seed: self.manifest.seed.get(),
            source_normals_digest: weather.source_normals_digest,
            normal_phase: u8::try_from(phase).expect("normal phase fits u8"),
            paired_day_index: day_index / 2,
        })?;
        let draw = (u128::from(first_digest_u64(digest)) << 64
            | u128::from(second_digest_u64(digest)))
            % span;
        let draw = i128::try_from(draw).map_err(|_| {
            EngineError::PhysiologicalArithmetic("water-flux draw exceeds i128".to_owned())
        })?;
        let water_flux = if day_index.is_multiple_of(2) {
            draw
        } else {
            paired_total - draw
        };
        let air_motion = i128::from(eastward_air_mean)
            .abs()
            .checked_add(i128::from(northward_air_mean).abs())
            .ok_or_else(|| {
                EngineError::PhysiologicalArithmetic("air-motion magnitude overflowed".to_owned())
            })?;
        Ok((
            i64::try_from(water_flux).map_err(|_| {
                EngineError::PhysiologicalArithmetic("water flux exceeds i64".to_owned())
            })?,
            i64::try_from(air_motion).map_err(|_| {
                EngineError::PhysiologicalArithmetic("air motion exceeds i64".to_owned())
            })?,
        ))
    }

    #[must_use]
    pub fn new(manifest: WorldManifest) -> Self {
        Self {
            manifest,
            configuration: None,
            status: WorldStatus::Initializing,
            tick: SimTick::ZERO,
            organisms: BTreeMap::new(),
            material_instances: BTreeMap::new(),
            pending_reproductive_developments: BTreeMap::new(),
            pending_cognition_requests: BTreeMap::new(),
            initial_cancer_research_cohort: BTreeSet::new(),
            cancer_burdens: BTreeMap::new(),
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
    pub fn is_initial_cancer_research_resident(&self, resident_id: EntityId) -> bool {
        self.initial_cancer_research_cohort.contains(&resident_id)
    }

    #[must_use]
    pub fn cancer_burden(&self, resident_id: EntityId) -> Option<&CancerBurdenState> {
        self.cancer_burdens.get(&resident_id)
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
        self.plan_genesis_internal(None, initial_organisms, Vec::new())
    }

    pub fn plan_configured_genesis(
        &self,
        configuration: WorldConfiguration,
        initial_organisms: Vec<InitialOrganism>,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        configuration.validate()?;
        self.plan_genesis_internal(Some(configuration), initial_organisms, Vec::new())
    }

    pub fn plan_configured_genesis_with_materials(
        &self,
        configuration: WorldConfiguration,
        initial_organisms: Vec<InitialOrganism>,
        initial_materials: Vec<InitialMaterialInstance>,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        configuration.validate()?;
        self.plan_genesis_internal(Some(configuration), initial_organisms, initial_materials)
    }

    fn plan_genesis_internal(
        &self,
        configuration: Option<WorldConfiguration>,
        mut initial_organisms: Vec<InitialOrganism>,
        mut initial_materials: Vec<InitialMaterialInstance>,
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
        initial_materials.sort_by_key(|instance| instance.object_id);
        if initial_materials
            .windows(2)
            .any(|pair| pair[0].object_id == pair[1].object_id)
        {
            return Err(EngineError::DuplicateInitialMaterial);
        }
        if self.uses_reproductive_physiology_driver()
            && !initial_organisms
                .iter()
                .any(|organism| organism.role == OrganismRole::Person)
        {
            return Err(EngineError::MissingInitialPeople);
        }
        if self.uses_material_reservoir_driver()
            && !initial_materials
                .iter()
                .any(|instance| instance.reservoir.is_some())
        {
            return Err(EngineError::MissingInitialMaterialReservoir);
        }
        if self.uses_heritable_disposition_driver() {
            let mut profiles_by_species = BTreeMap::new();
            for organism in &initial_organisms {
                let profile = organism.heritable_disposition_profile.as_ref().ok_or(
                    EngineError::MissingHeritableDispositionProfile(organism.organism_id),
                )?;
                profile
                    .validate()
                    .map_err(|_| EngineError::InvalidHeritableDisposition)?;
                if profile.species != organism.species {
                    return Err(EngineError::InvalidHeritableDisposition);
                }
                let key = (
                    organism.species.catalog.clone(),
                    organism.species.identifier.clone(),
                    organism.species.scientific_name.clone(),
                    organism.species.source_url.clone(),
                );
                let fingerprint = Digest::canonical(profile)?;
                if profiles_by_species
                    .insert(key, fingerprint)
                    .is_some_and(|previous| previous != fingerprint)
                {
                    return Err(EngineError::InvalidHeritableDisposition);
                }
            }
        }

        let initial_cancer_research_cohort = match &self.manifest.experiment {
            Some(WorldExperimentCommitment::CancerResearch(_)) => {
                Some(seeded_cancer_research_cohort(
                    self.manifest.seed,
                    initial_organisms.iter().filter_map(|organism| {
                        (organism.role == OrganismRole::Person)
                            .then_some((organism.organism_id, organism.birth_category.as_str()))
                    }),
                )?)
            }
            None => None,
        };

        let mut events = Vec::with_capacity(
            initial_organisms
                .len()
                .saturating_mul(if self.uses_adult_body_mass_state_driver() {
                    2
                } else {
                    1
                })
                .saturating_add(initial_materials.len().saturating_mul(2))
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
        if self.uses_local_weather_driver() {
            let weather = configuration
                .as_ref()
                .and_then(WorldConfiguration::local_weather_baseline)
                .ok_or(EngineError::MissingLocalWeather)?;
            weather
                .validate()
                .map_err(|error| EngineError::InvalidLocalWeather(error.to_string()))?;
        }
        if self.uses_terrain_movement_driver() {
            let surface = configuration
                .as_ref()
                .and_then(WorldConfiguration::local_surface_baseline)
                .ok_or_else(|| {
                    EngineError::InvalidLocalSurface(
                        "terrain movement ruleset requires a local surface baseline".to_owned(),
                    )
                })?;
            surface
                .validate()
                .map_err(|error| EngineError::InvalidLocalSurface(error.to_string()))?;
            if self.uses_topsoil_movement_driver() {
                topsoil_adjusted_movement_exposure(1, &surface.topsoil_source_quantiles)
                    .map_err(|error| EngineError::InvalidLocalSurface(error.to_string()))?;
            }
        }
        events.push(DomainEvent::WorldStarted {
            manifest: self.manifest.clone(),
        });
        if let Some(configuration) = &configuration {
            events.push(DomainEvent::WorldConfigured {
                configuration: configuration.clone(),
            });
        }
        for organism in initial_organisms {
            let adult_body_mass = if self.uses_adult_body_mass_state_driver() {
                let commitment = organism.adult_body_mass.as_ref().ok_or_else(|| {
                    EngineError::InvalidEmbodiedEvent(format!(
                        "organism {} lacks an adult-body-mass commitment",
                        organism.organism_id
                    ))
                })?;
                commitment
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                if commitment.species != organism.species {
                    return Err(EngineError::InvalidEmbodiedEvent(
                        "adult-body-mass commitment species does not match organism".to_owned(),
                    ));
                }
                Some(commitment.clone())
            } else {
                if organism.adult_body_mass.is_some() {
                    return Err(EngineError::InvalidEmbodiedEvent(
                        "adult-body-mass state requires ruleset 32".to_owned(),
                    ));
                }
                None
            };
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
            if let Some(reproduction) = &organism.reproductive_physiology {
                reproduction
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                if reproduction.species != organism.species
                    || !reproduction.supports_category(&organism.birth_category)
                    || configuration.as_ref().is_none_or(|world| {
                        world.tick_duration_seconds != reproduction.tick_duration_seconds
                    })
                {
                    return Err(EngineError::InvalidReproductiveCommitment(
                        organism.organism_id,
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
            if self.uses_reproductive_physiology_driver()
                && organism.reproductive_physiology.is_none()
            {
                return Err(EngineError::MissingReproductiveCommitment(
                    organism.organism_id,
                ));
            }
            if !self.uses_reproductive_physiology_driver()
                && organism.reproductive_physiology.is_some()
            {
                return Err(EngineError::ReproductivePhysiologyUnsupported);
            }
            let (heritable_disposition_profile, heritable_disposition) =
                if self.uses_heritable_disposition_driver() {
                    let profile = organism.heritable_disposition_profile.as_ref().ok_or(
                        EngineError::MissingHeritableDispositionProfile(organism.organism_id),
                    )?;
                    profile
                        .validate()
                        .map_err(|_| EngineError::InvalidHeritableDisposition)?;
                    if profile.species != organism.species {
                        return Err(EngineError::InvalidHeritableDisposition);
                    }
                    (
                        Some(profile.clone()),
                        Some(self.founder_heritable_disposition(organism.organism_id, profile)?),
                    )
                } else {
                    if organism.heritable_disposition_profile.is_some() {
                        return Err(EngineError::HeritableDispositionUnsupported);
                    }
                    (None, None)
                };
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
                reproductive_physiology: organism.reproductive_physiology,
                heritable_disposition_profile,
                heritable_disposition,
            });
            if let Some(commitment) = adult_body_mass {
                events.push(DomainEvent::OrganismAdultBodyMassCommitted {
                    organism_id: organism.organism_id,
                    commitment,
                });
            }
        }
        if let Some(affected_resident_ids) = initial_cancer_research_cohort {
            events.push(DomainEvent::CancerResearchCohortCommitted {
                affected_resident_ids,
            });
        }
        for instance in initial_materials {
            events.push(DomainEvent::MaterialInstanceInitialized {
                object_id: instance.object_id,
                material: instance.material,
                embodied_patch: instance.embodied_patch,
                initial_mass_milligrams: instance.initial_mass_milligrams,
                oral_transfer_profiles: instance.oral_transfer_profiles,
            });
            if let Some(commitment) = instance.reservoir {
                events.push(DomainEvent::MaterialReservoirCommitted {
                    object_id: instance.object_id,
                    commitment,
                });
            }
        }
        Ok(events)
    }

    pub fn plan_next_tick(&self) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_status(WorldStatus::Running)?;
        if self.uses_celestial_driver() {
            return Err(EngineError::CelestialStateRequired);
        }
        self.plan_next_tick_internal(None, &[])
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
        self.plan_next_tick_internal(Some(celestial_state), &[])
    }

    /// Plan one tick with the exact fixed-deadline cognition inputs already
    /// durably latched by infrastructure. Replay uses the resulting events only.
    pub fn plan_next_tick_with_celestial_and_cognition(
        &self,
        celestial_state: CelestialState,
        cognition_inputs: &[CognitionDeadlineInput],
    ) -> Result<Vec<DomainEvent>, EngineError> {
        if !self.uses_celestial_driver() {
            return Err(EngineError::CelestialStateUnsupported);
        }
        if !self.uses_cognition_driver() {
            return Err(EngineError::CognitionUnsupported);
        }
        self.plan_next_tick_internal(Some(celestial_state), cognition_inputs)
    }

    fn plan_next_tick_internal(
        &self,
        celestial_state: Option<CelestialState>,
        cognition_inputs: &[CognitionDeadlineInput],
    ) -> Result<Vec<DomainEvent>, EngineError> {
        self.require_status(WorldStatus::Running)?;
        let next = self.tick.checked_next()?;
        let cognition_input = self.cognition_input_for_tick(next, cognition_inputs)?;
        let scheduled_events = self.plan_partition_tick_events_with_cognition(cognition_input)?;
        let mut events = vec![DomainEvent::TickAdvanced {
            from: self.tick,
            to: next,
        }];
        if let Some(input) = cognition_input {
            events.push(DomainEvent::CognitionInputRecorded {
                input: input.clone(),
            });
        }
        events.extend(scheduled_events);
        if let Some(state) = celestial_state {
            events.push(DomainEvent::CelestialStateRecorded { state });
        }
        let mut preview = self.clone();
        preview.apply_events(&events)?;
        if preview.uses_reproductive_physiology_driver() {
            let reproductive_events = preview.plan_reproductive_events()?;
            preview.apply_events(&reproductive_events)?;
            events.extend(reproductive_events);
        }
        if preview.uses_cancer_biology_driver() && preview.is_simulated_day_boundary()? {
            let cancer_events = preview.plan_cancer_burden_events()?;
            preview.apply_events(&cancer_events)?;
            events.extend(cancer_events);
        }
        let unavailable_cognition = preview.plan_unavailable_cognition_events()?;
        preview.apply_events(&unavailable_cognition)?;
        events.extend(unavailable_cognition);
        if preview.living_people() == 0 {
            events.push(DomainEvent::WorldExtinct);
            events.push(DomainEvent::WorldArchived);
        }
        Ok(events)
    }

    fn is_simulated_day_boundary(&self) -> Result<bool, EngineError> {
        let configuration = self
            .configuration
            .as_ref()
            .ok_or(EngineError::InvalidCancerResearchBurden)?;
        Ok(self
            .tick
            .get()
            .checked_mul(u64::from(configuration.tick_duration_seconds))
            .ok_or(EngineError::InvalidCancerResearchBurden)?
            .is_multiple_of(86_400))
    }

    fn plan_cancer_burden_events(&self) -> Result<Vec<DomainEvent>, EngineError> {
        let configuration = self
            .configuration
            .as_ref()
            .ok_or(EngineError::InvalidCancerResearchBurden)?;
        let elapsed_seconds = self
            .tick
            .get()
            .checked_mul(u64::from(configuration.tick_duration_seconds))
            .ok_or(EngineError::InvalidCancerResearchBurden)?;
        let day_ordinal = u32::try_from(elapsed_seconds / 86_400)
            .map_err(|_| EngineError::InvalidCancerResearchBurden)?;
        if day_ordinal == 0 {
            return Ok(Vec::new());
        }
        let transitions = self
            .cancer_burdens
            .iter()
            .filter(|(resident_id, _)| {
                self.organisms
                    .get(resident_id)
                    .is_some_and(OrganismState::is_alive)
            })
            .map(|(resident_id, from)| {
                from.advance_one_day(self.manifest.seed, *resident_id, day_ordinal, self.tick)
                    .map(|to| CancerBurdenTransition {
                        resident_id: *resident_id,
                        from: from.clone(),
                        to,
                    })
                    .map_err(|_| EngineError::InvalidCancerResearchBurden)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(vec![DomainEvent::CancerBurdensAdvanced {
            day_ordinal,
            transitions,
        }])
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
        if preview.uses_reproductive_physiology_driver() {
            let endings = preview.unavailable_reproductive_endings();
            preview.apply_events(&endings)?;
            events.extend(endings);
        }
        let unavailable_cognition = preview.plan_unavailable_cognition_events()?;
        preview.apply_events(&unavailable_cognition)?;
        events.extend(unavailable_cognition);
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

    /// Select one optional external-cognition job from canonical body-owned state.
    /// The caller chooses only the living organism; request identity, deadline,
    /// inputs, and budgets are fixed by the ruleset.
    pub fn plan_cognition_request(
        &self,
        organism_id: EntityId,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        if !self.uses_cognition_driver() {
            return Err(EngineError::CognitionUnsupported);
        }
        if !self.pending_cognition_requests.is_empty() {
            return Err(EngineError::CognitionRequestAlreadyPending);
        }
        Ok(vec![DomainEvent::CognitionRequestSelected {
            selection: self.expected_cognition_selection(organism_id)?,
        }])
    }

    /// Select the next world-total cognition subject from canonical state. This is
    /// the production entry point: infrastructure cannot choose which life receives
    /// the bounded request.
    pub fn plan_scheduled_cognition_request(&self) -> Result<Vec<DomainEvent>, EngineError> {
        if !self.uses_cognition_driver() {
            return Err(EngineError::CognitionUnsupported);
        }
        self.require_status(WorldStatus::Running)?;
        if !self.pending_cognition_requests.is_empty() {
            return Ok(Vec::new());
        }
        let living_organism_ids = self
            .organisms
            .values()
            .filter(|organism| {
                organism.is_alive()
                    && (!self.uses_person_only_cognition() || organism.role == OrganismRole::Person)
            })
            .map(|organism| organism.organism_id)
            .collect::<Vec<_>>();
        if living_organism_ids.is_empty() {
            return Ok(Vec::new());
        }
        let draw = Digest::canonical(&CognitionSubjectDraw {
            driver_version: 1,
            world_seed: self.manifest.seed.get(),
            selected_at_tick: self.tick,
            living_organism_ids: &living_organism_ids,
        })?;
        let length =
            u64::try_from(living_organism_ids.len()).map_err(|_| EngineError::TooManyEvents)?;
        let index = usize::try_from(first_digest_u64(draw) % length)
            .map_err(|_| EngineError::TooManyEvents)?;
        self.plan_cognition_request(living_organism_ids[index])
    }

    fn expected_cognition_selection(
        &self,
        organism_id: EntityId,
    ) -> Result<CognitionRequestSelection, EngineError> {
        self.require_living_organism(organism_id)?;
        let organism = self
            .organisms
            .get(&organism_id)
            .expect("living-organism presence was checked");
        if self.uses_person_only_cognition() && organism.role != OrganismRole::Person {
            return Err(EngineError::InvalidCognitionSelection(
                "this ruleset reserves external cognition for people".to_owned(),
            ));
        }
        let deadline_tick = SimTick::new(
            self.tick
                .get()
                .checked_add(COGNITION_RESPONSE_WINDOW_TICKS)
                .ok_or(TimeOverflow)?,
        );

        // Retain the most recent bounded subset, then restore the domain contract's
        // canonical address ordering. Infrastructure never chooses this subset.
        let mut selected_readings = organism.perception_memory.iter().collect::<Vec<_>>();
        selected_readings.sort_by(|left, right| {
            right
                .observed_at
                .cmp(&left.observed_at)
                .then_with(|| perception_memory_key(left).cmp(&perception_memory_key(right)))
        });
        selected_readings.truncate(MAX_COGNITION_SELECTION_READINGS);
        let mut readings = selected_readings
            .into_iter()
            .map(|reading| CognitionReading {
                subject_id: reading.subject_id,
                channel: reading.channel,
                property_code: reading.property_code.clone(),
                quantized_value: reading.quantized_value,
                uncertainty: reading.uncertainty,
                observed_at: reading.observed_at,
            })
            .collect::<Vec<_>>();
        readings.sort_by(|left, right| {
            (left.subject_id, left.channel, left.property_code.as_str()).cmp(&(
                right.subject_id,
                right.channel,
                right.property_code.as_str(),
            ))
        });

        CognitionRequestSelection::new(
            self.world_id(),
            organism_id,
            self.tick,
            deadline_tick,
            COGNITION_REQUEST_ORDINAL,
            organism.bodily_needs(),
            readings,
            organism.action_values.clone(),
            COGNITION_MEMORY_QUERY_V1,
            COGNITION_MEMORY_MAX_TOKENS,
            COGNITION_MODEL_MAX_OUTPUT_TOKENS,
        )
        .map_err(|error| EngineError::InvalidCognitionSelection(error.to_string()))
    }

    fn cognition_input_for_tick<'a>(
        &self,
        next: SimTick,
        inputs: &'a [CognitionDeadlineInput],
    ) -> Result<Option<&'a CognitionDeadlineInput>, EngineError> {
        if !self.uses_cognition_driver() {
            return if inputs.is_empty() {
                Ok(None)
            } else {
                Err(EngineError::CognitionUnsupported)
            };
        }
        if inputs
            .windows(2)
            .any(|pair| pair[0].request_id >= pair[1].request_id)
        {
            return Err(EngineError::InvalidCognitionInput(
                "deadline inputs are duplicated or not ordered by request ID".to_owned(),
            ));
        }
        let due = self
            .pending_cognition_requests
            .values()
            .filter(|selection| selection.deadline_tick == next)
            .collect::<Vec<_>>();
        if due.len() != inputs.len() {
            return if due.is_empty() {
                Err(EngineError::UnexpectedCognitionInput)
            } else {
                Err(EngineError::CognitionInputRequired)
            };
        }
        for (selection, input) in due.iter().zip(inputs) {
            input
                .validate_against(selection)
                .map_err(|error| EngineError::InvalidCognitionInput(error.to_string()))?;
        }
        Ok(inputs.first())
    }

    fn plan_unavailable_cognition_events(&self) -> Result<Vec<DomainEvent>, EngineError> {
        if !self.uses_cognition_driver() {
            return Ok(Vec::new());
        }
        self.pending_cognition_requests
            .values()
            .filter_map(|selection| {
                let subject_alive = self
                    .organisms
                    .get(&selection.organism_id)
                    .is_some_and(OrganismState::is_alive);
                let reason = if !subject_alive {
                    Some(CognitionUnavailableReason::SubjectUnavailable)
                } else if self.living_people() == 0 {
                    Some(CognitionUnavailableReason::WorldArchived)
                } else {
                    None
                }?;
                Some(
                    CognitionDeadlineInput::unavailable(
                        selection,
                        Digest::ZERO,
                        Digest::ZERO,
                        Digest::ZERO,
                        reason,
                    )
                    .map(|input| DomainEvent::CognitionInputRecorded { input })
                    .map_err(|error| EngineError::InvalidCognitionInput(error.to_string())),
                )
            })
            .collect()
    }

    /// Record a chosen primitive bodily act for a living organism. World physics will
    /// eventually resolve effects; this event itself encodes no cultural outcome.
    pub fn plan_action(
        &self,
        organism_id: EntityId,
        action: PrimitiveAction,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        let local_index = self
            .uses_local_interaction_driver()
            .then(|| self.local_organism_index())
            .transpose()?;
        self.plan_action_with_local_index(organism_id, action, local_index.as_ref())
    }

    fn plan_action_with_local_index(
        &self,
        organism_id: EntityId,
        action: PrimitiveAction,
        local_index: Option<&LocalOrganismIndex>,
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
                PrimitiveActionKind::ApplyForce
                    if self.uses_material_surface_trace_driver()
                        && action.target_id.is_some_and(|object_id| {
                            self.material_instances
                                .get(&object_id)
                                .is_some_and(|instance| instance.held_by == Some(organism_id))
                        }) =>
                {
                    let object_id = action.target_id.expect("guarded material target");
                    if self.uses_material_surface_regions_driver() {
                        let contact_region = action
                            .contact_region
                            .ok_or(EngineError::MissingSurfaceContactRegion)?;
                        if let Some((from_region, from_total, to_region, to_total)) = self
                            .next_material_surface_region_trace(
                                organism_id,
                                object_id,
                                contact_region,
                                action.intensity,
                            )?
                        {
                            events.push(DomainEvent::MaterialSurfaceRegionTraceChanged {
                                object_id,
                                organism_id,
                                contact_region,
                                from_region_trace_units: from_region,
                                from_total_trace_units: from_total,
                                applied_force_units: action.intensity,
                                to_region_trace_units: to_region,
                                to_total_trace_units: to_total,
                            });
                            events.push(DomainEvent::OrganismPerceived {
                                organism_id,
                                perception: surface_region_perception(
                                    object_id,
                                    contact_region,
                                    to_region,
                                    to_total,
                                ),
                            });
                        }
                    } else if let Some((from_trace_units, to_trace_units)) =
                        self.next_material_surface_trace(organism_id, object_id, action.intensity)?
                    {
                        events.push(DomainEvent::MaterialSurfaceTraceChanged {
                            object_id,
                            organism_id,
                            from_trace_units,
                            applied_force_units: action.intensity,
                            to_trace_units,
                        });
                        events.push(DomainEvent::OrganismPerceived {
                            organism_id,
                            perception: SituatedPerception {
                                subject_id: Some(object_id),
                                readings: vec![PropertyReading {
                                    channel: PerceptionChannel::Touch,
                                    property_code: "surface_trace".to_owned(),
                                    quantized_value: i32::try_from(to_trace_units)
                                        .expect("surface trace is bounded to i32"),
                                    uncertainty: 0,
                                }],
                            },
                        });
                    }
                }
                _ => {}
            }
        }
        if self.uses_signal_propagation_driver() && action.kind == PrimitiveActionKind::EmitSignal {
            events.extend(self.local_signal_perceptions_with_index(
                organism_id,
                action.intensity,
                local_index,
            )?);
        }
        if self.uses_material_ingestion_driver()
            && !self.uses_material_reservoir_driver()
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
    fn local_signal_perceptions_with_index(
        &self,
        source_id: EntityId,
        intensity: u16,
        local_index: Option<&LocalOrganismIndex>,
    ) -> Result<Vec<DomainEvent>, EngineError> {
        let source_patch = self
            .organisms
            .get(&source_id)
            .and_then(|organism| organism.embodied_patch)
            .ok_or(EngineError::MissingEmbodiedPatch(source_id))?;
        let candidate_ids = if let Some(index) = local_index {
            self.local_vicinity_organisms(index, source_patch)?
        } else {
            self.organisms.keys().collect()
        };
        let mut recipients = Vec::new();
        for recipient_id in candidate_ids {
            let recipient = self
                .organisms
                .get(recipient_id)
                .expect("spatial index contains only canonical organisms");
            if recipient.organism_id == source_id || !recipient.is_alive() {
                continue;
            }
            let Some(patch) = recipient.embodied_patch else {
                continue;
            };
            let audible = if self.uses_local_interaction_driver() {
                self.patches_share_local_vicinity(source_patch, patch)?
            } else {
                source_patch == patch
            };
            if audible {
                recipients.push((recipient, patch));
            }
        }
        recipients.sort_by_key(|(recipient, patch)| {
            (
                Self::patch_grid_distance(source_patch, *patch),
                recipient.organism_id,
            )
        });
        if self.uses_local_interaction_driver() {
            recipients.truncate(MAX_LOCAL_SIGNAL_RECIPIENTS);
        }
        Ok(recipients
            .into_iter()
            .map(|(recipient, _)| DomainEvent::OrganismPerceived {
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

    fn nearest_recent_signal_source_patch(
        &self,
        organism: &OrganismState,
    ) -> Result<Option<S2CellId>, EngineError> {
        let source_patch = organism
            .embodied_patch
            .ok_or(EngineError::MissingEmbodiedPatch(organism.organism_id))?;
        let mut nearby = Vec::new();
        for memory in organism.perception_memory.iter().filter(|memory| {
            memory.observed_at == self.tick
                && memory.channel == PerceptionChannel::Sound
                && memory.property_code == "signal_amplitude"
        }) {
            let Some(source_id) = memory.subject_id else {
                continue;
            };
            let Some(source) = self
                .organisms
                .get(&source_id)
                .filter(|source| source.is_alive())
            else {
                continue;
            };
            let Some(patch) = source.embodied_patch else {
                continue;
            };
            if self.patches_share_local_vicinity(source_patch, patch)? {
                nearby.push((
                    Self::patch_grid_distance(source_patch, patch),
                    source_id,
                    patch,
                ));
            }
        }
        nearby.sort_unstable();
        Ok(nearby.first().map(|(_, _, patch)| *patch))
    }

    #[cfg(test)]
    fn deterministic_policy_candidates(
        &self,
        organism: &OrganismState,
        age_ticks: u64,
    ) -> Result<Vec<PolicyCandidate>, EngineError> {
        self.deterministic_policy_candidates_with_cognition(organism, age_ticks, None)
    }

    fn deterministic_policy_candidates_with_cognition(
        &self,
        organism: &OrganismState,
        age_ticks: u64,
        cognition_preference: Option<CognitionMotorPreference>,
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
            let may_select_shared_source = self.uses_material_reservoir_driver();
            let patch_leader = self
                .organisms
                .values()
                .filter(|candidate| candidate.is_alive() && candidate.embodied_patch == Some(patch))
                .map(|candidate| candidate.organism_id)
                .min()
                == Some(organism.organism_id);
            let local_objects = self
                .material_instances
                .values()
                .filter(|instance| {
                    instance.is_accessible_from(patch)
                        && (instance.reservoir.is_some() || patch_leader)
                })
                .map(|instance| instance.object_id)
                .collect::<Vec<_>>();
            if may_select_shared_source || patch_leader {
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
                    contact_region: None,
                    movement_direction: None,
                },
                weight: 2,
            },
            PolicyCandidate {
                action: PrimitiveAction {
                    kind: PrimitiveActionKind::Orient,
                    target_id: None,
                    intensity: 1,
                    contact_region: None,
                    movement_direction: None,
                },
                weight: 2,
            },
            PolicyCandidate {
                action: PrimitiveAction {
                    kind: PrimitiveActionKind::Reach,
                    target_id: target,
                    intensity: 1,
                    contact_region: None,
                    movement_direction: None,
                },
                weight: if target.is_some() { oral_drive } else { 1 },
            },
        ];
        if self.uses_selectable_movement_driver() {
            candidates[0].action.movement_direction = Some(0);
            for direction in 1..4_u8 {
                candidates.insert(
                    usize::from(direction),
                    PolicyCandidate {
                        action: PrimitiveAction {
                            kind: PrimitiveActionKind::Move,
                            target_id: None,
                            intensity: 1,
                            contact_region: None,
                            movement_direction: Some(direction),
                        },
                        weight: 2,
                    },
                );
            }
        }
        if let Some(target_id) = target {
            if held_objects.is_empty() {
                let target_is_reservoir = self
                    .material_instances
                    .get(&target_id)
                    .is_some_and(|instance| instance.reservoir.is_some());
                if target_is_reservoir {
                    for kind in [
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
                                contact_region: None,
                                movement_direction: None,
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
                } else {
                    candidates.push(PolicyCandidate {
                        action: PrimitiveAction {
                            kind: PrimitiveActionKind::Grasp,
                            target_id: Some(target_id),
                            intensity: 1,
                            contact_region: None,
                            movement_direction: None,
                        },
                        weight: oral_drive,
                    });
                }
            } else {
                for kind in [
                    PrimitiveActionKind::Release,
                    PrimitiveActionKind::ApplyForce,
                    PrimitiveActionKind::Bite,
                    PrimitiveActionKind::Chew,
                    PrimitiveActionKind::Swallow,
                ] {
                    if kind == PrimitiveActionKind::ApplyForce
                        && self.uses_material_surface_regions_driver()
                    {
                        for contact_region in 0..MATERIAL_SURFACE_REGION_COUNT {
                            candidates.push(PolicyCandidate {
                                action: PrimitiveAction {
                                    kind,
                                    target_id: Some(target_id),
                                    intensity: 1,
                                    contact_region: Some(
                                        u8::try_from(contact_region)
                                            .expect("surface region count fits u8"),
                                    ),
                                    movement_direction: None,
                                },
                                weight: 1,
                            });
                        }
                        continue;
                    }
                    candidates.push(PolicyCandidate {
                        action: PrimitiveAction {
                            kind,
                            target_id: Some(target_id),
                            intensity: 1,
                            contact_region: None,
                            movement_direction: None,
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
        candidates.push(PolicyCandidate {
            action: PrimitiveAction {
                kind: PrimitiveActionKind::Rest,
                target_id: None,
                intensity: 1,
                contact_region: None,
                movement_direction: None,
            },
            weight: rest_drive,
        });
        if self.uses_acoustic_variation_driver() {
            for intensity in 1..=SIGNAL_INTENSITY_VARIANT_COUNT {
                candidates.push(PolicyCandidate {
                    action: PrimitiveAction {
                        kind: PrimitiveActionKind::EmitSignal,
                        target_id: None,
                        intensity,
                        contact_region: None,
                        movement_direction: None,
                    },
                    weight: 2,
                });
            }
        } else {
            candidates.push(PolicyCandidate {
                action: PrimitiveAction {
                    kind: PrimitiveActionKind::EmitSignal,
                    target_id: None,
                    intensity: 1,
                    contact_region: None,
                    movement_direction: None,
                },
                weight: 2,
            });
        }

        if self.uses_heritable_disposition_driver() {
            let profile = organism.heritable_disposition_profile.as_ref().ok_or(
                EngineError::MissingHeritableDispositionProfile(organism.organism_id),
            )?;
            let disposition = organism
                .heritable_disposition
                .as_ref()
                .ok_or(EngineError::InvalidHeritableDisposition)?;
            disposition
                .validate_against(profile)
                .map_err(|_| EngineError::InvalidHeritableDisposition)?;
            for candidate in &mut candidates {
                candidate.weight = inherited_candidate_weight(
                    candidate.weight,
                    disposition,
                    candidate.action.kind,
                )?;
            }
        }

        if self.uses_action_learning_driver() {
            for candidate in &mut candidates {
                candidate.weight = learned_candidate_weight(
                    candidate.weight,
                    organism.action_value(candidate.action.kind),
                );
            }
        }

        if self.uses_movement_direction_learning_driver() {
            for candidate in &mut candidates {
                if let Some(direction) = candidate.action.movement_direction {
                    candidate.weight = learned_candidate_weight(
                        candidate.weight,
                        organism.movement_direction_value(direction).map(|entry| {
                            ActionValueState {
                                value_schema_version: ACTION_VALUE_STATE_SCHEMA_VERSION,
                                action_kind: PrimitiveActionKind::Move,
                                observations: entry.observations,
                                value: entry.value,
                            }
                        }),
                    );
                }
            }
        }

        if self.uses_social_learning_driver() {
            for candidate in &mut candidates {
                candidate.weight = learned_candidate_weight(
                    candidate.weight,
                    organism.social_action_value(candidate.action.kind),
                );
            }
        }

        if self.uses_local_interaction_driver()
            && let Some(target_patch) = self.nearest_recent_signal_source_patch(organism)?
        {
            apply_local_cohesion_weights(patch, target_patch, &mut candidates)?;
        }

        if self.uses_signal_action_association_driver()
            && let Some(signal_intensity) = organism.recent_signal(self.tick)
        {
            for candidate in &mut candidates {
                let association = if self.uses_competitive_signal_learning_driver() {
                    organism.distinctive_signal_prediction(
                        signal_intensity,
                        candidate.action.kind,
                        candidate.action.movement_direction,
                    )
                } else {
                    organism.signal_action_association(
                        signal_intensity,
                        candidate.action.kind,
                        self.uses_signal_motor_association_driver()
                            .then_some(candidate.action.movement_direction)
                            .flatten(),
                    )
                };
                candidate.weight = associated_candidate_weight(candidate.weight, association);
            }
        }

        if self.uses_signal_convention_reuse_driver() {
            let recent_signal = organism.recent_signal(self.tick);
            let context = candidates
                .iter()
                .filter(|candidate| candidate.action.kind != PrimitiveActionKind::EmitSignal)
                .max_by_key(|candidate| candidate.weight)
                .map(|candidate| (candidate.action.clone(), candidate.weight));
            for candidate in &mut candidates {
                if candidate.action.kind != PrimitiveActionKind::EmitSignal {
                    continue;
                }
                let signal_intensity = u8::try_from(candidate.action.intensity)
                    .expect("bounded signal intensity fits u8");
                if let Some((context_action, context_weight)) = &context {
                    candidate.weight = if self.uses_competitive_signal_learning_driver() {
                        competitive_signal_convention_candidate_weight(
                            candidate.weight,
                            signal_intensity,
                            recent_signal,
                            organism.signal_convention_strength(
                                signal_intensity,
                                context_action.kind,
                                context_action.movement_direction,
                            ),
                        )
                    } else {
                        signal_convention_candidate_weight(
                            candidate.weight,
                            signal_intensity,
                            recent_signal,
                            *context_weight,
                            organism.signal_action_association(
                                signal_intensity,
                                context_action.kind,
                                context_action.movement_direction,
                            ),
                        )
                    };
                } else {
                    candidate.weight = signal_convention_candidate_weight(
                        candidate.weight,
                        signal_intensity,
                        recent_signal,
                        0,
                        None,
                    );
                }
            }
        }

        if let Some(preference) = cognition_preference {
            for candidate in &mut candidates {
                if candidate.action.kind == preference.action_kind
                    && preference
                        .contact_region
                        .is_none_or(|region| candidate.action.contact_region == Some(region))
                    && preference
                        .signal_intensity
                        .is_none_or(|intensity| candidate.action.intensity == u16::from(intensity))
                    && preference.movement_direction.is_none_or(|direction| {
                        candidate.action.movement_direction == Some(direction)
                    })
                {
                    candidate.weight = candidate
                        .weight
                        .checked_add(COGNITION_ACTION_WEIGHT_BONUS)
                        .ok_or(EngineError::TooManyEvents)?;
                }
            }
        }

        Ok(candidates)
    }

    #[cfg(test)]
    fn deterministic_policy_action(
        &self,
        organism: &OrganismState,
        age_ticks: u64,
    ) -> Result<PrimitiveAction, EngineError> {
        self.deterministic_policy_action_with_cognition(organism, age_ticks, None)
    }

    fn deterministic_policy_action_with_cognition(
        &self,
        organism: &OrganismState,
        age_ticks: u64,
        cognition_preference: Option<CognitionMotorPreference>,
    ) -> Result<PrimitiveAction, EngineError> {
        let candidates = self.deterministic_policy_candidates_with_cognition(
            organism,
            age_ticks,
            cognition_preference,
        )?;
        let needs = organism.bodily_regulation.needs;

        let digest = Digest::canonical(&PolicyActionDraw {
            policy_version: if self.uses_signal_convention_reuse_driver() {
                9
            } else if self.uses_selectable_movement_driver() {
                8
            } else if self.uses_signal_action_association_driver() {
                7
            } else if self.uses_acoustic_variation_driver() {
                6
            } else if self.uses_social_learning_driver() {
                5
            } else if self.uses_cognition_driver() {
                4
            } else if self.uses_heritable_disposition_driver() {
                3
            } else if self.uses_action_learning_driver() {
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
        if !(self.uses_acoustic_variation_driver()
            && action.kind == PrimitiveActionKind::EmitSignal)
        {
            action.intensity = u16::from(digest.as_bytes()[8] % 4) + 1;
        }
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

    fn next_movement_direction_value(
        &self,
        organism: &OrganismState,
        movement_direction: u8,
        from: BodilyNeedState,
        to: BodilyNeedState,
    ) -> Result<
        (
            Option<MovementDirectionValueState>,
            MovementDirectionValueState,
        ),
        EngineError,
    > {
        let prior = organism.movement_direction_value(movement_direction);
        let observations = prior.map_or(Ok(1), |prior| {
            prior.observations.checked_add(1).ok_or(
                EngineError::MovementDirectionValueObservationOverflow(organism.organism_id),
            )
        })?;
        let prior_value = prior.map_or(0_i32, |prior| i32::from(prior.value));
        let value = prior_value
            .saturating_add(i32::from(action_outcome_reward(from, to)))
            .clamp(i32::from(ACTION_VALUE_MIN), i32::from(ACTION_VALUE_MAX));
        Ok((
            prior,
            MovementDirectionValueState {
                value_schema_version: MOVEMENT_DIRECTION_VALUE_SCHEMA_VERSION,
                movement_direction,
                observations,
                value: i16::try_from(value).expect("bounded direction value fits i16"),
            },
        ))
    }

    fn next_social_action_value(
        &self,
        organism: &OrganismState,
        action_kind: PrimitiveActionKind,
    ) -> Result<(Option<ActionValueState>, ActionValueState), EngineError> {
        let prior = organism.social_action_value(action_kind);
        let observations =
            prior.map_or(Ok(1), |prior| {
                prior.observations.checked_add(1).ok_or(
                    EngineError::ActionValueObservationOverflow(organism.organism_id),
                )
            })?;
        let value = prior
            .map_or(1_i16, |prior| prior.value.saturating_add(1))
            .min(ACTION_VALUE_MAX);
        Ok((
            prior,
            ActionValueState {
                value_schema_version: ACTION_VALUE_STATE_SCHEMA_VERSION,
                action_kind,
                observations,
                value,
            },
        ))
    }

    fn next_signal_action_association(
        &self,
        organism: &OrganismState,
        signal_intensity: u8,
        action: &PrimitiveAction,
        coordinated: bool,
    ) -> Result<
        (
            Option<SignalActionAssociationState>,
            SignalActionAssociationState,
            Option<SignalActionAssociationState>,
            Option<SignalActionAssociationState>,
        ),
        EngineError,
    > {
        let movement_direction = self
            .uses_signal_motor_association_driver()
            .then_some(action.movement_direction)
            .flatten();
        let prior =
            organism.signal_action_association(signal_intensity, action.kind, movement_direction);
        let observations =
            prior.map_or(Ok(1), |prior| {
                prior.observations.checked_add(1).ok_or(
                    EngineError::ActionValueObservationOverflow(organism.organism_id),
                )
            })?;
        let competitive = self.uses_competitive_signal_learning_driver();
        let reinforcement = if coordinated {
            SIGNAL_COORDINATION_REINFORCEMENT
        } else {
            SIGNAL_PREDICTION_REINFORCEMENT
        };
        let value = if competitive {
            prior
                .map_or(reinforcement, |prior| {
                    prior.value.saturating_add(reinforcement)
                })
                .min(ACTION_VALUE_MAX)
        } else {
            prior
                .map_or(1_i16, |prior| prior.value.saturating_add(1))
                .min(ACTION_VALUE_MAX)
        };
        let association_schema_version = if competitive {
            COMPETITIVE_SIGNAL_ASSOCIATION_SCHEMA_VERSION
        } else if self.uses_signal_motor_association_driver() {
            SIGNAL_MOTOR_ASSOCIATION_SCHEMA_VERSION
        } else {
            SIGNAL_ACTION_ASSOCIATION_SCHEMA_VERSION
        };
        let inhibited_from = competitive
            .then(|| {
                organism
                    .signal_action_associations
                    .iter()
                    .copied()
                    .filter(|entry| {
                        let same_form_competing_meaning = entry.signal_intensity
                            == signal_intensity
                            && (entry.action_kind, entry.movement_direction)
                                != (action.kind, movement_direction);
                        let same_meaning_competing_form = entry.signal_intensity
                            != signal_intensity
                            && (entry.action_kind, entry.movement_direction)
                                == (action.kind, movement_direction);
                        same_form_competing_meaning || same_meaning_competing_form
                    })
                    .max_by_key(|entry| {
                        (
                            entry.value,
                            entry.observations,
                            entry.signal_intensity,
                            entry.action_kind,
                            entry.movement_direction,
                        )
                    })
            })
            .flatten();
        let inhibited_to = inhibited_from.map(|from| SignalActionAssociationState {
            observations: from.observations.saturating_add(1),
            value: from
                .value
                .saturating_sub(SIGNAL_PREDICTION_INHIBITION)
                .max(1),
            ..from
        });
        Ok((
            prior,
            SignalActionAssociationState {
                association_schema_version,
                signal_intensity,
                action_kind: action.kind,
                movement_direction,
                observations,
                value,
            },
            inhibited_from,
            inhibited_to,
        ))
    }

    fn social_observations(
        &self,
        actions: &BTreeMap<EntityId, PrimitiveAction>,
        local_index: Option<&LocalOrganismIndex>,
    ) -> Result<BTreeMap<EntityId, EntityId>, EngineError> {
        if self.uses_local_interaction_driver() {
            let owned_index;
            let local_index = if let Some(index) = local_index {
                index
            } else {
                owned_index = self.local_organism_index()?;
                &owned_index
            };
            let mut observations = BTreeMap::new();
            for observer_id in actions.keys() {
                let observer = self
                    .organisms
                    .get(observer_id)
                    .ok_or(EngineError::UnknownOrganism(*observer_id))?;
                let observer_patch = observer
                    .embodied_patch
                    .ok_or(EngineError::MissingEmbodiedPatch(*observer_id))?;
                let nearby = self
                    .local_vicinity_organisms(local_index, observer_patch)?
                    .into_iter()
                    .copied()
                    .filter(|actor_id| actor_id != observer_id && actions.contains_key(actor_id))
                    .collect::<Vec<_>>();
                if nearby.is_empty() {
                    continue;
                }
                let nearby_digest = Digest::canonical(&nearby)?;
                let digest = Digest::canonical(&SocialAttentionDraw {
                    social_attention_version: 2,
                    world_seed: self.manifest.seed.get(),
                    observer_id: *observer_id,
                    tick: self.tick.checked_next()?,
                    co_located_actor_digest: nearby_digest,
                })?;
                let actor_index = usize::try_from(
                    first_digest_u64(digest)
                        % u64::try_from(nearby.len()).expect("nearby length fits u64"),
                )
                .expect("bounded nearby actor index fits usize");
                observations.insert(*observer_id, nearby[actor_index]);
            }
            return Ok(observations);
        }
        let mut by_patch = BTreeMap::<S2CellId, Vec<EntityId>>::new();
        for organism in self.organisms.values().filter(|organism| {
            organism.is_alive()
                && organism.embodied_patch.is_some()
                && actions.contains_key(&organism.organism_id)
        }) {
            by_patch
                .entry(organism.embodied_patch.expect("filtered embodied patch"))
                .or_default()
                .push(organism.organism_id);
        }
        let mut observations = BTreeMap::new();
        for co_located in by_patch.values().filter(|group| group.len() > 1) {
            let co_located_actor_digest = Digest::canonical(co_located)?;
            for (observer_index, observer_id) in co_located.iter().enumerate() {
                let digest = Digest::canonical(&SocialAttentionDraw {
                    social_attention_version: 1,
                    world_seed: self.manifest.seed.get(),
                    observer_id: *observer_id,
                    tick: self.tick.checked_next()?,
                    co_located_actor_digest,
                })?;
                let mut actor_index = usize::try_from(
                    first_digest_u64(digest)
                        % u64::try_from(co_located.len() - 1).expect("length fits"),
                )
                .expect("bounded actor index fits usize");
                if actor_index >= observer_index {
                    actor_index += 1;
                }
                observations.insert(*observer_id, co_located[actor_index]);
            }
        }
        Ok(observations)
    }

    fn reproductive_pair(
        &self,
        left: &OrganismState,
        right: &OrganismState,
    ) -> Option<(ReproductivePhysiologyCommitment, EntityId)> {
        if left.organism_id == right.organism_id
            || left.species != right.species
            || left.role != right.role
            || left.embodied_patch.is_none()
            || left.embodied_patch != right.embodied_patch
            || left.reproductive_physiology != right.reproductive_physiology
            || (self.uses_close_kin_exclusion_driver() && self.are_close_kin(left, right))
        {
            return None;
        }
        let profile = left.reproductive_physiology.as_ref()?;
        let developing_parent_id = Self::reproductive_category_developer(profile, left, right)?;
        Some((profile.clone(), developing_parent_id))
    }

    /// Private genealogy guard. Including the organism itself at depth zero
    /// makes direct ancestry and shared ancestry one uniform intersection test.
    /// A combined path of four covers siblings, avuncular relations, and first
    /// cousins while leaving unrelated founders eligible.
    fn are_close_kin(&self, left: &OrganismState, right: &OrganismState) -> bool {
        const MAX_ANCESTOR_DEPTH: u8 = 3;
        const MAX_COMBINED_PATH: u8 = 4;
        let left_ancestry = self.ancestry_depths(left.organism_id, MAX_ANCESTOR_DEPTH);
        let right_ancestry = self.ancestry_depths(right.organism_id, MAX_ANCESTOR_DEPTH);
        left_ancestry.iter().any(|(ancestor, left_depth)| {
            right_ancestry.get(ancestor).is_some_and(|right_depth| {
                left_depth.saturating_add(*right_depth) <= MAX_COMBINED_PATH
            })
        })
    }

    fn ancestry_depths(&self, organism_id: EntityId, maximum_depth: u8) -> BTreeMap<EntityId, u8> {
        let mut depths = BTreeMap::from([(organism_id, 0)]);
        let mut frontier = vec![(organism_id, 0_u8)];
        while let Some((descendant_id, depth)) = frontier.pop() {
            if depth >= maximum_depth {
                continue;
            }
            let Some(descendant) = self.organisms.get(&descendant_id) else {
                continue;
            };
            let next_depth = depth + 1;
            for parent_id in descendant.parent_ids.iter().rev() {
                if depths
                    .get(parent_id)
                    .is_some_and(|known| *known <= next_depth)
                {
                    continue;
                }
                depths.insert(*parent_id, next_depth);
                frontier.push((*parent_id, next_depth));
            }
        }
        depths
    }

    fn reproductive_category_developer(
        profile: &ReproductivePhysiologyCommitment,
        left: &OrganismState,
        right: &OrganismState,
    ) -> Option<EntityId> {
        let (first, second) = if (&left.birth_category, left.organism_id)
            <= (&right.birth_category, right.organism_id)
        {
            (left, right)
        } else {
            (right, left)
        };
        let pairing = profile.compatible_pairs.iter().find(|pair| {
            pair.first == first.birth_category && pair.second == second.birth_category
        })?;
        let developing_parent_id = if pairing.first == pairing.second
            || pairing.developing_parent == first.birth_category
        {
            first.organism_id
        } else {
            second.organism_id
        };
        Some(developing_parent_id)
    }

    fn reproductively_ready(
        &self,
        organism: &OrganismState,
        profile: &ReproductivePhysiologyCommitment,
    ) -> bool {
        organism.is_alive()
            && organism
                .age_ticks
                .is_some_and(|age| age >= profile.maturity_age_ticks_for(&organism.birth_category))
            && organism
                .reproductive_available_at
                .is_none_or(|available_at| available_at <= self.tick)
    }

    fn reproductive_draw(
        &self,
        stream: &'static str,
        tick: SimTick,
        parent_ids: &[EntityId],
    ) -> Result<Digest, EngineError> {
        Digest::canonical(&ReproductiveDraw {
            driver_version: 1,
            stream,
            world_seed: self.manifest.seed.get(),
            tick,
            parent_ids,
        })
        .map_err(EngineError::from)
    }

    fn heritable_disposition_draw(
        &self,
        stream: &'static str,
        derived_at: SimTick,
        organism_id: EntityId,
        parent_ids: &[EntityId],
        profile: &HeritableDispositionProfile,
        action_kind: PrimitiveActionKind,
    ) -> Result<Digest, EngineError> {
        Digest::canonical(&HeritableDispositionDraw {
            driver_version: 1,
            stream,
            world_seed: self.manifest.seed.get(),
            derived_at,
            organism_id,
            parent_ids,
            profile_fingerprint: Digest::canonical(profile)?,
            action_kind,
        })
        .map_err(EngineError::from)
    }

    fn founder_heritable_disposition(
        &self,
        organism_id: EntityId,
        profile: &HeritableDispositionProfile,
    ) -> Result<HeritableDisposition, EngineError> {
        profile
            .validate()
            .map_err(|_| EngineError::InvalidHeritableDisposition)?;
        let spread = u64::from(profile.founder_variation_steps);
        let width = spread
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(EngineError::HeritableDispositionArithmetic)?;
        let mut action_weights = Vec::with_capacity(HERITABLE_ACTION_KINDS.len());
        for action_kind in HERITABLE_ACTION_KINDS {
            let draw = first_digest_u64(self.heritable_disposition_draw(
                "founder",
                SimTick::ZERO,
                organism_id,
                &[],
                profile,
                action_kind,
            )?);
            let offset = i64::try_from(draw % width).expect("bounded founder variation fits i64")
                - i64::try_from(spread).expect("u16 spread fits i64");
            let weight = i64::from(profile.neutral_action_weight)
                .checked_add(offset)
                .and_then(|value| u16::try_from(value).ok())
                .ok_or(EngineError::HeritableDispositionArithmetic)?;
            action_weights.push(HeritableActionWeight {
                action_kind,
                weight,
            });
        }
        let disposition = HeritableDisposition {
            disposition_schema_version: HERITABLE_DISPOSITION_SCHEMA_VERSION,
            profile_digest: profile.profile_digest,
            generation: 0,
            derived_at: SimTick::ZERO,
            action_weights,
        };
        disposition
            .validate_against(profile)
            .map_err(|_| EngineError::InvalidHeritableDisposition)?;
        Ok(disposition)
    }

    fn offspring_heritable_disposition(
        &self,
        offspring_id: EntityId,
        parent_ids: &[EntityId],
        derived_at: SimTick,
        profile: &HeritableDispositionProfile,
    ) -> Result<HeritableDisposition, EngineError> {
        if parent_ids.len() != 2 || parent_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(EngineError::InvalidHeritableDisposition);
        }
        profile
            .validate()
            .map_err(|_| EngineError::InvalidHeritableDisposition)?;
        let parents = parent_ids
            .iter()
            .map(|parent_id| {
                let parent = self
                    .organisms
                    .get(parent_id)
                    .ok_or(EngineError::UnknownParent(*parent_id))?;
                if parent.heritable_disposition_profile.as_ref() != Some(profile) {
                    return Err(EngineError::InvalidHeritableDisposition);
                }
                let disposition = parent
                    .heritable_disposition
                    .as_ref()
                    .ok_or(EngineError::InvalidHeritableDisposition)?;
                disposition
                    .validate_against(profile)
                    .map_err(|_| EngineError::InvalidHeritableDisposition)?;
                Ok(disposition)
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let generation = parents
            .iter()
            .map(|parent| parent.generation)
            .max()
            .expect("two parents")
            .checked_add(1)
            .ok_or(EngineError::HeritableDispositionArithmetic)?;
        let mut action_weights = Vec::with_capacity(HERITABLE_ACTION_KINDS.len());
        for action_kind in HERITABLE_ACTION_KINDS {
            let inheritance = self.heritable_disposition_draw(
                "inheritance",
                derived_at,
                offspring_id,
                parent_ids,
                profile,
                action_kind,
            )?;
            let inherited_parent = usize::from(inheritance.as_bytes()[0] & 1);
            let mut weight = parents[inherited_parent]
                .action_weight(action_kind)
                .ok_or(EngineError::InvalidHeritableDisposition)?;
            let mutation = self.heritable_disposition_draw(
                "mutation",
                derived_at,
                offspring_id,
                parent_ids,
                profile,
                action_kind,
            )?;
            let mutation_roll = first_digest_u64(mutation) % u64::from(HERITABLE_PROBABILITY_SCALE);
            if mutation_roll < u64::from(profile.mutation_probability_millionths) {
                let magnitude = u16::try_from(
                    second_digest_u64(mutation) % u64::from(profile.mutation_max_step) + 1,
                )
                .expect("bounded mutation magnitude fits u16");
                weight = bounded_mutated_weight(
                    weight,
                    magnitude,
                    mutation.as_bytes()[16] & 1 != 0,
                    profile,
                );
            }
            action_weights.push(HeritableActionWeight {
                action_kind,
                weight,
            });
        }
        let disposition = HeritableDisposition {
            disposition_schema_version: HERITABLE_DISPOSITION_SCHEMA_VERSION,
            profile_digest: profile.profile_digest,
            generation,
            derived_at,
            action_weights,
        };
        disposition
            .validate_against(profile)
            .map_err(|_| EngineError::InvalidHeritableDisposition)?;
        Ok(disposition)
    }

    fn reproductive_opportunity_succeeds_at(
        &self,
        profile: &ReproductivePhysiologyCommitment,
        parent_ids: &[EntityId],
        tick: SimTick,
    ) -> Result<bool, EngineError> {
        let phase = first_digest_u64(self.reproductive_draw(
            "opportunity-phase",
            SimTick::ZERO,
            parent_ids,
        )?) % profile.opportunity_interval_ticks;
        if tick.get() % profile.opportunity_interval_ticks != phase {
            return Ok(false);
        }
        let draw =
            first_digest_u64(self.reproductive_draw("opportunity-success", tick, parent_ids)?)
                % u64::from(REPRODUCTIVE_PROBABILITY_SCALE);
        Ok(draw < u64::from(profile.initiation_probability_millionths))
    }

    fn offspring_category_at(
        &self,
        profile: &ReproductivePhysiologyCommitment,
        parent_ids: &[EntityId],
        tick: SimTick,
    ) -> Result<BirthCategory, EngineError> {
        let total = profile
            .offspring_categories
            .iter()
            .try_fold(0_u64, |total, category| {
                total
                    .checked_add(u64::from(category.weight))
                    .ok_or(EngineError::ReproductiveArithmetic)
            })?;
        let mut draw =
            first_digest_u64(self.reproductive_draw("offspring-category", tick, parent_ids)?)
                % total;
        for category in &profile.offspring_categories {
            let weight = u64::from(category.weight);
            if draw < weight {
                return Ok(category.category.clone());
            }
            draw -= weight;
        }
        unreachable!("validated positive category weights cover the draw")
    }

    fn plan_reproductive_start(
        &self,
        left: &OrganismState,
        right: &OrganismState,
        profile: &ReproductivePhysiologyCommitment,
        developing_parent_id: EntityId,
    ) -> Result<Option<DomainEvent>, EngineError> {
        if !self.reproductively_ready(left, profile) || !self.reproductively_ready(right, profile) {
            return Ok(None);
        }
        let mut parent_ids = vec![left.organism_id, right.organism_id];
        parent_ids.sort_unstable();
        if !self.reproductive_opportunity_succeeds_at(profile, &parent_ids, self.tick)? {
            return Ok(None);
        }
        let birth_category = self.offspring_category_at(profile, &parent_ids, self.tick)?;
        let development_digest =
            self.reproductive_draw("development-identity", self.tick, &parent_ids)?;
        let offspring_digest =
            self.reproductive_draw("offspring-identity", self.tick, &parent_ids)?;
        let development_id =
            EntityId::deterministic(self.world_id(), development_digest.as_bytes());
        let offspring_id = EntityId::deterministic(self.world_id(), offspring_digest.as_bytes());
        if self
            .pending_reproductive_developments
            .contains_key(&development_id)
            || self.organisms.contains_key(&offspring_id)
        {
            return Err(EngineError::ReproductiveIdentityCollision);
        }
        let due = self
            .tick
            .get()
            .checked_add(profile.development_ticks)
            .ok_or(EngineError::ReproductiveArithmetic)?;
        let available = due
            .checked_add(profile.recovery_ticks)
            .ok_or(EngineError::ReproductiveArithmetic)?;
        let (heritable_disposition_profile, offspring_heritable_disposition) =
            if self.uses_heritable_disposition_driver() {
                let heritable_profile = left.heritable_disposition_profile.as_ref().ok_or(
                    EngineError::MissingHeritableDispositionProfile(left.organism_id),
                )?;
                if right.heritable_disposition_profile.as_ref() != Some(heritable_profile) {
                    return Err(EngineError::InvalidHeritableDisposition);
                }
                (
                    Some(heritable_profile.clone()),
                    Some(self.offspring_heritable_disposition(
                        offspring_id,
                        &parent_ids,
                        self.tick,
                        heritable_profile,
                    )?),
                )
            } else {
                (None, None)
            };
        Ok(Some(DomainEvent::ReproductiveDevelopmentStarted {
            development_id,
            offspring_id,
            species: profile.species.clone(),
            role: left.role,
            birth_category,
            parent_ids,
            developing_parent_id,
            profile_digest: profile.profile_digest,
            due_tick: SimTick::new(due),
            parents_available_at: SimTick::new(available),
            heritable_disposition_profile,
            offspring_heritable_disposition,
        }))
    }

    fn unavailable_reproductive_endings(&self) -> Vec<DomainEvent> {
        self.pending_reproductive_developments
            .values()
            .filter(|pending| {
                self.organisms
                    .get(&pending.developing_parent_id)
                    .is_none_or(|parent| !parent.is_alive())
            })
            .map(|pending| DomainEvent::ReproductiveDevelopmentEnded {
                development_id: pending.development_id,
                developing_parent_id: pending.developing_parent_id,
                reason: ReproductiveDevelopmentEnd::DevelopingParentUnavailable,
            })
            .collect()
    }

    fn plan_reproductive_events(&self) -> Result<Vec<DomainEvent>, EngineError> {
        if !self.uses_reproductive_physiology_driver() {
            return Ok(Vec::new());
        }
        let mut events = self.unavailable_reproductive_endings();
        for pending in self.pending_reproductive_developments.values() {
            let developing_parent = self.organisms.get(&pending.developing_parent_id);
            if developing_parent.is_none_or(|parent| !parent.is_alive()) {
                continue;
            }
            if pending.due_tick < self.tick {
                return Err(EngineError::OverdueReproductiveDevelopment(
                    pending.development_id,
                ));
            }
            if pending.due_tick == self.tick {
                let parent = developing_parent.expect("living developing parent checked");
                events.push(DomainEvent::OrganismBorn {
                    organism_id: pending.offspring_id,
                    development_id: Some(pending.development_id),
                    species: pending.species.clone(),
                    role: pending.role,
                    birth_category: pending.birth_category.clone(),
                    parent_ids: pending.parent_ids.clone(),
                    location_id: parent.location_id,
                    embodied_patch: parent.embodied_patch,
                    metabolic_rate: parent.metabolic_rate.clone(),
                    physiological_regulation: parent.physiological_regulation.clone(),
                    reproductive_physiology: parent.reproductive_physiology.clone(),
                    heritable_disposition_profile: pending.heritable_disposition_profile.clone(),
                    heritable_disposition: pending.offspring_heritable_disposition.clone(),
                });
                if self.uses_adult_body_mass_state_driver() {
                    events.push(DomainEvent::OrganismAdultBodyMassCommitted {
                        organism_id: pending.offspring_id,
                        commitment: parent.adult_body_mass.clone().ok_or_else(|| {
                            EngineError::InvalidEmbodiedEvent(format!(
                                "developing parent {} lacks adult-body-mass state",
                                parent.organism_id
                            ))
                        })?,
                    });
                }
            }
        }

        let mut groups =
            BTreeMap::<(S2CellId, &str, &str, u8, Digest, Digest), Vec<&OrganismState>>::new();
        for organism in self
            .organisms
            .values()
            .filter(|organism| organism.is_alive())
        {
            let Some(patch) = organism.embodied_patch else {
                continue;
            };
            let Some(profile) = organism.reproductive_physiology.as_ref() else {
                continue;
            };
            let role = match organism.role {
                OrganismRole::Person => 0,
                OrganismRole::Fauna => 1,
            };
            let profile_fingerprint = Digest::canonical(profile)?;
            let heritable_profile_fingerprint = if self.uses_heritable_disposition_driver() {
                Digest::canonical(organism.heritable_disposition_profile.as_ref().ok_or(
                    EngineError::MissingHeritableDispositionProfile(organism.organism_id),
                )?)?
            } else {
                Digest::ZERO
            };
            groups
                .entry((
                    patch,
                    organism.species.catalog.as_str(),
                    organism.species.identifier.as_str(),
                    role,
                    profile_fingerprint,
                    heritable_profile_fingerprint,
                ))
                .or_default()
                .push(organism);
        }
        let mut committed_parents = BTreeSet::new();
        for organisms in groups.values() {
            let profile = organisms[0]
                .reproductive_physiology
                .as_ref()
                .expect("reproductive group members have profiles");
            if organisms
                .iter()
                .any(|organism| organism.reproductive_physiology.as_ref() != Some(profile))
            {
                return Err(EngineError::InvalidReproductiveCommitment(
                    organisms[0].organism_id,
                ));
            }
            if self.uses_heritable_disposition_driver() {
                let heritable_profile = organisms[0].heritable_disposition_profile.as_ref().ok_or(
                    EngineError::MissingHeritableDispositionProfile(organisms[0].organism_id),
                )?;
                if organisms.iter().any(|organism| {
                    organism.heritable_disposition_profile.as_ref() != Some(heritable_profile)
                }) {
                    return Err(EngineError::InvalidHeritableDisposition);
                }
            }
            let mut categories = BTreeMap::<&BirthCategory, Vec<&OrganismState>>::new();
            for organism in organisms {
                categories
                    .entry(&organism.birth_category)
                    .or_default()
                    .push(organism);
            }
            for pairing in &profile.compatible_pairs {
                let first = categories
                    .get(&pairing.first)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .copied()
                    .filter(|organism| {
                        !committed_parents.contains(&organism.organism_id)
                            && self.reproductively_ready(organism, profile)
                    })
                    .collect::<Vec<_>>();
                if pairing.first == pairing.second {
                    for pair in first.chunks_exact(2) {
                        let left = pair[0];
                        let right = pair[1];
                        let Some((pair_profile, developing_parent_id)) =
                            self.reproductive_pair(left, right)
                        else {
                            return Err(EngineError::InvalidReproductiveCommitment(
                                left.organism_id,
                            ));
                        };
                        if let Some(event) = self.plan_reproductive_start(
                            left,
                            right,
                            &pair_profile,
                            developing_parent_id,
                        )? {
                            events.push(event);
                            committed_parents.insert(left.organism_id);
                            committed_parents.insert(right.organism_id);
                        }
                    }
                    continue;
                }
                let second = categories
                    .get(&pairing.second)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .copied()
                    .filter(|organism| {
                        !committed_parents.contains(&organism.organism_id)
                            && self.reproductively_ready(organism, profile)
                    })
                    .collect::<Vec<_>>();
                for (left, right) in first.into_iter().zip(second) {
                    let Some((pair_profile, developing_parent_id)) =
                        self.reproductive_pair(left, right)
                    else {
                        return Err(EngineError::InvalidReproductiveCommitment(left.organism_id));
                    };
                    if let Some(event) = self.plan_reproductive_start(
                        left,
                        right,
                        &pair_profile,
                        developing_parent_id,
                    )? {
                        events.push(event);
                        committed_parents.insert(left.organism_id);
                        committed_parents.insert(right.organism_id);
                    }
                }
            }
        }
        Ok(events)
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
            let baseline_exposure = tick_seconds
                .checked_mul(u128::from(regulation.fatigue_recovery_seconds))
                .ok_or_else(|| {
                    EngineError::PhysiologicalArithmetic("fatigue exposure overflowed".to_owned())
                })?;
            let mut exposure = if self.uses_terrain_movement_driver()
                && action.kind == PrimitiveActionKind::Move
            {
                let surface = configuration.local_surface_baseline().ok_or_else(|| {
                    EngineError::PhysiologicalArithmetic(
                        "terrain movement requires a local surface baseline".to_owned(),
                    )
                })?;
                terrain_adjusted_movement_exposure(
                    baseline_exposure,
                    surface.terrain_minimum_millimetres,
                    surface.terrain_maximum_millimetres,
                )?
            } else {
                baseline_exposure
            };
            if self.uses_topsoil_movement_driver() && action.kind == PrimitiveActionKind::Move {
                let surface = configuration.local_surface_baseline().ok_or_else(|| {
                    EngineError::PhysiologicalArithmetic(
                        "topsoil movement requires a local surface baseline".to_owned(),
                    )
                })?;
                exposure = topsoil_adjusted_movement_exposure(
                    exposure,
                    &surface.topsoil_source_quantiles,
                )?;
            }
            add_load(
                organism.bodily_regulation.fatigue_load_second_squared,
                exposure,
                fatigue_capacity,
            )?
        };

        let (temperature, decimal_places) = self.local_temperature_at_tick(configuration)?;
        let temperature = decimal_to_millicelsius(temperature, decimal_places)?;
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
            pending_reproductive_developments: self
                .pending_reproductive_developments
                .values()
                .collect(),
            pending_cognition_requests: self.pending_cognition_requests.values().collect(),
            initial_cancer_research_cohort: self
                .initial_cancer_research_cohort
                .iter()
                .copied()
                .collect(),
            cancer_burdens: self.cancer_burdens.values().collect(),
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

    fn plan_partition_tick_events_with_cognition(
        &self,
        cognition_input: Option<&CognitionDeadlineInput>,
    ) -> Result<Vec<DomainEvent>, EngineError> {
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
        let local_index = self
            .uses_local_interaction_driver()
            .then(|| self.local_organism_index())
            .transpose()?;
        let plan = schedule.plan_next_tick(self.tick)?;
        let outputs = plan
            .partitions()
            .iter()
            .map(|partition| {
                let work_outputs = partition
                    .work()
                    .iter()
                    .map(|work| {
                        let organism_id = EntityId::from_uuid(Uuid::from_bytes(
                            work.key().subject().into_bytes(),
                        ));
                        let organism = self.organisms.get(&organism_id).ok_or_else(|| {
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
                                let (temperature, _) = self
                                    .local_temperature_at_tick(configuration)
                                    .map_err(|error| {
                                        EngineError::PartitionScheduleState(error.to_string())
                                    })?;
                                let temperature = i32::try_from(temperature).map_err(|_| {
                                    EngineError::PartitionScheduleState(
                                        "temperature does not fit perception range".to_owned(),
                                    )
                                })?;
                                let readings = if self.uses_local_atmospheric_flux_driver() {
                                    let (water_flux, air_motion) = self
                                        .local_atmospheric_flux_at_tick(configuration)
                                        .map_err(|error| {
                                            EngineError::PartitionScheduleState(error.to_string())
                                        })?;
                                    let water_flux = i32::try_from(water_flux).map_err(|_| {
                                        EngineError::PartitionScheduleState(
                                            "water flux does not fit perception range".to_owned(),
                                        )
                                    })?;
                                    let air_motion = i32::try_from(air_motion).map_err(|_| {
                                        EngineError::PartitionScheduleState(
                                            "air motion does not fit perception range".to_owned(),
                                        )
                                    })?;
                                    vec![
                                        PropertyReading {
                                            channel: PerceptionChannel::Touch,
                                            property_code: "air_motion".to_owned(),
                                            quantized_value: air_motion,
                                            uncertainty: 0,
                                        },
                                        PropertyReading {
                                            channel: PerceptionChannel::Touch,
                                            property_code: "temperature".to_owned(),
                                            quantized_value: temperature,
                                            uncertainty: 0,
                                        },
                                        PropertyReading {
                                            channel: PerceptionChannel::Touch,
                                            property_code: "water_flux".to_owned(),
                                            quantized_value: water_flux,
                                            uncertainty: 0,
                                        },
                                    ]
                                } else {
                                    vec![PropertyReading {
                                        channel: PerceptionChannel::Touch,
                                        property_code: "temperature".to_owned(),
                                        quantized_value: temperature,
                                        uncertainty: 0,
                                    }]
                                };
                                events.push(Emission::new(
                                    partition.partition(),
                                    work.key(),
                                    2,
                                    DomainEvent::OrganismPerceived {
                                        organism_id: organism.organism_id,
                                        perception: SituatedPerception {
                                            subject_id: None,
                                            readings,
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
                                let cognition_preference = cognition_input
                                    .filter(|input| input.organism_id == organism.organism_id)
                                    .and_then(|input| {
                                        input.action_kind().map(|kind| CognitionMotorPreference {
                                            action_kind: kind,
                                            contact_region: input.contact_region(),
                                            signal_intensity: input.signal_intensity(),
                                            movement_direction: input.movement_direction(),
                                        })
                                    });
                                let action = self.deterministic_policy_action_with_cognition(
                                    organism,
                                    to_age_ticks,
                                    cognition_preference,
                                )?;
                                let action_kind = action.kind;
                                let movement_direction = action.movement_direction;
                                let resolved_action = self.plan_action_with_local_index(
                                    organism.organism_id,
                                    action,
                                    local_index.as_ref(),
                                )?;
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
                                    let direction = if self.uses_selectable_movement_driver() {
                                        usize::from(
                                            movement_direction
                                                .ok_or(EngineError::MissingMovementDirection)?,
                                        )
                                    } else {
                                        usize::try_from(
                                            first_digest_u64(Digest::canonical(
                                                &PolicyMovementDraw {
                                                    policy_version: 1,
                                                    world_seed: self.manifest.seed.get(),
                                                    organism_id: organism.organism_id,
                                                    tick: self.tick.checked_next()?,
                                                    age_ticks: to_age_ticks,
                                                    from_patch,
                                                },
                                            )?) % 4,
                                        )
                                        .expect("direction is in 0..4")
                                    };
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
                                            contact_region: None,
                                            movement_direction: None,
                                        },
                                    },
                                ));
                                if action_kind == PrimitiveActionKind::EmitSignal
                                    && self.uses_signal_propagation_driver()
                                {
                                    for (offset, perception) in self
                                        .local_signal_perceptions_with_index(
                                            organism.organism_id,
                                            1,
                                            local_index.as_ref(),
                                        )?
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
        if self.uses_material_reservoir_driver() {
            let oral_transfers = self.plan_ordered_oral_transfers(&events)?;
            events.extend(oral_transfers);
        }
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
                }
                | DomainEvent::MaterialReservoirOralPortionTransferred {
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
                if self.uses_movement_direction_learning_driver()
                    && action.kind == PrimitiveActionKind::Move
                {
                    let direction = action
                        .movement_direction
                        .ok_or(EngineError::MissingMovementDirection)?;
                    let (from, to_value) = self.next_movement_direction_value(
                        organism,
                        direction,
                        organism.bodily_regulation.needs,
                        to.needs,
                    )?;
                    events.push(DomainEvent::OrganismMovementDirectionValueChanged {
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
            if self.uses_social_learning_driver() {
                for (observer_id, actor_id) in
                    self.social_observations(&actions, local_index.as_ref())?
                {
                    let observer = self
                        .organisms
                        .get(&observer_id)
                        .expect("social observer is a living organism");
                    let action = actions
                        .get(&actor_id)
                        .expect("selected social actor has a scheduled action");
                    let (from, to) = self.next_social_action_value(observer, action.kind)?;
                    events.push(DomainEvent::OrganismSocialActionValueChanged {
                        observer_id,
                        actor_id,
                        from,
                        to,
                    });
                    if self.uses_signal_action_association_driver()
                        && let Some(signal_intensity) =
                            observer.recent_signal_from(actor_id, self.tick)
                    {
                        let observer_action = actions
                            .get(&observer_id)
                            .expect("social observer has a scheduled action");
                        let coordinated = observer_action.kind == action.kind
                            && observer_action.movement_direction == action.movement_direction;
                        let (from, to, inhibited_from, inhibited_to) = self
                            .next_signal_action_association(
                                observer,
                                signal_intensity,
                                action,
                                coordinated,
                            )?;
                        events.push(DomainEvent::OrganismSignalActionAssociationChanged {
                            observer_id,
                            actor_id,
                            from,
                            to,
                            inhibited_from,
                            inhibited_to,
                        });
                    }
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

    fn uses_reproductive_physiology_driver(&self) -> bool {
        self.manifest.ruleset_version >= REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION
    }

    fn uses_heritable_disposition_driver(&self) -> bool {
        self.manifest.ruleset_version >= HERITABLE_DISPOSITION_RULESET_VERSION
    }

    fn uses_cognition_driver(&self) -> bool {
        self.manifest.ruleset_version >= COGNITION_RULESET_VERSION
    }

    fn uses_material_reservoir_driver(&self) -> bool {
        self.manifest.ruleset_version >= MATERIAL_RESERVOIR_RULESET_VERSION
    }

    fn uses_social_learning_driver(&self) -> bool {
        self.manifest.ruleset_version >= SOCIAL_LEARNING_RULESET_VERSION
    }

    fn uses_material_surface_trace_driver(&self) -> bool {
        self.manifest.ruleset_version >= MATERIAL_SURFACE_TRACE_RULESET_VERSION
    }

    fn uses_material_surface_regions_driver(&self) -> bool {
        self.manifest.ruleset_version >= MATERIAL_SURFACE_REGIONS_RULESET_VERSION
    }

    fn uses_acoustic_variation_driver(&self) -> bool {
        self.manifest.ruleset_version >= ACOUSTIC_VARIATION_RULESET_VERSION
    }

    fn uses_signal_action_association_driver(&self) -> bool {
        self.manifest.ruleset_version >= SIGNAL_ACTION_ASSOCIATION_RULESET_VERSION
    }

    fn uses_selectable_movement_driver(&self) -> bool {
        self.manifest.ruleset_version >= SELECTABLE_MOVEMENT_RULESET_VERSION
    }

    fn uses_movement_direction_learning_driver(&self) -> bool {
        self.manifest.ruleset_version >= MOVEMENT_DIRECTION_LEARNING_RULESET_VERSION
    }

    fn uses_signal_motor_association_driver(&self) -> bool {
        self.manifest.ruleset_version >= SIGNAL_MOTOR_ASSOCIATION_RULESET_VERSION
    }

    fn uses_signal_convention_reuse_driver(&self) -> bool {
        signal_convention_reuse_active(self.manifest.ruleset_version, self.tick)
    }

    fn uses_competitive_signal_learning_driver(&self) -> bool {
        self.manifest.ruleset_version >= GROUNDED_PREDICTIVE_COGNITION_RULESET_VERSION
    }

    fn uses_person_only_cognition(&self) -> bool {
        self.manifest.ruleset_version >= PERSON_COGNITION_RULESET_VERSION
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
            DomainEvent::MaterialReservoirCommitted { object_id, .. } => self
                .material_instances
                .get(object_id)
                .or_else(|| resulting_state.material_instances.get(object_id))
                .map(|instance| instance.embodied_patch),
            DomainEvent::MaterialInstanceHeld { holder_id, .. } => self
                .organisms
                .get(holder_id)
                .or_else(|| resulting_state.organisms.get(holder_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::MaterialInstanceReleased { embodied_patch, .. } => Some(*embodied_patch),
            DomainEvent::MaterialSurfaceTraceChanged { organism_id, .. }
            | DomainEvent::MaterialSurfaceRegionTraceChanged { organism_id, .. } => self
                .organisms
                .get(organism_id)
                .or_else(|| resulting_state.organisms.get(organism_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::MaterialOralPortionTransferred { organism_id, .. } => self
                .organisms
                .get(organism_id)
                .or_else(|| resulting_state.organisms.get(organism_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::MaterialReservoirOralPortionTransferred { organism_id, .. } => self
                .organisms
                .get(organism_id)
                .or_else(|| resulting_state.organisms.get(organism_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::OrganismMoved { to_patch, .. } => Some(*to_patch),
            DomainEvent::OrganismDied { organism_id, .. }
            | DomainEvent::OrganismAdultBodyMassCommitted { organism_id, .. }
            | DomainEvent::OrganismAgeAdvanced { organism_id, .. }
            | DomainEvent::OrganismNeedsChanged { organism_id, .. }
            | DomainEvent::OrganismActionValueChanged { organism_id, .. }
            | DomainEvent::OrganismMovementDirectionValueChanged { organism_id, .. }
            | DomainEvent::OrganismPerceived { organism_id, .. }
            | DomainEvent::OrganismActed { organism_id, .. } => self
                .organisms
                .get(organism_id)
                .or_else(|| resulting_state.organisms.get(organism_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::OrganismSocialActionValueChanged { observer_id, .. } => self
                .organisms
                .get(observer_id)
                .or_else(|| resulting_state.organisms.get(observer_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::OrganismSignalActionAssociationChanged { observer_id, .. } => self
                .organisms
                .get(observer_id)
                .or_else(|| resulting_state.organisms.get(observer_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::CognitionRequestSelected { selection } => self
                .organisms
                .get(&selection.organism_id)
                .or_else(|| resulting_state.organisms.get(&selection.organism_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::CognitionInputRecorded { input } => self
                .organisms
                .get(&input.organism_id)
                .or_else(|| resulting_state.organisms.get(&input.organism_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::ReproductiveDevelopmentStarted {
                developing_parent_id,
                ..
            }
            | DomainEvent::ReproductiveDevelopmentEnded {
                developing_parent_id,
                ..
            } => resulting_state
                .organisms
                .get(developing_parent_id)
                .or_else(|| self.organisms.get(developing_parent_id))
                .and_then(|organism| organism.embodied_patch),
            DomainEvent::WorldStarted { .. }
            | DomainEvent::WorldConfigured { .. }
            | DomainEvent::CancerResearchCohortCommitted { .. }
            | DomainEvent::CancerBurdensAdvanced { .. }
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
        let starts_world = events
            .iter()
            .any(|event| matches!(event, DomainEvent::WorldStarted { .. }));
        let has_material_reservoir_commitment = events
            .iter()
            .any(|event| matches!(event, DomainEvent::MaterialReservoirCommitted { .. }));
        if has_material_reservoir_commitment
            && (!self.uses_material_reservoir_driver() || !starts_world)
        {
            return Err(EngineError::MaterialReservoirUnsupported);
        }
        if self.uses_material_reservoir_driver()
            && starts_world
            && !has_material_reservoir_commitment
        {
            return Err(EngineError::MissingInitialMaterialReservoir);
        }
        if self.uses_reproductive_physiology_driver()
            && !starts_world
            && events
                .iter()
                .any(|event| matches!(event, DomainEvent::OrganismInitialized { .. }))
        {
            return Err(EngineError::OrganismInitializationOutsideGenesis);
        }
        if self.uses_reproductive_physiology_driver()
            && starts_world
            && !events.iter().any(|event| {
                matches!(
                    event,
                    DomainEvent::OrganismInitialized {
                        role: OrganismRole::Person,
                        ..
                    }
                )
            })
        {
            return Err(EngineError::MissingInitialPeople);
        }
        let tick_advance_count = events
            .iter()
            .filter(|event| matches!(event, DomainEvent::TickAdvanced { .. }))
            .count();
        let tick_advanced = tick_advance_count != 0;
        let tick_advance_index = events
            .iter()
            .position(|event| matches!(event, DomainEvent::TickAdvanced { .. }));
        if self.uses_reproductive_physiology_driver()
            && (tick_advance_count > 1 || tick_advance_index.is_some_and(|index| index != 0))
        {
            return Err(EngineError::InvalidTickAdvanceEventSet);
        }
        if self.uses_action_learning_driver()
            && !tick_advanced
            && let Some(organism_id) = events.iter().find_map(|event| match event {
                DomainEvent::OrganismActed { organism_id, .. } => Some(*organism_id),
                _ => None,
            })
        {
            return Err(EngineError::InvalidActionValueTransition(organism_id));
        }
        if self.uses_material_reservoir_driver() {
            let actual_transfers = events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        DomainEvent::MaterialOralPortionTransferred { .. }
                            | DomainEvent::MaterialReservoirOralPortionTransferred { .. }
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            let expected_transfers = if tick_advanced {
                self.plan_ordered_oral_transfers(events)?
            } else {
                Vec::new()
            };
            if actual_transfers != expected_transfers {
                return Err(EngineError::InvalidMaterialReservoirEventSet);
            }
        } else if events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::MaterialReservoirCommitted { .. }
                    | DomainEvent::MaterialReservoirOralPortionTransferred { .. }
            )
        }) {
            return Err(EngineError::MaterialReservoirUnsupported);
        }
        if self.uses_material_surface_regions_driver() {
            let mut expected_traces = Vec::new();
            let mut expected_perceptions = Vec::new();
            for event in events {
                let DomainEvent::OrganismActed {
                    organism_id,
                    action,
                } = event
                else {
                    continue;
                };
                if action.kind != PrimitiveActionKind::ApplyForce {
                    continue;
                }
                let Some(object_id) = action.target_id else {
                    continue;
                };
                if !self
                    .material_instances
                    .get(&object_id)
                    .is_some_and(|instance| instance.held_by == Some(*organism_id))
                {
                    continue;
                }
                let contact_region = action
                    .contact_region
                    .ok_or(EngineError::MissingSurfaceContactRegion)?;
                if let Some((from_region, from_total, to_region, to_total)) = self
                    .next_material_surface_region_trace(
                        *organism_id,
                        object_id,
                        contact_region,
                        action.intensity,
                    )?
                {
                    expected_traces.push(DomainEvent::MaterialSurfaceRegionTraceChanged {
                        object_id,
                        organism_id: *organism_id,
                        contact_region,
                        from_region_trace_units: from_region,
                        from_total_trace_units: from_total,
                        applied_force_units: action.intensity,
                        to_region_trace_units: to_region,
                        to_total_trace_units: to_total,
                    });
                    expected_perceptions.push(DomainEvent::OrganismPerceived {
                        organism_id: *organism_id,
                        perception: surface_region_perception(
                            object_id,
                            contact_region,
                            to_region,
                            to_total,
                        ),
                    });
                }
            }
            let actual_traces = events
                .iter()
                .filter(|event| {
                    matches!(event, DomainEvent::MaterialSurfaceRegionTraceChanged { .. })
                })
                .cloned()
                .collect::<Vec<_>>();
            let actual_perceptions = events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        DomainEvent::OrganismPerceived { perception, .. }
                            if perception.readings.iter().any(|reading| reading.property_code.starts_with("surface_region_"))
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            if actual_traces != expected_traces
                || actual_perceptions != expected_perceptions
                || events
                    .iter()
                    .any(|event| matches!(event, DomainEvent::MaterialSurfaceTraceChanged { .. }))
            {
                return Err(EngineError::InvalidMaterialSurfaceRegionEventSet);
            }
        } else if self.uses_material_surface_trace_driver() {
            let mut expected_traces = Vec::new();
            let mut expected_perceptions = Vec::new();
            for event in events {
                let DomainEvent::OrganismActed {
                    organism_id,
                    action,
                } = event
                else {
                    continue;
                };
                if action.kind != PrimitiveActionKind::ApplyForce {
                    continue;
                }
                let Some(object_id) = action.target_id else {
                    continue;
                };
                if !self
                    .material_instances
                    .get(&object_id)
                    .is_some_and(|instance| instance.held_by == Some(*organism_id))
                {
                    continue;
                }
                if let Some((from_trace_units, to_trace_units)) =
                    self.next_material_surface_trace(*organism_id, object_id, action.intensity)?
                {
                    expected_traces.push(DomainEvent::MaterialSurfaceTraceChanged {
                        object_id,
                        organism_id: *organism_id,
                        from_trace_units,
                        applied_force_units: action.intensity,
                        to_trace_units,
                    });
                    expected_perceptions.push(DomainEvent::OrganismPerceived {
                        organism_id: *organism_id,
                        perception: SituatedPerception {
                            subject_id: Some(object_id),
                            readings: vec![PropertyReading {
                                channel: PerceptionChannel::Touch,
                                property_code: "surface_trace".to_owned(),
                                quantized_value: i32::try_from(to_trace_units)
                                    .expect("surface trace is bounded to i32"),
                                uncertainty: 0,
                            }],
                        },
                    });
                }
            }
            let actual_traces = events
                .iter()
                .filter(|event| matches!(event, DomainEvent::MaterialSurfaceTraceChanged { .. }))
                .cloned()
                .collect::<Vec<_>>();
            let actual_perceptions = events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        DomainEvent::OrganismPerceived { perception, .. }
                            if perception.readings.iter().any(|reading| reading.property_code == "surface_trace")
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            if actual_traces != expected_traces || actual_perceptions != expected_perceptions {
                return Err(EngineError::InvalidMaterialSurfaceTraceEventSet);
            }
        } else if events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::MaterialSurfaceTraceChanged { .. }
                    | DomainEvent::MaterialSurfaceRegionTraceChanged { .. }
            ) || matches!(
                event,
                DomainEvent::OrganismPerceived { perception, .. }
                    if perception.readings.iter().any(|reading|
                        reading.property_code == "surface_trace"
                            || reading.property_code.starts_with("surface_region_"))
            )
        }) {
            return Err(EngineError::MaterialSurfaceTraceUnsupported);
        }
        for (index, event) in events.iter().enumerate() {
            let (DomainEvent::MaterialOralPortionTransferred {
                object_id,
                organism_id,
                ..
            }
            | DomainEvent::MaterialReservoirOralPortionTransferred {
                object_id,
                organism_id,
                ..
            }) = event
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
        for (index, event) in events.iter().enumerate() {
            if let DomainEvent::OrganismMovementDirectionValueChanged {
                organism_id,
                from,
                to,
            } = event
            {
                if !self.uses_movement_direction_learning_driver() || !tick_advanced {
                    return Err(EngineError::InvalidMovementDirectionValueTransition(
                        *organism_id,
                    ));
                }
                let matching_actions = events
                    .iter()
                    .enumerate()
                    .filter_map(|(prior_index, prior)| match prior {
                        DomainEvent::OrganismActed {
                            organism_id: actor_id,
                            action,
                        } if actor_id == organism_id
                            && action.kind == PrimitiveActionKind::Move
                            && action.movement_direction == Some(to.movement_direction) =>
                        {
                            Some(prior_index)
                        }
                        _ => None,
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
                    return Err(EngineError::InvalidMovementDirectionValueTransition(
                        *organism_id,
                    ));
                }
                let action_index = matching_actions[0];
                let needs_index = matching_needs[0];
                let Some(tick_index) = tick_advance_index else {
                    return Err(EngineError::InvalidMovementDirectionValueTransition(
                        *organism_id,
                    ));
                };
                if !(tick_index < action_index && action_index < needs_index && needs_index < index)
                {
                    return Err(EngineError::InvalidMovementDirectionValueTransition(
                        *organism_id,
                    ));
                }
                let DomainEvent::OrganismNeedsChanged {
                    from: body_from,
                    to: body_to,
                    ..
                } = &events[needs_index]
                else {
                    unreachable!("need index was selected from bodily transitions")
                };
                let organism = self
                    .organisms
                    .get(organism_id)
                    .ok_or(EngineError::UnknownOrganism(*organism_id))?;
                let expected = self.next_movement_direction_value(
                    organism,
                    to.movement_direction,
                    body_from.needs,
                    body_to.needs,
                )?;
                if expected.0 != *from || expected.1 != *to {
                    return Err(EngineError::InvalidMovementDirectionValueTransition(
                        *organism_id,
                    ));
                }
            }
            if self.uses_movement_direction_learning_driver()
                && tick_advanced
                && let DomainEvent::OrganismActed {
                    organism_id,
                    action,
                } = event
                && action.kind == PrimitiveActionKind::Move
            {
                let matching_updates = events[index + 1..]
                    .iter()
                    .filter(|later| {
                        matches!(
                            later,
                            DomainEvent::OrganismMovementDirectionValueChanged {
                                organism_id: actor_id,
                                to,
                                ..
                            } if actor_id == organism_id
                                && Some(to.movement_direction) == action.movement_direction
                        )
                    })
                    .count();
                if matching_updates != 1 {
                    return Err(EngineError::InvalidMovementDirectionValueTransition(
                        *organism_id,
                    ));
                }
            }
        }
        if self.uses_selectable_movement_driver() && tick_advanced {
            let moves = events
                .iter()
                .enumerate()
                .filter_map(|(index, event)| match event {
                    DomainEvent::OrganismActed {
                        organism_id,
                        action,
                    } if action.kind == PrimitiveActionKind::Move => {
                        Some((index, *organism_id, action.movement_direction))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let relocations = events
                .iter()
                .enumerate()
                .filter_map(|(index, event)| match event {
                    DomainEvent::OrganismMoved {
                        organism_id,
                        from_patch,
                        to_patch,
                    } => Some((index, *organism_id, *from_patch, *to_patch)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if moves.len() != relocations.len() {
                return Err(EngineError::InvalidSelectableMovementEventSet);
            }
            for (action_index, organism_id, direction) in moves {
                let from_patch = self
                    .organisms
                    .get(&organism_id)
                    .and_then(|organism| organism.embodied_patch)
                    .ok_or(EngineError::MissingEmbodiedPatch(organism_id))?;
                let direction =
                    usize::from(direction.ok_or(EngineError::MissingMovementDirection)?);
                let expected_to =
                    s2_edge_neighbors(from_patch).map_err(EngineError::from)?[direction];
                let matching = relocations.iter().filter(
                    |(relocation_index, moved_id, actual_from, actual_to)| {
                        action_index < *relocation_index
                            && organism_id == *moved_id
                            && from_patch == *actual_from
                            && expected_to == *actual_to
                    },
                );
                if matching.count() != 1 {
                    return Err(EngineError::InvalidSelectableMovementEventSet);
                }
            }
        }
        if self.uses_social_learning_driver() && tick_advanced {
            let actions = events
                .iter()
                .filter_map(|event| match event {
                    DomainEvent::OrganismActed {
                        organism_id,
                        action,
                    } => Some((*organism_id, action.clone())),
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>();
            let expected_observations = self.social_observations(&actions, None)?;
            for observer in self
                .organisms
                .values()
                .filter(|organism| organism.is_alive())
            {
                let expected_actor = expected_observations.get(&observer.organism_id).copied();
                let matches = events
                    .iter()
                    .enumerate()
                    .filter_map(|(index, event)| match event {
                        DomainEvent::OrganismSocialActionValueChanged {
                            observer_id,
                            actor_id,
                            from,
                            to,
                        } if *observer_id == observer.organism_id => {
                            Some((index, *actor_id, from, to))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                match (expected_actor, matches.as_slice()) {
                    (None, []) => {}
                    (Some(actor_id), [(social_index, actual_actor, from, to)])
                        if actor_id == *actual_actor =>
                    {
                        let action_index = events
                            .iter()
                            .position(|event| {
                                matches!(event, DomainEvent::OrganismActed { organism_id, .. } if *organism_id == actor_id)
                            })
                            .ok_or(EngineError::InvalidSocialActionValueTransition(
                                observer.organism_id,
                            ))?;
                        let action = actions.get(&actor_id).expect("selected action exists");
                        let expected = self.next_social_action_value(observer, action.kind)?;
                        if action_index >= *social_index
                            || expected.0 != **from
                            || expected.1 != **to
                        {
                            return Err(EngineError::InvalidSocialActionValueTransition(
                                observer.organism_id,
                            ));
                        }
                    }
                    _ => {
                        return Err(EngineError::InvalidSocialActionValueTransition(
                            observer.organism_id,
                        ));
                    }
                }
            }
        } else if events
            .iter()
            .any(|event| matches!(event, DomainEvent::OrganismSocialActionValueChanged { .. }))
        {
            return Err(EngineError::SocialLearningUnsupported);
        }
        if self.uses_signal_action_association_driver() && tick_advanced {
            let actions = events
                .iter()
                .filter_map(|event| match event {
                    DomainEvent::OrganismActed {
                        organism_id,
                        action,
                    } => Some((*organism_id, action.clone())),
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>();
            let expected_observations = self.social_observations(&actions, None)?;
            for observer in self
                .organisms
                .values()
                .filter(|organism| organism.is_alive())
            {
                let expected =
                    expected_observations
                        .get(&observer.organism_id)
                        .and_then(|actor_id| {
                            observer
                                .recent_signal_from(*actor_id, self.tick)
                                .map(|intensity| (*actor_id, intensity))
                        });
                let matches = events
                    .iter()
                    .filter_map(|event| match event {
                        DomainEvent::OrganismSignalActionAssociationChanged {
                            observer_id,
                            actor_id,
                            from,
                            to,
                            inhibited_from,
                            inhibited_to,
                        } if *observer_id == observer.organism_id => {
                            Some((*actor_id, *from, *to, *inhibited_from, *inhibited_to))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                match (expected, matches.as_slice()) {
                    (None, []) => {}
                    (
                        Some((actor_id, intensity)),
                        [(actual_actor, from, to, inhibited_from, inhibited_to)],
                    ) if actor_id == *actual_actor => {
                        let action = actions.get(&actor_id).expect("selected action exists");
                        let observer_action = actions
                            .get(&observer.organism_id)
                            .expect("social observer has a scheduled action");
                        let coordinated = observer_action.kind == action.kind
                            && observer_action.movement_direction == action.movement_direction;
                        let expected = self.next_signal_action_association(
                            observer,
                            intensity,
                            action,
                            coordinated,
                        )?;
                        if expected.0 != *from
                            || expected.1 != *to
                            || expected.2 != *inhibited_from
                            || expected.3 != *inhibited_to
                        {
                            return Err(EngineError::InvalidSignalActionAssociation(
                                observer.organism_id,
                            ));
                        }
                    }
                    _ => {
                        return Err(EngineError::InvalidSignalActionAssociation(
                            observer.organism_id,
                        ));
                    }
                }
            }
        } else if events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::OrganismSignalActionAssociationChanged { .. }
            )
        }) {
            return Err(EngineError::SignalActionAssociationUnsupported);
        }
        if self.uses_reproductive_physiology_driver() {
            let is_reproductive_event = |event: &DomainEvent| {
                matches!(
                    event,
                    DomainEvent::ReproductiveDevelopmentStarted { .. }
                        | DomainEvent::ReproductiveDevelopmentEnded { .. }
                        | DomainEvent::OrganismBorn {
                            development_id: Some(_),
                            ..
                        }
                )
            };
            if !tick_advanced {
                if events.iter().any(|event| {
                    matches!(
                        event,
                        DomainEvent::ReproductiveDevelopmentStarted { .. }
                            | DomainEvent::OrganismBorn { .. }
                    )
                }) {
                    return Err(EngineError::InvalidReproductiveEventSet);
                }
                let actual_indices = events
                    .iter()
                    .enumerate()
                    .filter_map(|(index, event)| {
                        matches!(event, DomainEvent::ReproductiveDevelopmentEnded { .. })
                            .then_some(index)
                    })
                    .collect::<Vec<_>>();
                if let (Some(first), Some(last)) = (
                    actual_indices.first().copied(),
                    actual_indices.last().copied(),
                ) && (actual_indices != (first..=last).collect::<Vec<_>>()
                    || events[..first].iter().any(|event| {
                        matches!(
                            event,
                            DomainEvent::WorldExtinct | DomainEvent::WorldArchived
                        )
                    })
                    || events[last + 1..].iter().any(|event| {
                        !matches!(
                            event,
                            DomainEvent::WorldExtinct | DomainEvent::WorldArchived
                        )
                    }))
                {
                    return Err(EngineError::InvalidReproductiveEventSet);
                }
                let core_events = events
                    .iter()
                    .filter(|event| {
                        !matches!(
                            event,
                            DomainEvent::ReproductiveDevelopmentEnded { .. }
                                | DomainEvent::WorldExtinct
                                | DomainEvent::WorldArchived
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let mut preview = self.clone();
                preview.apply_events(&core_events)?;
                let expected = preview.unavailable_reproductive_endings();
                let actual = actual_indices
                    .iter()
                    .map(|index| events[*index].clone())
                    .collect::<Vec<_>>();
                if actual != expected {
                    return Err(EngineError::InvalidReproductiveEventSet);
                }
            } else {
                if events.iter().any(|event| {
                    matches!(
                        event,
                        DomainEvent::OrganismBorn {
                            development_id: None,
                            ..
                        }
                    )
                }) {
                    return Err(EngineError::InvalidReproductiveEventSet);
                }
                let actual_indices = events
                    .iter()
                    .enumerate()
                    .filter_map(|(index, event)| is_reproductive_event(event).then_some(index))
                    .collect::<Vec<_>>();
                if let (Some(first), Some(last)) = (
                    actual_indices.first().copied(),
                    actual_indices.last().copied(),
                ) && (actual_indices != (first..=last).collect::<Vec<_>>()
                    || events[..first].iter().any(|event| {
                        matches!(
                            event,
                            DomainEvent::WorldExtinct | DomainEvent::WorldArchived
                        )
                    })
                    || events[last + 1..].iter().any(|event| {
                        !matches!(
                            event,
                            DomainEvent::WorldExtinct | DomainEvent::WorldArchived
                        )
                    }))
                {
                    return Err(EngineError::InvalidReproductiveEventSet);
                }
                let core_events = events
                    .iter()
                    .filter(|event| {
                        !is_reproductive_event(event)
                            && !matches!(
                                event,
                                DomainEvent::WorldExtinct | DomainEvent::WorldArchived
                            )
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let mut preview = self.clone();
                preview.apply_events(&core_events)?;
                let expected = preview.plan_reproductive_events()?;
                let actual = actual_indices
                    .iter()
                    .map(|index| events[*index].clone())
                    .collect::<Vec<_>>();
                if actual != expected {
                    return Err(EngineError::InvalidReproductiveEventSet);
                }
            }
        } else if events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::ReproductiveDevelopmentStarted { .. }
                    | DomainEvent::ReproductiveDevelopmentEnded { .. }
            )
        }) {
            return Err(EngineError::ReproductivePhysiologyUnsupported);
        }
        if self.uses_reproductive_physiology_driver() && self.status == WorldStatus::Running {
            let first_lifecycle = events.iter().position(|event| {
                matches!(
                    event,
                    DomainEvent::WorldExtinct | DomainEvent::WorldArchived
                )
            });
            let core_events = first_lifecycle.map_or(events, |index| &events[..index]);
            let actual_lifecycle = first_lifecycle.map_or(&[][..], |index| &events[index..]);
            let mut preview = self.clone();
            preview.apply_events(core_events)?;
            let expects_extinction = preview.living_people() == 0;
            let has_exact_extinction = matches!(
                actual_lifecycle,
                [DomainEvent::WorldExtinct, DomainEvent::WorldArchived]
            );
            if expects_extinction != has_exact_extinction
                || (!expects_extinction && !actual_lifecycle.is_empty())
            {
                return Err(EngineError::InvalidWorldLifecycleEventSet);
            }
        }
        Ok(())
    }

    fn event_schema_version(&self) -> u16 {
        if self.uses_cancer_biology_driver() {
            CANCER_BURDEN_EVENT_SCHEMA_VERSION
        } else if self.uses_world_experiment_bootstrap() {
            CANCER_RESEARCH_COHORT_EVENT_SCHEMA_VERSION
        } else if self.uses_competitive_signal_learning_driver() {
            COMPETITIVE_SIGNAL_LEARNING_EVENT_SCHEMA_VERSION
        } else if self.uses_adult_body_mass_state_driver() {
            ADULT_BODY_MASS_EVENT_SCHEMA_VERSION
        } else if self.uses_mass_scaled_metabolism_driver() {
            MASS_SCALED_METABOLISM_EVENT_SCHEMA_VERSION
        } else if self.uses_topsoil_movement_driver() {
            TOPSOIL_MOVEMENT_EVENT_SCHEMA_VERSION
        } else if self.uses_terrain_movement_driver() {
            TERRAIN_MOVEMENT_EVENT_SCHEMA_VERSION
        } else if self.uses_local_atmospheric_flux_driver() {
            LOCAL_ATMOSPHERIC_FLUX_EVENT_SCHEMA_VERSION
        } else if self.uses_local_weather_driver() {
            LOCAL_WEATHER_EVENT_SCHEMA_VERSION
        } else if self.uses_signal_motor_association_driver() {
            SIGNAL_MOTOR_ASSOCIATION_EVENT_SCHEMA_VERSION
        } else if self.uses_movement_direction_learning_driver() {
            MOVEMENT_DIRECTION_LEARNING_EVENT_SCHEMA_VERSION
        } else if self.uses_selectable_movement_driver() {
            SELECTABLE_MOVEMENT_EVENT_SCHEMA_VERSION
        } else if self.uses_signal_action_association_driver() {
            SIGNAL_ACTION_ASSOCIATION_EVENT_SCHEMA_VERSION
        } else if self.uses_material_surface_regions_driver() {
            MATERIAL_SURFACE_REGIONS_EVENT_SCHEMA_VERSION
        } else if self.uses_material_surface_trace_driver() {
            MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION
        } else if self.uses_social_learning_driver() {
            SOCIAL_LEARNING_EVENT_SCHEMA_VERSION
        } else if self.uses_material_reservoir_driver() {
            MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION
        } else if self.uses_cognition_driver() {
            COGNITION_EVENT_SCHEMA_VERSION
        } else if self.uses_heritable_disposition_driver() {
            HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION
        } else if self.uses_reproductive_physiology_driver() {
            REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION
        } else if self.uses_action_learning_driver() {
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
        if self.uses_cancer_biology_driver() {
            CANCER_BURDEN_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_world_experiment_bootstrap() {
            CANCER_RESEARCH_COHORT_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_adult_body_mass_state_driver() {
            ADULT_BODY_MASS_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_mass_scaled_metabolism_driver() {
            MASS_SCALED_METABOLISM_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_topsoil_movement_driver() {
            TOPSOIL_MOVEMENT_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_terrain_movement_driver() {
            TERRAIN_MOVEMENT_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_local_atmospheric_flux_driver() {
            LOCAL_ATMOSPHERIC_FLUX_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_local_weather_driver() {
            LOCAL_WEATHER_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_signal_action_association_driver() {
            SIGNAL_ACTION_ASSOCIATION_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_material_surface_regions_driver() {
            MATERIAL_SURFACE_REGIONS_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_material_surface_trace_driver() {
            MATERIAL_SURFACE_TRACE_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_social_learning_driver() {
            SOCIAL_LEARNING_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_material_reservoir_driver() {
            MATERIAL_RESERVOIR_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_cognition_driver() {
            COGNITION_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_heritable_disposition_driver() {
            HERITABLE_DISPOSITION_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_reproductive_physiology_driver() {
            REPRODUCTIVE_PHYSIOLOGY_STATE_HASH_SCHEMA_VERSION
        } else if self.uses_action_learning_driver() {
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
                    || !self.pending_reproductive_developments.is_empty()
                    || !self.pending_cognition_requests.is_empty()
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
                    || !self.pending_reproductive_developments.is_empty()
                    || !self.pending_cognition_requests.is_empty()
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
                reproductive_physiology,
                heritable_disposition_profile,
                heritable_disposition,
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
                if let Some(reproduction) = reproductive_physiology {
                    reproduction
                        .validate()
                        .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                    if reproduction.species != *species
                        || !reproduction.supports_category(birth_category)
                        || self.configuration.as_ref().is_none_or(|world| {
                            world.tick_duration_seconds != reproduction.tick_duration_seconds
                        })
                    {
                        return Err(EngineError::InvalidReproductiveCommitment(*organism_id));
                    }
                }
                if self.uses_reproductive_physiology_driver() && reproductive_physiology.is_none() {
                    return Err(EngineError::MissingReproductiveCommitment(*organism_id));
                }
                if !self.uses_reproductive_physiology_driver() && reproductive_physiology.is_some()
                {
                    return Err(EngineError::ReproductivePhysiologyUnsupported);
                }
                if self.uses_heritable_disposition_driver() {
                    let profile = heritable_disposition_profile.as_ref().ok_or(
                        EngineError::MissingHeritableDispositionProfile(*organism_id),
                    )?;
                    let disposition = heritable_disposition
                        .as_ref()
                        .ok_or(EngineError::InvalidHeritableDisposition)?;
                    if profile.species != *species
                        || disposition
                            != &self.founder_heritable_disposition(*organism_id, profile)?
                    {
                        return Err(EngineError::InvalidHeritableDisposition);
                    }
                } else if heritable_disposition_profile.is_some() || heritable_disposition.is_some()
                {
                    return Err(EngineError::HeritableDispositionUnsupported);
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
                    adult_body_mass: None,
                    physiological_regulation: physiological_regulation.clone(),
                    reproductive_physiology: reproductive_physiology.clone(),
                    reproductive_available_at: None,
                    heritable_disposition_profile: heritable_disposition_profile.clone(),
                    heritable_disposition: heritable_disposition.clone(),
                    bodily_regulation: BodilyRegulationState::default(),
                    bodily_regulated_at: None,
                    perception_memory: Vec::new(),
                    action_values: Vec::new(),
                    action_values_updated_at: None,
                    movement_direction_values: Vec::new(),
                    movement_direction_values_updated_at: None,
                    social_action_values: Vec::new(),
                    social_action_values_updated_at: None,
                    signal_action_associations: Vec::new(),
                    signal_action_associations_updated_at: None,
                    death: None,
                })?;
                self.refresh_partition_schedule()?;
            }
            DomainEvent::OrganismAdultBodyMassCommitted {
                organism_id,
                commitment,
            } => {
                if !self.uses_adult_body_mass_state_driver() {
                    return Err(EngineError::InvalidEmbodiedEvent(
                        "adult-body-mass state requires ruleset 32".to_owned(),
                    ));
                }
                commitment
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                let organism = self
                    .organisms
                    .get_mut(organism_id)
                    .ok_or(EngineError::UnknownOrganism(*organism_id))?;
                if commitment.species != organism.species || organism.adult_body_mass.is_some() {
                    return Err(EngineError::InvalidEmbodiedEvent(
                        "adult-body-mass commitment is duplicated or species-mismatched".to_owned(),
                    ));
                }
                organism.adult_body_mass = Some(commitment.clone());
            }
            DomainEvent::CancerResearchCohortCommitted {
                affected_resident_ids,
            } => {
                if self.tick != SimTick::ZERO
                    || !self.uses_world_experiment_bootstrap()
                    || !self.initial_cancer_research_cohort.is_empty()
                {
                    return Err(EngineError::InvalidCancerResearchInitialCohort);
                }
                let expected = seeded_cancer_research_cohort(
                    self.manifest.seed,
                    self.organisms.values().filter_map(|organism| {
                        (organism.role == OrganismRole::Person)
                            .then_some((organism.organism_id, organism.birth_category.as_str()))
                    }),
                )?;
                if affected_resident_ids != &expected {
                    return Err(EngineError::InvalidCancerResearchInitialCohort);
                }
                self.initial_cancer_research_cohort = expected.iter().copied().collect();
                if self.uses_cancer_biology_driver() {
                    let target = match &self.manifest.experiment {
                        Some(WorldExperimentCommitment::CancerResearch(commitment)) => {
                            commitment.target
                        }
                        None => return Err(EngineError::InvalidCancerResearchBurden),
                    };
                    self.cancer_burdens = expected
                        .into_iter()
                        .map(|resident_id| {
                            CancerBurdenState::seeded_initial(
                                self.manifest.seed,
                                resident_id,
                                target,
                            )
                            .map(|burden| (resident_id, burden))
                            .map_err(|_| EngineError::InvalidCancerResearchBurden)
                        })
                        .collect::<Result<_, _>>()?;
                }
            }
            DomainEvent::CancerBurdensAdvanced {
                day_ordinal,
                transitions,
            } => {
                if !self.uses_cancer_biology_driver() || *day_ordinal == 0 {
                    return Err(EngineError::InvalidCancerResearchBurden);
                }
                let expected = self.plan_cancer_burden_events()?;
                if expected
                    != [DomainEvent::CancerBurdensAdvanced {
                        day_ordinal: *day_ordinal,
                        transitions: transitions.clone(),
                    }]
                {
                    return Err(EngineError::InvalidCancerResearchBurden);
                }
                for transition in transitions {
                    self.cancer_burdens
                        .insert(transition.resident_id, transition.to.clone());
                }
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
                            surface_trace_units: 0,
                            surface_region_trace_units: if self
                                .uses_material_surface_regions_driver()
                            {
                                vec![0; MATERIAL_SURFACE_REGION_COUNT]
                            } else {
                                Vec::new()
                            },
                            oral_transfer_profiles: oral_transfer_profiles.clone(),
                            reservoir: None,
                            reservoir_settled_at: None,
                        },
                    )
                    .is_some()
                {
                    return Err(EngineError::DuplicateMaterialInstance(*object_id));
                }
            }
            DomainEvent::MaterialReservoirCommitted {
                object_id,
                commitment,
            } => {
                if !self.uses_material_reservoir_driver() || self.tick != SimTick::ZERO {
                    return Err(EngineError::MaterialReservoirUnsupported);
                }
                commitment
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                let instance = self
                    .material_instances
                    .get_mut(object_id)
                    .ok_or(EngineError::UnknownMaterialInstance(*object_id))?;
                let initial_mass = instance
                    .remaining_mass_milligrams
                    .ok_or(EngineError::MissingMaterialMass(*object_id))?;
                if instance.material != commitment.material
                    || instance.reservoir.is_some()
                    || initial_mass > commitment.maximum_mass_milligrams
                    || !commitment.coverage_patch.contains(instance.embodied_patch)
                {
                    return Err(EngineError::InvalidMaterialReservoir(*object_id));
                }
                instance.reservoir = Some(commitment.clone());
                instance.reservoir_settled_at = Some(self.tick);
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
            DomainEvent::MaterialSurfaceTraceChanged {
                object_id,
                organism_id,
                from_trace_units,
                applied_force_units,
                to_trace_units,
            } => {
                if !self.uses_material_surface_trace_driver()
                    || self.uses_material_surface_regions_driver()
                {
                    return Err(EngineError::MaterialSurfaceTraceUnsupported);
                }
                self.require_living_organism(*organism_id)?;
                let instance = self
                    .material_instances
                    .get_mut(object_id)
                    .ok_or(EngineError::UnknownMaterialInstance(*object_id))?;
                let expected_to = from_trace_units
                    .checked_add(u32::from(*applied_force_units))
                    .filter(|to| *to <= MAX_MATERIAL_SURFACE_TRACE_UNITS)
                    .ok_or(EngineError::InvalidMaterialSurfaceTrace(*object_id))?;
                if instance.held_by != Some(*organism_id)
                    || instance.surface_trace_units != *from_trace_units
                    || expected_to != *to_trace_units
                {
                    return Err(EngineError::InvalidMaterialSurfaceTrace(*object_id));
                }
                instance.surface_trace_units = *to_trace_units;
            }
            DomainEvent::MaterialSurfaceRegionTraceChanged {
                object_id,
                organism_id,
                contact_region,
                from_region_trace_units,
                from_total_trace_units,
                applied_force_units,
                to_region_trace_units,
                to_total_trace_units,
            } => {
                if !self.uses_material_surface_regions_driver() {
                    return Err(EngineError::MaterialSurfaceRegionsUnsupported);
                }
                self.require_living_organism(*organism_id)?;
                let instance = self
                    .material_instances
                    .get_mut(object_id)
                    .ok_or(EngineError::UnknownMaterialInstance(*object_id))?;
                let region_index = usize::from(*contact_region);
                let force = u32::from(*applied_force_units);
                if region_index >= MATERIAL_SURFACE_REGION_COUNT
                    || instance.surface_region_trace_units.len() != MATERIAL_SURFACE_REGION_COUNT
                    || instance.held_by != Some(*organism_id)
                    || instance.surface_region_trace_units[region_index] != *from_region_trace_units
                    || instance.surface_trace_units != *from_total_trace_units
                    || from_region_trace_units.checked_add(force) != Some(*to_region_trace_units)
                    || from_total_trace_units.checked_add(force) != Some(*to_total_trace_units)
                    || *to_region_trace_units > MAX_MATERIAL_SURFACE_TRACE_UNITS
                    || *to_total_trace_units > MAX_MATERIAL_SURFACE_TRACE_UNITS
                {
                    return Err(EngineError::InvalidMaterialSurfaceRegions(*object_id));
                }
                instance.surface_region_trace_units[region_index] = *to_region_trace_units;
                instance.surface_trace_units = *to_total_trace_units;
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
            DomainEvent::MaterialReservoirOralPortionTransferred {
                object_id,
                organism_id,
                profile_digest,
                settled_from_tick,
                settled_to_tick,
                from_mass_milligrams,
                replenished_mass_milligrams,
                transferred_mass_milligrams,
                to_mass_milligrams,
            } => {
                if !self.uses_material_reservoir_driver() {
                    return Err(EngineError::MaterialReservoirUnsupported);
                }
                self.require_living_organism(*organism_id)?;
                let organism = self
                    .organisms
                    .get(organism_id)
                    .expect("living organism presence checked");
                let organism_species = organism.species.clone();
                let organism_patch = organism
                    .embodied_patch
                    .ok_or(EngineError::MissingEmbodiedPatch(*organism_id))?;
                let instance = self
                    .material_instances
                    .get_mut(object_id)
                    .ok_or(EngineError::UnknownMaterialInstance(*object_id))?;
                let reservoir = instance
                    .reservoir
                    .as_ref()
                    .ok_or(EngineError::InvalidMaterialReservoir(*object_id))?;
                let profile = instance
                    .oral_transfer_profiles
                    .iter()
                    .find(|profile| {
                        profile.species == organism_species
                            && profile.profile_digest == *profile_digest
                    })
                    .ok_or(EngineError::InvalidMaterialOralTransfer(*object_id))?;
                let current_mass = instance
                    .remaining_mass_milligrams
                    .ok_or(EngineError::MissingMaterialMass(*object_id))?;
                let current_settled_at = instance
                    .reservoir_settled_at
                    .ok_or(EngineError::InvalidMaterialReservoir(*object_id))?;
                let elapsed_ticks = settled_to_tick
                    .get()
                    .checked_sub(settled_from_tick.get())
                    .ok_or(EngineError::InvalidMaterialReservoir(*object_id))?;
                let capacity_remaining = reservoir
                    .maximum_mass_milligrams
                    .checked_sub(current_mass)
                    .ok_or(EngineError::InvalidMaterialReservoir(*object_id))?;
                let expected_replenished = reservoir
                    .replenishment_mass_milligrams_per_tick
                    .saturating_mul(elapsed_ticks)
                    .min(capacity_remaining);
                let expected_to = current_mass
                    .checked_add(expected_replenished)
                    .and_then(|available| available.checked_sub(*transferred_mass_milligrams))
                    .ok_or(EngineError::InvalidMaterialOralTransfer(*object_id))?;
                if instance.held_by.is_some()
                    || !reservoir.coverage_patch.contains(organism_patch)
                    || current_mass != *from_mass_milligrams
                    || current_settled_at != *settled_from_tick
                    || *settled_to_tick != self.tick
                    || *replenished_mass_milligrams != expected_replenished
                    || *transferred_mass_milligrams != profile.transfer_mass_milligrams
                    || *to_mass_milligrams != expected_to
                {
                    return Err(EngineError::InvalidMaterialOralTransfer(*object_id));
                }
                instance.remaining_mass_milligrams = Some(*to_mass_milligrams);
                instance.reservoir_settled_at = Some(*settled_to_tick);
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
            DomainEvent::ReproductiveDevelopmentStarted {
                development_id,
                offspring_id,
                species,
                role,
                birth_category,
                parent_ids,
                developing_parent_id,
                profile_digest,
                due_tick,
                parents_available_at,
                heritable_disposition_profile,
                offspring_heritable_disposition,
            } => {
                if !self.uses_reproductive_physiology_driver() {
                    return Err(EngineError::ReproductivePhysiologyUnsupported);
                }
                if parent_ids.len() != 2
                    || self
                        .pending_reproductive_developments
                        .contains_key(development_id)
                    || self.organisms.contains_key(offspring_id)
                    || development_id == offspring_id
                {
                    return Err(EngineError::InvalidReproductiveDevelopment(*development_id));
                }
                let left = self
                    .organisms
                    .get(&parent_ids[0])
                    .ok_or(EngineError::UnknownParent(parent_ids[0]))?;
                let right = self
                    .organisms
                    .get(&parent_ids[1])
                    .ok_or(EngineError::UnknownParent(parent_ids[1]))?;
                let Some((profile, expected_developing_parent)) =
                    self.reproductive_pair(left, right)
                else {
                    return Err(EngineError::InvalidReproductiveDevelopment(*development_id));
                };
                let expected_due = self
                    .tick
                    .get()
                    .checked_add(profile.development_ticks)
                    .ok_or(EngineError::ReproductiveArithmetic)?;
                let expected_available = expected_due
                    .checked_add(profile.recovery_ticks)
                    .ok_or(EngineError::ReproductiveArithmetic)?;
                let expected_development_id = EntityId::deterministic(
                    self.world_id(),
                    self.reproductive_draw("development-identity", self.tick, parent_ids)?
                        .as_bytes(),
                );
                let expected_offspring_id = EntityId::deterministic(
                    self.world_id(),
                    self.reproductive_draw("offspring-identity", self.tick, parent_ids)?
                        .as_bytes(),
                );
                let expected_heritable_disposition = if self.uses_heritable_disposition_driver() {
                    let heritable_profile = left.heritable_disposition_profile.as_ref().ok_or(
                        EngineError::MissingHeritableDispositionProfile(left.organism_id),
                    )?;
                    if right.heritable_disposition_profile.as_ref() != Some(heritable_profile) {
                        return Err(EngineError::InvalidHeritableDisposition);
                    }
                    Some(self.offspring_heritable_disposition(
                        *offspring_id,
                        parent_ids,
                        self.tick,
                        heritable_profile,
                    )?)
                } else {
                    None
                };
                if !self.reproductively_ready(left, &profile)
                    || !self.reproductively_ready(right, &profile)
                    || !self
                        .reproductive_opportunity_succeeds_at(&profile, parent_ids, self.tick)?
                    || self.offspring_category_at(&profile, parent_ids, self.tick)?
                        != *birth_category
                    || profile.species != *species
                    || left.role != *role
                    || profile.profile_digest != *profile_digest
                    || expected_developing_parent != *developing_parent_id
                    || expected_development_id != *development_id
                    || expected_offspring_id != *offspring_id
                    || *due_tick != SimTick::new(expected_due)
                    || *parents_available_at != SimTick::new(expected_available)
                    || if self.uses_heritable_disposition_driver() {
                        left.heritable_disposition_profile.as_ref()
                            != heritable_disposition_profile.as_ref()
                            || expected_heritable_disposition.as_ref()
                                != offspring_heritable_disposition.as_ref()
                    } else {
                        heritable_disposition_profile.is_some()
                            || offspring_heritable_disposition.is_some()
                    }
                {
                    return Err(EngineError::InvalidReproductiveDevelopment(*development_id));
                }
                for parent_id in parent_ids {
                    self.organisms
                        .get_mut(parent_id)
                        .expect("parent presence checked")
                        .reproductive_available_at = Some(*parents_available_at);
                }
                self.pending_reproductive_developments.insert(
                    *development_id,
                    PendingReproductiveDevelopment {
                        development_id: *development_id,
                        offspring_id: *offspring_id,
                        species: species.clone(),
                        role: *role,
                        birth_category: birth_category.clone(),
                        parent_ids: parent_ids.clone(),
                        developing_parent_id: *developing_parent_id,
                        profile_digest: *profile_digest,
                        started_at: self.tick,
                        due_tick: *due_tick,
                        parents_available_at: *parents_available_at,
                        heritable_disposition_profile: heritable_disposition_profile.clone(),
                        offspring_heritable_disposition: offspring_heritable_disposition.clone(),
                    },
                );
            }
            DomainEvent::ReproductiveDevelopmentEnded {
                development_id,
                developing_parent_id,
                reason: ReproductiveDevelopmentEnd::DevelopingParentUnavailable,
            } => {
                if !self.uses_reproductive_physiology_driver() {
                    return Err(EngineError::ReproductivePhysiologyUnsupported);
                }
                let pending = self
                    .pending_reproductive_developments
                    .get(development_id)
                    .ok_or(EngineError::UnknownReproductiveDevelopment(*development_id))?;
                if pending.developing_parent_id != *developing_parent_id
                    || self
                        .organisms
                        .get(developing_parent_id)
                        .is_some_and(OrganismState::is_alive)
                {
                    return Err(EngineError::InvalidReproductiveDevelopment(*development_id));
                }
                self.pending_reproductive_developments
                    .remove(development_id);
            }
            DomainEvent::OrganismBorn {
                organism_id,
                development_id,
                species,
                role,
                birth_category,
                parent_ids,
                location_id,
                embodied_patch,
                metabolic_rate,
                physiological_regulation,
                reproductive_physiology,
                heritable_disposition_profile,
                heritable_disposition,
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
                if let Some(reproduction) = reproductive_physiology {
                    reproduction
                        .validate()
                        .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                    if reproduction.species != *species
                        || !reproduction.supports_category(birth_category)
                        || self.configuration.as_ref().is_none_or(|world| {
                            world.tick_duration_seconds != reproduction.tick_duration_seconds
                        })
                    {
                        return Err(EngineError::InvalidReproductiveCommitment(*organism_id));
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
                if self.uses_reproductive_physiology_driver() {
                    let development_id = development_id
                        .ok_or(EngineError::UnboundReproductiveBirth(*organism_id))?;
                    let pending = self
                        .pending_reproductive_developments
                        .get(&development_id)
                        .cloned()
                        .ok_or(EngineError::UnknownReproductiveDevelopment(development_id))?;
                    let developing_parent = self
                        .organisms
                        .get(&pending.developing_parent_id)
                        .ok_or(EngineError::UnknownParent(pending.developing_parent_id))?;
                    if !developing_parent.is_alive()
                        || pending.offspring_id != *organism_id
                        || pending.species != *species
                        || pending.role != *role
                        || pending.birth_category != *birth_category
                        || pending.parent_ids != *parent_ids
                        || pending.due_tick != self.tick
                        || developing_parent.location_id != *location_id
                        || developing_parent.embodied_patch != *embodied_patch
                        || developing_parent.metabolic_rate != *metabolic_rate
                        || developing_parent.physiological_regulation != *physiological_regulation
                        || developing_parent.reproductive_physiology != *reproductive_physiology
                        || pending.heritable_disposition_profile != *heritable_disposition_profile
                        || pending.offspring_heritable_disposition != *heritable_disposition
                        || reproductive_physiology
                            .as_ref()
                            .is_none_or(|profile| profile.profile_digest != pending.profile_digest)
                    {
                        return Err(EngineError::InvalidReproductiveBirth(*organism_id));
                    }
                    self.pending_reproductive_developments
                        .remove(&development_id);
                } else if development_id.is_some() || reproductive_physiology.is_some() {
                    return Err(EngineError::ReproductivePhysiologyUnsupported);
                }
                if self.uses_heritable_disposition_driver() {
                    let profile = heritable_disposition_profile.as_ref().ok_or(
                        EngineError::MissingHeritableDispositionProfile(*organism_id),
                    )?;
                    let disposition = heritable_disposition
                        .as_ref()
                        .ok_or(EngineError::InvalidHeritableDisposition)?;
                    if profile.species != *species
                        || disposition.validate_against(profile).is_err()
                        || disposition.derived_at >= self.tick
                    {
                        return Err(EngineError::InvalidHeritableDisposition);
                    }
                } else if heritable_disposition_profile.is_some() || heritable_disposition.is_some()
                {
                    return Err(EngineError::HeritableDispositionUnsupported);
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
                    adult_body_mass: None,
                    physiological_regulation: physiological_regulation.clone(),
                    reproductive_physiology: reproductive_physiology.clone(),
                    reproductive_available_at: None,
                    heritable_disposition_profile: heritable_disposition_profile.clone(),
                    heritable_disposition: heritable_disposition.clone(),
                    bodily_regulation: BodilyRegulationState::default(),
                    bodily_regulated_at: None,
                    perception_memory: Vec::new(),
                    action_values: Vec::new(),
                    action_values_updated_at: None,
                    movement_direction_values: Vec::new(),
                    movement_direction_values_updated_at: None,
                    social_action_values: Vec::new(),
                    social_action_values_updated_at: None,
                    signal_action_associations: Vec::new(),
                    signal_action_associations_updated_at: None,
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
            DomainEvent::OrganismMovementDirectionValueChanged {
                organism_id,
                from,
                to,
            } => {
                if !self.uses_movement_direction_learning_driver() {
                    return Err(EngineError::MovementDirectionLearningUnsupported);
                }
                self.require_living_organism(*organism_id)?;
                to.validate().map_err(|_| {
                    EngineError::InvalidMovementDirectionValueTransition(*organism_id)
                })?;
                let organism = self
                    .organisms
                    .get_mut(organism_id)
                    .expect("living organism presence checked");
                let expected_observations = from.map_or(Ok(1), |from| {
                    from.observations.checked_add(1).ok_or(
                        EngineError::MovementDirectionValueObservationOverflow(*organism_id),
                    )
                })?;
                if organism.movement_direction_values_updated_at == Some(self.tick)
                    || organism.movement_direction_value(to.movement_direction) != *from
                    || to.observations != expected_observations
                    || from.is_some_and(|from| from.movement_direction != to.movement_direction)
                {
                    return Err(EngineError::InvalidMovementDirectionValueTransition(
                        *organism_id,
                    ));
                }
                match organism
                    .movement_direction_values
                    .binary_search_by_key(&to.movement_direction, |entry| entry.movement_direction)
                {
                    Ok(index) => organism.movement_direction_values[index] = *to,
                    Err(index) => organism.movement_direction_values.insert(index, *to),
                }
                organism.movement_direction_values_updated_at = Some(self.tick);
            }
            DomainEvent::OrganismSocialActionValueChanged {
                observer_id,
                actor_id,
                from,
                to,
            } => {
                if !self.uses_social_learning_driver() || observer_id == actor_id {
                    return Err(EngineError::InvalidSocialActionValueTransition(
                        *observer_id,
                    ));
                }
                self.require_living_organism(*observer_id)?;
                self.require_living_organism(*actor_id)?;
                to.validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                let observer = self
                    .organisms
                    .get_mut(observer_id)
                    .expect("living observer presence checked");
                let expected_observations = from.map_or(Ok(1), |from| {
                    from.observations
                        .checked_add(1)
                        .ok_or(EngineError::ActionValueObservationOverflow(*observer_id))
                })?;
                let expected_value = from
                    .map_or(1_i16, |from| from.value.saturating_add(1))
                    .min(ACTION_VALUE_MAX);
                if observer.social_action_values_updated_at == Some(self.tick)
                    || observer.social_action_value(to.action_kind) != *from
                    || to.observations != expected_observations
                    || to.value != expected_value
                    || from.is_some_and(|from| from.action_kind != to.action_kind)
                {
                    return Err(EngineError::InvalidSocialActionValueTransition(
                        *observer_id,
                    ));
                }
                match observer
                    .social_action_values
                    .binary_search_by_key(&to.action_kind, |entry| entry.action_kind)
                {
                    Ok(index) => observer.social_action_values[index] = *to,
                    Err(index) => observer.social_action_values.insert(index, *to),
                }
                observer.social_action_values_updated_at = Some(self.tick);
            }
            DomainEvent::OrganismSignalActionAssociationChanged {
                observer_id,
                actor_id,
                from,
                to,
                inhibited_from,
                inhibited_to,
            } => {
                if !self.uses_signal_action_association_driver() || observer_id == actor_id {
                    return Err(EngineError::InvalidSignalActionAssociation(*observer_id));
                }
                self.require_living_organism(*observer_id)?;
                self.require_living_organism(*actor_id)?;
                to.validate()
                    .map_err(|_| EngineError::InvalidSignalActionAssociation(*observer_id))?;
                let maximum = if self.uses_signal_motor_association_driver() {
                    MAX_SIGNAL_MOTOR_ASSOCIATIONS
                } else {
                    MAX_SIGNAL_ACTION_ASSOCIATIONS
                };
                let competitive_signal_learning = self.uses_competitive_signal_learning_driver();
                let observer = self
                    .organisms
                    .get_mut(observer_id)
                    .expect("living observer presence checked");
                let expected_observations = from.map_or(Ok(1), |from| {
                    from.observations
                        .checked_add(1)
                        .ok_or(EngineError::ActionValueObservationOverflow(*observer_id))
                })?;
                if observer.signal_action_associations_updated_at == Some(self.tick)
                    || observer.signal_action_association(
                        to.signal_intensity,
                        to.action_kind,
                        to.movement_direction,
                    ) != *from
                    || to.observations != expected_observations
                {
                    return Err(EngineError::InvalidSignalActionAssociation(*observer_id));
                }
                if competitive_signal_learning {
                    let valid_reinforcement = from.map_or_else(
                        || {
                            matches!(
                                to.value,
                                SIGNAL_PREDICTION_REINFORCEMENT | SIGNAL_COORDINATION_REINFORCEMENT
                            )
                        },
                        |from| {
                            to.value
                                == from
                                    .value
                                    .saturating_add(SIGNAL_PREDICTION_REINFORCEMENT)
                                    .min(ACTION_VALUE_MAX)
                                || to.value
                                    == from
                                        .value
                                        .saturating_add(SIGNAL_COORDINATION_REINFORCEMENT)
                                        .min(ACTION_VALUE_MAX)
                        },
                    );
                    if to.association_schema_version
                        != COMPETITIVE_SIGNAL_ASSOCIATION_SCHEMA_VERSION
                        || !valid_reinforcement
                    {
                        return Err(EngineError::InvalidSignalActionAssociation(*observer_id));
                    }
                    match (inhibited_from, inhibited_to) {
                        (None, None) => {}
                        (Some(inhibited_from), Some(inhibited_to)) => {
                            let current = observer.signal_action_association(
                                inhibited_from.signal_intensity,
                                inhibited_from.action_kind,
                                inhibited_from.movement_direction,
                            );
                            if current != Some(*inhibited_from)
                                || inhibited_to.association_schema_version
                                    != COMPETITIVE_SIGNAL_ASSOCIATION_SCHEMA_VERSION
                                || inhibited_to.observations
                                    != inhibited_from.observations.saturating_add(1)
                                || inhibited_to.value
                                    != inhibited_from
                                        .value
                                        .saturating_sub(SIGNAL_PREDICTION_INHIBITION)
                                        .max(1)
                            {
                                return Err(EngineError::InvalidSignalActionAssociation(
                                    *observer_id,
                                ));
                            }
                        }
                        _ => {
                            return Err(EngineError::InvalidSignalActionAssociation(*observer_id));
                        }
                    }
                } else {
                    let expected_value = from
                        .map_or(1_i16, |from| from.value.saturating_add(1))
                        .min(ACTION_VALUE_MAX);
                    if to.value != expected_value
                        || inhibited_from.is_some()
                        || inhibited_to.is_some()
                    {
                        return Err(EngineError::InvalidSignalActionAssociation(*observer_id));
                    }
                }
                let key = (to.signal_intensity, to.action_kind, to.movement_direction);
                match observer
                    .signal_action_associations
                    .binary_search_by_key(&key, |entry| {
                        (
                            entry.signal_intensity,
                            entry.action_kind,
                            entry.movement_direction,
                        )
                    }) {
                    Ok(index) => observer.signal_action_associations[index] = *to,
                    Err(index) => {
                        if observer.signal_action_associations.len() >= maximum {
                            return Err(EngineError::InvalidSignalActionAssociation(*observer_id));
                        }
                        observer.signal_action_associations.insert(index, *to);
                    }
                }
                if let Some(inhibited_to) = inhibited_to {
                    let inhibited_key = (
                        inhibited_to.signal_intensity,
                        inhibited_to.action_kind,
                        inhibited_to.movement_direction,
                    );
                    let inhibited_index = observer
                        .signal_action_associations
                        .binary_search_by_key(&inhibited_key, |entry| {
                            (
                                entry.signal_intensity,
                                entry.action_kind,
                                entry.movement_direction,
                            )
                        })
                        .map_err(|_| EngineError::InvalidSignalActionAssociation(*observer_id))?;
                    observer.signal_action_associations[inhibited_index] = *inhibited_to;
                }
                observer.signal_action_associations_updated_at = Some(self.tick);
            }
            DomainEvent::CognitionRequestSelected { selection } => {
                if !self.uses_cognition_driver() {
                    return Err(EngineError::CognitionUnsupported);
                }
                selection
                    .validate()
                    .map_err(|error| EngineError::InvalidCognitionSelection(error.to_string()))?;
                if selection.world_id != self.world_id()
                    || selection.selected_at_tick != self.tick
                    || selection.deadline_tick <= self.tick
                {
                    return Err(EngineError::InvalidCognitionSelection(
                        "selection world or simulation-time boundary is invalid".to_owned(),
                    ));
                }
                if !self.pending_cognition_requests.is_empty() {
                    return Err(EngineError::CognitionRequestAlreadyPending);
                }
                let expected = self.expected_cognition_selection(selection.organism_id)?;
                if selection != &expected {
                    return Err(EngineError::InvalidCognitionSelection(
                        "selection does not match canonical body-owned inputs and ruleset budgets"
                            .to_owned(),
                    ));
                }
                if self
                    .pending_cognition_requests
                    .insert(selection.request_id, selection.clone())
                    .is_some()
                {
                    return Err(EngineError::DuplicateCognitionRequest(selection.request_id));
                }
            }
            DomainEvent::CognitionInputRecorded { input } => {
                if !self.uses_cognition_driver() {
                    return Err(EngineError::CognitionUnsupported);
                }
                let selection = self
                    .pending_cognition_requests
                    .get(&input.request_id)
                    .ok_or(EngineError::UnknownCognitionRequest(input.request_id))?;
                input
                    .validate_against(selection)
                    .map_err(|error| EngineError::InvalidCognitionInput(error.to_string()))?;
                if self.tick > selection.deadline_tick {
                    return Err(EngineError::InvalidCognitionInput(
                        "deadline input arrived after its simulated-time deadline".to_owned(),
                    ));
                }
                if self.tick < selection.deadline_tick {
                    let valid_early_resolution = match &input.outcome {
                        CognitionInputOutcome::Unavailable {
                            reason: CognitionUnavailableReason::SubjectUnavailable,
                        } => self
                            .organisms
                            .get(&selection.organism_id)
                            .is_none_or(|organism| !organism.is_alive()),
                        CognitionInputOutcome::Unavailable {
                            reason: CognitionUnavailableReason::WorldArchived,
                        } => self.living_people() == 0,
                        _ => false,
                    };
                    if !valid_early_resolution {
                        return Err(EngineError::InvalidCognitionInput(
                            "only a mechanically unavailable subject or world may resolve early"
                                .to_owned(),
                        ));
                    }
                } else if matches!(
                    &input.outcome,
                    CognitionInputOutcome::Model(_) if self
                        .organisms
                        .get(&selection.organism_id)
                        .is_none_or(|organism| !organism.is_alive())
                ) {
                    return Err(EngineError::InvalidCognitionInput(
                        "model input cannot bias an unavailable subject".to_owned(),
                    ));
                }
                self.pending_cognition_requests.remove(&input.request_id);
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

    /// Resolve all ruleset-seventeen oral transfers after the partition barrier has
    /// fixed action order. Shared reservoirs therefore have one deterministic mass
    /// sequence even when many organisms act against the same source in one tick.
    fn plan_ordered_oral_transfers(
        &self,
        events: &[DomainEvent],
    ) -> Result<Vec<DomainEvent>, EngineError> {
        let mut remaining = self
            .material_instances
            .iter()
            .filter_map(|(object_id, instance)| {
                instance
                    .remaining_mass_milligrams
                    .map(|mass| (*object_id, mass))
            })
            .collect::<BTreeMap<_, _>>();
        let mut reservoir_settled_at = self
            .material_instances
            .iter()
            .filter_map(|(object_id, instance)| {
                instance
                    .reservoir_settled_at
                    .map(|settled_at| (*object_id, settled_at))
            })
            .collect::<BTreeMap<_, _>>();
        let settlement_tick = self.tick.checked_next()?;
        let mut transfers = Vec::new();
        for event in events {
            let DomainEvent::OrganismActed {
                organism_id,
                action,
            } = event
            else {
                continue;
            };
            if action.kind != PrimitiveActionKind::Swallow {
                continue;
            }
            let Some(object_id) = action.target_id else {
                continue;
            };
            let organism = self
                .organisms
                .get(organism_id)
                .ok_or(EngineError::UnknownOrganism(*organism_id))?;
            let embodied_patch = organism
                .embodied_patch
                .ok_or(EngineError::MissingEmbodiedPatch(*organism_id))?;
            let instance = self
                .material_instances
                .get(&object_id)
                .ok_or(EngineError::UnknownMaterialInstance(object_id))?;
            let accessible = match &instance.reservoir {
                Some(reservoir) => {
                    instance.held_by.is_none() && reservoir.coverage_patch.contains(embodied_patch)
                }
                None => instance.held_by == Some(*organism_id),
            };
            if !accessible {
                return Err(EngineError::InvalidMaterialOralTransfer(object_id));
            }
            let Some(profile) = instance
                .oral_transfer_profiles
                .iter()
                .find(|profile| profile.species == organism.species)
            else {
                continue;
            };
            let from_mass_milligrams = *remaining
                .get(&object_id)
                .ok_or(EngineError::MissingMaterialMass(object_id))?;
            if let Some(reservoir) = &instance.reservoir {
                let settled_from_tick = *reservoir_settled_at
                    .get(&object_id)
                    .ok_or(EngineError::InvalidMaterialReservoir(object_id))?;
                let elapsed_ticks = settlement_tick
                    .get()
                    .checked_sub(settled_from_tick.get())
                    .ok_or(EngineError::InvalidMaterialReservoir(object_id))?;
                let capacity_remaining = reservoir
                    .maximum_mass_milligrams
                    .checked_sub(from_mass_milligrams)
                    .ok_or(EngineError::InvalidMaterialReservoir(object_id))?;
                let replenished_mass_milligrams = reservoir
                    .replenishment_mass_milligrams_per_tick
                    .saturating_mul(elapsed_ticks)
                    .min(capacity_remaining);
                let available = from_mass_milligrams
                    .checked_add(replenished_mass_milligrams)
                    .ok_or(EngineError::InvalidMaterialReservoir(object_id))?;
                let Some(to_mass_milligrams) =
                    available.checked_sub(profile.transfer_mass_milligrams)
                else {
                    continue;
                };
                remaining.insert(object_id, to_mass_milligrams);
                reservoir_settled_at.insert(object_id, settlement_tick);
                transfers.push(DomainEvent::MaterialReservoirOralPortionTransferred {
                    object_id,
                    organism_id: *organism_id,
                    profile_digest: profile.profile_digest,
                    settled_from_tick,
                    settled_to_tick: settlement_tick,
                    from_mass_milligrams,
                    replenished_mass_milligrams,
                    transferred_mass_milligrams: profile.transfer_mass_milligrams,
                    to_mass_milligrams,
                });
            } else {
                let Some(to_mass_milligrams) =
                    from_mass_milligrams.checked_sub(profile.transfer_mass_milligrams)
                else {
                    continue;
                };
                remaining.insert(object_id, to_mass_milligrams);
                transfers.push(DomainEvent::MaterialOralPortionTransferred {
                    object_id,
                    organism_id: *organism_id,
                    profile_digest: profile.profile_digest,
                    from_mass_milligrams,
                    transferred_mass_milligrams: profile.transfer_mass_milligrams,
                    to_mass_milligrams,
                });
            }
        }
        Ok(transfers)
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
        if instance.reservoir.is_some() {
            return Err(EngineError::MaterialReservoirCannotBeHeld(object_id));
        }
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

    fn next_material_surface_trace(
        &self,
        organism_id: EntityId,
        object_id: EntityId,
        applied_force_units: u16,
    ) -> Result<Option<(u32, u32)>, EngineError> {
        let instance = self
            .material_instances
            .get(&object_id)
            .ok_or(EngineError::UnknownMaterialInstance(object_id))?;
        if instance.held_by != Some(organism_id) {
            return Err(EngineError::MaterialInstanceNotHeldByActor(object_id));
        }
        let from = instance.surface_trace_units;
        let Some(to) = from
            .checked_add(u32::from(applied_force_units))
            .filter(|to| *to <= MAX_MATERIAL_SURFACE_TRACE_UNITS)
        else {
            return Ok(None);
        };
        Ok(Some((from, to)))
    }

    fn next_material_surface_region_trace(
        &self,
        organism_id: EntityId,
        object_id: EntityId,
        contact_region: u8,
        applied_force_units: u16,
    ) -> Result<Option<(u32, u32, u32, u32)>, EngineError> {
        let instance = self
            .material_instances
            .get(&object_id)
            .ok_or(EngineError::UnknownMaterialInstance(object_id))?;
        if instance.held_by != Some(organism_id) {
            return Err(EngineError::MaterialInstanceNotHeldByActor(object_id));
        }
        let region_index = usize::from(contact_region);
        if instance.surface_region_trace_units.len() != MATERIAL_SURFACE_REGION_COUNT
            || region_index >= MATERIAL_SURFACE_REGION_COUNT
        {
            return Err(EngineError::InvalidMaterialSurfaceRegions(object_id));
        }
        let from_region = instance.surface_region_trace_units[region_index];
        let from_total = instance.surface_trace_units;
        let force = u32::from(applied_force_units);
        let Some(to_region) = from_region
            .checked_add(force)
            .filter(|to| *to <= MAX_MATERIAL_SURFACE_TRACE_UNITS)
        else {
            return Ok(None);
        };
        let Some(to_total) = from_total
            .checked_add(force)
            .filter(|to| *to <= MAX_MATERIAL_SURFACE_TRACE_UNITS)
        else {
            return Ok(None);
        };
        Ok(Some((from_region, from_total, to_region, to_total)))
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
        self.manifest.validate()?;
        if self.manifest.ruleset_version == 0 {
            return Err(EngineError::ZeroRulesetVersion);
        }
        if self.uses_world_experiment_bootstrap()
            && self.manifest.ruleset_version < CANCER_RESEARCH_WORLD_RULESET_VERSION
        {
            return Err(EngineError::WorldExperimentRequiresNewerRuleset);
        }
        if self.status == WorldStatus::Initializing {
            if !self.initial_cancer_research_cohort.is_empty() || !self.cancer_burdens.is_empty() {
                return Err(EngineError::InvalidCancerResearchInitialCohort);
            }
        } else if self.uses_world_experiment_bootstrap() {
            if self.initial_cancer_research_cohort.len()
                != CANCER_RESEARCH_INITIAL_AFFECTED_RESIDENTS as usize
                || self
                    .initial_cancer_research_cohort
                    .iter()
                    .any(|resident_id| {
                        self.organisms
                            .get(resident_id)
                            .is_none_or(|organism| organism.role != OrganismRole::Person)
                    })
            {
                return Err(EngineError::InvalidCancerResearchInitialCohort);
            }
            if self.uses_cancer_biology_driver() {
                let target = match &self.manifest.experiment {
                    Some(WorldExperimentCommitment::CancerResearch(commitment)) => {
                        commitment.target
                    }
                    None => return Err(EngineError::InvalidCancerResearchBurden),
                };
                if self.cancer_burdens.len() != self.initial_cancer_research_cohort.len()
                    || self.cancer_burdens.keys().copied().collect::<BTreeSet<_>>()
                        != self.initial_cancer_research_cohort
                    || self
                        .cancer_burdens
                        .values()
                        .any(|burden| burden.target != target || burden.validate().is_err())
                {
                    return Err(EngineError::InvalidCancerResearchBurden);
                }
            } else if !self.cancer_burdens.is_empty() {
                return Err(EngineError::InvalidCancerResearchBurden);
            }
        } else if !self.initial_cancer_research_cohort.is_empty() || !self.cancer_burdens.is_empty()
        {
            return Err(EngineError::InvalidCancerResearchInitialCohort);
        }
        if let Some(configuration) = &self.configuration {
            configuration.validate()?;
        }
        if self.status == WorldStatus::Initializing
            && (self.tick != SimTick::ZERO
                || !self.organisms.is_empty()
                || !self.material_instances.is_empty()
                || !self.pending_reproductive_developments.is_empty())
        {
            return Err(EngineError::InvalidGenesisState);
        }
        if self.uses_reproductive_physiology_driver()
            && self.status != WorldStatus::Initializing
            && !self
                .organisms
                .values()
                .any(|organism| organism.role == OrganismRole::Person)
        {
            return Err(EngineError::MissingInitialPeople);
        }
        if matches!(self.status, WorldStatus::Extinct | WorldStatus::Archived)
            && self.living_people() != 0
        {
            return Err(EngineError::LivingPeopleRemain);
        }
        if self.uses_reproductive_physiology_driver()
            && (self.status == WorldStatus::Extinct
                || (self.status == WorldStatus::Running && self.living_people() == 0))
        {
            return Err(EngineError::UnarchivedWorldExtinction);
        }
        if self.uses_celestial_driver()
            && self.tick != SimTick::ZERO
            && self.celestial_tick != Some(self.tick)
        {
            return Err(EngineError::MissingCelestialState(self.tick));
        }
        if self.uses_bodily_regulation_driver() && self.status != WorldStatus::Initializing {
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
        if self.uses_local_weather_driver() && self.status != WorldStatus::Initializing {
            self.configuration
                .as_ref()
                .and_then(WorldConfiguration::local_weather_baseline)
                .ok_or(EngineError::MissingLocalWeather)?
                .validate()
                .map_err(|error| EngineError::InvalidLocalWeather(error.to_string()))?;
        }
        if self.uses_terrain_movement_driver() && self.status != WorldStatus::Initializing {
            let surface = self
                .configuration
                .as_ref()
                .and_then(WorldConfiguration::local_surface_baseline)
                .ok_or_else(|| {
                    EngineError::InvalidLocalSurface(
                        "terrain movement ruleset requires a local surface baseline".to_owned(),
                    )
                })?;
            surface
                .validate()
                .map_err(|error| EngineError::InvalidLocalSurface(error.to_string()))?;
            if self.uses_topsoil_movement_driver() {
                topsoil_adjusted_movement_exposure(1, &surface.topsoil_source_quantiles)
                    .map_err(|error| EngineError::InvalidLocalSurface(error.to_string()))?;
            }
        }
        let mut heritable_profiles_by_species = BTreeMap::new();
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
            match (
                self.uses_adult_body_mass_state_driver(),
                organism.adult_body_mass.as_ref(),
            ) {
                (true, Some(commitment)) => {
                    commitment
                        .validate()
                        .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                    if commitment.species != organism.species {
                        return Err(EngineError::InvalidEmbodiedEvent(
                            "adult-body-mass commitment species does not match organism".to_owned(),
                        ));
                    }
                }
                (true, None) => {
                    return Err(EngineError::InvalidEmbodiedEvent(format!(
                        "organism {} lacks adult-body-mass state",
                        organism.organism_id
                    )));
                }
                (false, Some(_)) => {
                    return Err(EngineError::InvalidEmbodiedEvent(
                        "adult-body-mass state requires ruleset 32".to_owned(),
                    ));
                }
                (false, None) => {}
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
            if self.uses_reproductive_physiology_driver() {
                let reproduction = organism.reproductive_physiology.as_ref().ok_or(
                    EngineError::MissingReproductiveCommitment(organism.organism_id),
                )?;
                reproduction
                    .validate()
                    .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                if reproduction.species != organism.species
                    || !reproduction.supports_category(&organism.birth_category)
                    || self.configuration.as_ref().is_none_or(|world| {
                        world.tick_duration_seconds != reproduction.tick_duration_seconds
                    })
                    || organism
                        .reproductive_available_at
                        .is_some_and(|available_at| available_at < organism.initialized_at)
                {
                    return Err(EngineError::InvalidReproductiveCommitment(
                        organism.organism_id,
                    ));
                }
            } else if organism.reproductive_physiology.is_some()
                || organism.reproductive_available_at.is_some()
            {
                return Err(EngineError::ReproductivePhysiologyUnsupported);
            }
            if self.uses_heritable_disposition_driver() {
                let profile = organism.heritable_disposition_profile.as_ref().ok_or(
                    EngineError::MissingHeritableDispositionProfile(organism.organism_id),
                )?;
                let disposition = organism
                    .heritable_disposition
                    .as_ref()
                    .ok_or(EngineError::InvalidHeritableDisposition)?;
                if profile.species != organism.species
                    || disposition.validate_against(profile).is_err()
                {
                    return Err(EngineError::InvalidHeritableDisposition);
                }
                let species_key = (
                    organism.species.catalog.clone(),
                    organism.species.identifier.clone(),
                    organism.species.scientific_name.clone(),
                    organism.species.source_url.clone(),
                );
                let profile_fingerprint = Digest::canonical(profile)?;
                if heritable_profiles_by_species
                    .insert(species_key, profile_fingerprint)
                    .is_some_and(|previous| previous != profile_fingerprint)
                {
                    return Err(EngineError::InvalidHeritableDisposition);
                }
                let expected = match (organism.born_at, organism.parent_ids.as_slice()) {
                    (None, []) if organism.initialized_at == SimTick::ZERO => {
                        self.founder_heritable_disposition(organism.organism_id, profile)?
                    }
                    (Some(born_at), [_, _]) if organism.initialized_at == born_at => {
                        let development_ticks = organism
                            .reproductive_physiology
                            .as_ref()
                            .ok_or(EngineError::MissingReproductiveCommitment(
                                organism.organism_id,
                            ))?
                            .development_ticks;
                        let derived_at = SimTick::new(
                            born_at
                                .get()
                                .checked_sub(development_ticks)
                                .ok_or(EngineError::InvalidHeritableDisposition)?,
                        );
                        self.offspring_heritable_disposition(
                            organism.organism_id,
                            &organism.parent_ids,
                            derived_at,
                            profile,
                        )?
                    }
                    _ => return Err(EngineError::InvalidHeritableDisposition),
                };
                if disposition != &expected {
                    return Err(EngineError::InvalidHeritableDisposition);
                }
            } else if organism.heritable_disposition_profile.is_some()
                || organism.heritable_disposition.is_some()
            {
                return Err(EngineError::HeritableDispositionUnsupported);
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
            if self.uses_movement_direction_learning_driver() {
                if organism.movement_direction_values.len() > 4
                    || organism
                        .movement_direction_values
                        .windows(2)
                        .any(|pair| pair[0].movement_direction >= pair[1].movement_direction)
                    || organism
                        .movement_direction_values
                        .iter()
                        .any(|entry| entry.validate().is_err())
                    || organism
                        .movement_direction_values_updated_at
                        .is_some_and(|updated_at| updated_at > self.tick)
                {
                    return Err(EngineError::InvalidMovementDirectionValueState(
                        organism.organism_id,
                    ));
                }
            } else if !organism.movement_direction_values.is_empty()
                || organism.movement_direction_values_updated_at.is_some()
            {
                return Err(EngineError::MovementDirectionLearningUnsupported);
            }
            if self.uses_social_learning_driver() {
                if organism
                    .social_action_values
                    .windows(2)
                    .any(|pair| pair[0].action_kind >= pair[1].action_kind)
                    || organism
                        .social_action_values
                        .iter()
                        .any(|entry| entry.validate().is_err() || entry.value <= 0)
                    || organism
                        .social_action_values_updated_at
                        .is_some_and(|updated_at| updated_at > self.tick)
                {
                    return Err(EngineError::InvalidSocialActionValueState(
                        organism.organism_id,
                    ));
                }
            } else if !organism.social_action_values.is_empty()
                || organism.social_action_values_updated_at.is_some()
            {
                return Err(EngineError::SocialLearningUnsupported);
            }
            if self.uses_signal_action_association_driver() {
                let maximum = if self.uses_signal_motor_association_driver() {
                    MAX_SIGNAL_MOTOR_ASSOCIATIONS
                } else {
                    MAX_SIGNAL_ACTION_ASSOCIATIONS
                };
                let expected_schema = if self.uses_competitive_signal_learning_driver() {
                    COMPETITIVE_SIGNAL_ASSOCIATION_SCHEMA_VERSION
                } else if self.uses_signal_motor_association_driver() {
                    SIGNAL_MOTOR_ASSOCIATION_SCHEMA_VERSION
                } else {
                    SIGNAL_ACTION_ASSOCIATION_SCHEMA_VERSION
                };
                if organism.signal_action_associations.len() > maximum
                    || organism.signal_action_associations.windows(2).any(|pair| {
                        (
                            pair[0].signal_intensity,
                            pair[0].action_kind,
                            pair[0].movement_direction,
                        ) >= (
                            pair[1].signal_intensity,
                            pair[1].action_kind,
                            pair[1].movement_direction,
                        )
                    })
                    || organism.signal_action_associations.iter().any(|entry| {
                        entry.validate().is_err()
                            || entry.association_schema_version != expected_schema
                    })
                    || organism
                        .signal_action_associations_updated_at
                        .is_some_and(|updated_at| updated_at > self.tick)
                {
                    return Err(EngineError::InvalidSignalActionAssociation(
                        organism.organism_id,
                    ));
                }
            } else if !organism.signal_action_associations.is_empty()
                || organism.signal_action_associations_updated_at.is_some()
            {
                return Err(EngineError::SignalActionAssociationUnsupported);
            }
            if organism
                .parent_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            {
                return Err(EngineError::NonCanonicalParentOrder);
            }
        }
        if self.uses_cognition_driver() {
            if self.status != WorldStatus::Running && !self.pending_cognition_requests.is_empty() {
                return Err(EngineError::InvalidCognitionInput(
                    "a non-running world retained a pending cognition request".to_owned(),
                ));
            }
            for (request_id, selection) in &self.pending_cognition_requests {
                selection
                    .validate()
                    .map_err(|error| EngineError::InvalidCognitionSelection(error.to_string()))?;
                if request_id != &selection.request_id
                    || selection.world_id != self.world_id()
                    || selection.selected_at_tick > self.tick
                    || selection.deadline_tick <= self.tick
                    || self
                        .organisms
                        .get(&selection.organism_id)
                        .is_none_or(|organism| !organism.is_alive())
                {
                    return Err(EngineError::InvalidCognitionSelection(
                        "pending cognition request disagrees with canonical state".to_owned(),
                    ));
                }
            }
        } else if !self.pending_cognition_requests.is_empty() {
            return Err(EngineError::CognitionUnsupported);
        }
        if !self.uses_reproductive_physiology_driver()
            && !self.pending_reproductive_developments.is_empty()
        {
            return Err(EngineError::ReproductivePhysiologyUnsupported);
        }
        let mut pending_parents = BTreeSet::new();
        for (development_id, pending) in &self.pending_reproductive_developments {
            if pending.parent_ids.len() != 2 {
                return Err(EngineError::InvalidReproductiveDevelopment(*development_id));
            }
            let developing_parent = self
                .organisms
                .get(&pending.developing_parent_id)
                .ok_or(EngineError::UnknownParent(pending.developing_parent_id))?;
            let profile = developing_parent.reproductive_physiology.as_ref().ok_or(
                EngineError::MissingReproductiveCommitment(pending.developing_parent_id),
            )?;
            let left = self
                .organisms
                .get(&pending.parent_ids[0])
                .ok_or(EngineError::UnknownParent(pending.parent_ids[0]))?;
            let right = self
                .organisms
                .get(&pending.parent_ids[1])
                .ok_or(EngineError::UnknownParent(pending.parent_ids[1]))?;
            let heredity_valid = if self.uses_heritable_disposition_driver() {
                let heritable_profile = left.heritable_disposition_profile.as_ref().ok_or(
                    EngineError::MissingHeritableDispositionProfile(left.organism_id),
                )?;
                right.heritable_disposition_profile.as_ref() == Some(heritable_profile)
                    && pending.heritable_disposition_profile.as_ref() == Some(heritable_profile)
                    && pending.offspring_heritable_disposition.as_ref()
                        == Some(&self.offspring_heritable_disposition(
                            pending.offspring_id,
                            &pending.parent_ids,
                            pending.started_at,
                            heritable_profile,
                        )?)
            } else {
                pending.heritable_disposition_profile.is_none()
                    && pending.offspring_heritable_disposition.is_none()
            };
            let expected_due = pending
                .started_at
                .get()
                .checked_add(profile.development_ticks)
                .ok_or(EngineError::ReproductiveArithmetic)?;
            let expected_available = expected_due
                .checked_add(profile.recovery_ticks)
                .ok_or(EngineError::ReproductiveArithmetic)?;
            let expected_development_id = EntityId::deterministic(
                self.world_id(),
                self.reproductive_draw(
                    "development-identity",
                    pending.started_at,
                    &pending.parent_ids,
                )?
                .as_bytes(),
            );
            let expected_offspring_id = EntityId::deterministic(
                self.world_id(),
                self.reproductive_draw(
                    "offspring-identity",
                    pending.started_at,
                    &pending.parent_ids,
                )?
                .as_bytes(),
            );
            if development_id != &pending.development_id
                || pending.development_id == pending.offspring_id
                || self.organisms.contains_key(&pending.offspring_id)
                || pending.parent_ids.len() != 2
                || !heredity_valid
                || pending.parent_ids.windows(2).any(|pair| pair[0] >= pair[1])
                || !pending.parent_ids.contains(&pending.developing_parent_id)
                || pending.started_at == SimTick::ZERO
                || pending.started_at > self.tick
                || pending.due_tick <= self.tick
                || pending.parents_available_at <= pending.due_tick
                || !developing_parent.is_alive()
                || developing_parent.species != pending.species
                || developing_parent.role != pending.role
                || profile.profile_digest != pending.profile_digest
                || left.reproductive_physiology.as_ref() != Some(profile)
                || right.reproductive_physiology.as_ref() != Some(profile)
                || Self::reproductive_category_developer(profile, left, right)
                    != Some(pending.developing_parent_id)
                || pending.due_tick != SimTick::new(expected_due)
                || pending.parents_available_at != SimTick::new(expected_available)
                || pending.development_id != expected_development_id
                || pending.offspring_id != expected_offspring_id
                || self.offspring_category_at(profile, &pending.parent_ids, pending.started_at)?
                    != pending.birth_category
                || !self.reproductive_opportunity_succeeds_at(
                    profile,
                    &pending.parent_ids,
                    pending.started_at,
                )?
                || pending.parent_ids.iter().any(|parent_id| {
                    !pending_parents.insert(*parent_id)
                        || self.organisms.get(parent_id).is_none_or(|parent| {
                            parent.species != pending.species
                                || parent.role != pending.role
                                || parent.reproductive_available_at
                                    != Some(pending.parents_available_at)
                        })
                })
            {
                return Err(EngineError::InvalidReproductiveDevelopment(*development_id));
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
            if (!self.uses_material_surface_trace_driver() && instance.surface_trace_units != 0)
                || instance.surface_trace_units > MAX_MATERIAL_SURFACE_TRACE_UNITS
            {
                return Err(EngineError::InvalidMaterialSurfaceTrace(*id));
            }
            if self.uses_material_surface_regions_driver() {
                let total = instance
                    .surface_region_trace_units
                    .iter()
                    .try_fold(0_u32, |total, region| total.checked_add(*region))
                    .ok_or(EngineError::InvalidMaterialSurfaceRegions(*id))?;
                if instance.surface_region_trace_units.len() != MATERIAL_SURFACE_REGION_COUNT
                    || instance
                        .surface_region_trace_units
                        .iter()
                        .any(|region| *region > MAX_MATERIAL_SURFACE_TRACE_UNITS)
                    || total != instance.surface_trace_units
                {
                    return Err(EngineError::InvalidMaterialSurfaceRegions(*id));
                }
            } else if !instance.surface_region_trace_units.is_empty() {
                return Err(EngineError::InvalidMaterialSurfaceRegions(*id));
            }
            match (&instance.reservoir, instance.reservoir_settled_at) {
                (None, None) => {}
                (Some(reservoir), Some(settled_at)) => {
                    reservoir
                        .validate()
                        .map_err(|error| EngineError::InvalidEmbodiedEvent(error.to_string()))?;
                    let remaining_mass = instance
                        .remaining_mass_milligrams
                        .ok_or(EngineError::MissingMaterialMass(*id))?;
                    if !self.uses_material_reservoir_driver()
                        || reservoir.material != instance.material
                        || !reservoir.coverage_patch.contains(instance.embodied_patch)
                        || remaining_mass > reservoir.maximum_mass_milligrams
                        || instance.held_by.is_some()
                        || settled_at > self.tick
                        || instance.oral_transfer_profiles.is_empty()
                    {
                        return Err(EngineError::InvalidMaterialReservoir(*id));
                    }
                }
                _ => return Err(EngineError::InvalidMaterialReservoir(*id)),
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
        if self.uses_material_reservoir_driver()
            && self.status != WorldStatus::Initializing
            && !self
                .material_instances
                .values()
                .any(|instance| instance.reservoir.is_some())
        {
            return Err(EngineError::MissingInitialMaterialReservoir);
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pending_reproductive_developments: Vec<&'a PendingReproductiveDevelopment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pending_cognition_requests: Vec<&'a CognitionRequestSelection>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    initial_cancer_research_cohort: Vec<EntityId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cancer_burdens: Vec<&'a CancerBurdenState>,
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
        let snapshot_schema_version = if state.uses_cancer_biology_driver() {
            CANCER_BURDEN_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_world_experiment_bootstrap() {
            CANCER_RESEARCH_COHORT_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_adult_body_mass_state_driver() {
            ADULT_BODY_MASS_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_mass_scaled_metabolism_driver() {
            MASS_SCALED_METABOLISM_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_topsoil_movement_driver() {
            TOPSOIL_MOVEMENT_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_terrain_movement_driver() {
            TERRAIN_MOVEMENT_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_local_atmospheric_flux_driver() {
            LOCAL_ATMOSPHERIC_FLUX_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_local_weather_driver() {
            LOCAL_WEATHER_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_signal_motor_association_driver() {
            SIGNAL_MOTOR_ASSOCIATION_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_movement_direction_learning_driver() {
            MOVEMENT_DIRECTION_LEARNING_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_signal_action_association_driver() {
            SIGNAL_ACTION_ASSOCIATION_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_material_surface_regions_driver() {
            MATERIAL_SURFACE_REGIONS_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_material_surface_trace_driver() {
            MATERIAL_SURFACE_TRACE_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_social_learning_driver() {
            SOCIAL_LEARNING_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_material_reservoir_driver() {
            MATERIAL_RESERVOIR_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_cognition_driver() {
            COGNITION_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_heritable_disposition_driver() {
            HERITABLE_DISPOSITION_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_reproductive_physiology_driver() {
            REPRODUCTIVE_PHYSIOLOGY_SNAPSHOT_SCHEMA_VERSION
        } else if state.uses_action_learning_driver() {
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
                | REPRODUCTIVE_PHYSIOLOGY_SNAPSHOT_SCHEMA_VERSION
                | HERITABLE_DISPOSITION_SNAPSHOT_SCHEMA_VERSION
                | COGNITION_SNAPSHOT_SCHEMA_VERSION
                | MATERIAL_RESERVOIR_SNAPSHOT_SCHEMA_VERSION
                | SOCIAL_LEARNING_SNAPSHOT_SCHEMA_VERSION
                | MATERIAL_SURFACE_TRACE_SNAPSHOT_SCHEMA_VERSION
                | MATERIAL_SURFACE_REGIONS_SNAPSHOT_SCHEMA_VERSION
                | SIGNAL_ACTION_ASSOCIATION_SNAPSHOT_SCHEMA_VERSION
                | MOVEMENT_DIRECTION_LEARNING_SNAPSHOT_SCHEMA_VERSION
                | SIGNAL_MOTOR_ASSOCIATION_SNAPSHOT_SCHEMA_VERSION
                | LOCAL_WEATHER_SNAPSHOT_SCHEMA_VERSION
                | LOCAL_ATMOSPHERIC_FLUX_SNAPSHOT_SCHEMA_VERSION
                | TERRAIN_MOVEMENT_SNAPSHOT_SCHEMA_VERSION
                | TOPSOIL_MOVEMENT_SNAPSHOT_SCHEMA_VERSION
                | MASS_SCALED_METABOLISM_SNAPSHOT_SCHEMA_VERSION
                | ADULT_BODY_MASS_SNAPSHOT_SCHEMA_VERSION
                | WORLD_EXPERIMENT_SNAPSHOT_SCHEMA_VERSION
                | CANCER_RESEARCH_COHORT_SNAPSHOT_SCHEMA_VERSION
                | CANCER_BURDEN_SNAPSHOT_SCHEMA_VERSION
        ) {
            return Err(EngineError::UnsupportedSnapshotSchema(
                self.snapshot_schema_version,
            ));
        }
        let expected_schema_version = if self.state.uses_cancer_biology_driver() {
            CANCER_BURDEN_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_world_experiment_bootstrap() {
            CANCER_RESEARCH_COHORT_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_adult_body_mass_state_driver() {
            ADULT_BODY_MASS_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_mass_scaled_metabolism_driver() {
            MASS_SCALED_METABOLISM_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_topsoil_movement_driver() {
            TOPSOIL_MOVEMENT_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_terrain_movement_driver() {
            TERRAIN_MOVEMENT_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_local_atmospheric_flux_driver() {
            LOCAL_ATMOSPHERIC_FLUX_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_local_weather_driver() {
            LOCAL_WEATHER_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_signal_motor_association_driver() {
            SIGNAL_MOTOR_ASSOCIATION_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_movement_direction_learning_driver() {
            MOVEMENT_DIRECTION_LEARNING_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_signal_action_association_driver() {
            SIGNAL_ACTION_ASSOCIATION_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_material_surface_regions_driver() {
            MATERIAL_SURFACE_REGIONS_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_material_surface_trace_driver() {
            MATERIAL_SURFACE_TRACE_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_social_learning_driver() {
            SOCIAL_LEARNING_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_material_reservoir_driver() {
            MATERIAL_RESERVOIR_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_cognition_driver() {
            COGNITION_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_heritable_disposition_driver() {
            HERITABLE_DISPOSITION_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_reproductive_physiology_driver() {
            REPRODUCTIVE_PHYSIOLOGY_SNAPSHOT_SCHEMA_VERSION
        } else if self.state.uses_action_learning_driver() {
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

fn latest_ruleset_event_schema_for_replay(state: &EngineState) -> Option<u16> {
    if state.uses_cancer_biology_driver() {
        Some(CANCER_BURDEN_EVENT_SCHEMA_VERSION)
    } else if state.uses_world_experiment_bootstrap() {
        Some(CANCER_RESEARCH_COHORT_EVENT_SCHEMA_VERSION)
    } else if state.uses_competitive_signal_learning_driver() {
        Some(COMPETITIVE_SIGNAL_LEARNING_EVENT_SCHEMA_VERSION)
    } else if state.uses_adult_body_mass_state_driver() {
        Some(ADULT_BODY_MASS_EVENT_SCHEMA_VERSION)
    } else if state.uses_mass_scaled_metabolism_driver() {
        Some(MASS_SCALED_METABOLISM_EVENT_SCHEMA_VERSION)
    } else if state.uses_topsoil_movement_driver() {
        Some(TOPSOIL_MOVEMENT_EVENT_SCHEMA_VERSION)
    } else if state.uses_terrain_movement_driver() {
        Some(TERRAIN_MOVEMENT_EVENT_SCHEMA_VERSION)
    } else if state.uses_local_atmospheric_flux_driver() {
        Some(LOCAL_ATMOSPHERIC_FLUX_EVENT_SCHEMA_VERSION)
    } else {
        None
    }
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
        let expected_schema = if let Some(schema) = latest_ruleset_event_schema_for_replay(&state) {
            schema
        } else if state.uses_local_weather_driver() {
            LOCAL_WEATHER_EVENT_SCHEMA_VERSION
        } else if state.uses_signal_motor_association_driver() {
            SIGNAL_MOTOR_ASSOCIATION_EVENT_SCHEMA_VERSION
        } else if state.uses_movement_direction_learning_driver() {
            MOVEMENT_DIRECTION_LEARNING_EVENT_SCHEMA_VERSION
        } else if state.uses_selectable_movement_driver() {
            SELECTABLE_MOVEMENT_EVENT_SCHEMA_VERSION
        } else if state.uses_signal_action_association_driver() {
            SIGNAL_ACTION_ASSOCIATION_EVENT_SCHEMA_VERSION
        } else if state.uses_material_surface_regions_driver() {
            MATERIAL_SURFACE_REGIONS_EVENT_SCHEMA_VERSION
        } else if state.uses_material_surface_trace_driver() {
            MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION
        } else if state.uses_social_learning_driver() {
            SOCIAL_LEARNING_EVENT_SCHEMA_VERSION
        } else if state.uses_material_reservoir_driver() {
            MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION
        } else if state.uses_cognition_driver() {
            COGNITION_EVENT_SCHEMA_VERSION
        } else if state.uses_heritable_disposition_driver() {
            HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION
        } else if state.uses_reproductive_physiology_driver() {
            REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION
        } else if state.uses_action_learning_driver() {
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
        let valid_schema = if expected_schema == EVENT_SCHEMA_VERSION {
            matches!(
                batch.event_schema_version,
                CONFIGURED_EVENT_SCHEMA_VERSION | EVENT_SCHEMA_VERSION
            )
        } else {
            batch.event_schema_version == expected_schema
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
    #[error(
        "Cancer World requires exactly 1,000 initial residents and a seed-selected 500-person affected cohort"
    )]
    InvalidCancerResearchInitialCohort,
    #[error("Cancer World burden state or progression is not the exact deterministic transition")]
    InvalidCancerResearchBurden,
    #[error("initial material list contains a duplicate identity")]
    DuplicateInitialMaterial,
    #[error("a ruleset-fourteen genesis requires at least one person")]
    MissingInitialPeople,
    #[error("organisms may be initialized only in the atomic world-start batch")]
    OrganismInitializationOutsideGenesis,
    #[error("world extinction and archival must be the exact terminal lifecycle suffix")]
    InvalidWorldLifecycleEventSet,
    #[error("a ruleset-fourteen tick transition requires one TickAdvanced event at index zero")]
    InvalidTickAdvanceEventSet,
    #[error("a ruleset-fourteen world without living people must be archived")]
    UnarchivedWorldExtinction,
    #[error("organism {0} is missing its species-bound heritable-disposition profile")]
    MissingHeritableDispositionProfile(EntityId),
    #[error("heritable-disposition data is invalid or does not match its deterministic derivation")]
    InvalidHeritableDisposition,
    #[error("heritable-disposition arithmetic overflowed")]
    HeritableDispositionArithmetic,
    #[error("heritable-disposition data is not supported by this ruleset")]
    HeritableDispositionUnsupported,
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
    #[error("ruleset seventeen requires at least one initial material reservoir")]
    MissingInitialMaterialReservoir,
    #[error("material-reservoir mechanics are unsupported by this ruleset")]
    MaterialReservoirUnsupported,
    #[error("material reservoir {0} is invalid")]
    InvalidMaterialReservoir(EntityId),
    #[error("material reservoir {0} cannot be held")]
    MaterialReservoirCannotBeHeld(EntityId),
    #[error("material surface traces are unsupported by this ruleset")]
    MaterialSurfaceTraceUnsupported,
    #[error("material instance {0} has an invalid surface-trace transition")]
    InvalidMaterialSurfaceTrace(EntityId),
    #[error("material surface-trace events do not exactly match primitive force and perception")]
    InvalidMaterialSurfaceTraceEventSet,
    #[error("material surface regions are unsupported by this ruleset")]
    MaterialSurfaceRegionsUnsupported,
    #[error("material instance {0} has invalid surface-region state or arithmetic")]
    InvalidMaterialSurfaceRegions(EntityId),
    #[error("a ruleset-twenty held-object force action requires a contact region")]
    MissingSurfaceContactRegion,
    #[error("a ruleset-twenty-three move action requires a movement direction")]
    MissingMovementDirection,
    #[error("selectable movement actions and relocations do not exactly agree")]
    InvalidSelectableMovementEventSet,
    #[error("material surface-region events do not exactly match primitive force and perception")]
    InvalidMaterialSurfaceRegionEventSet,
    #[error("material-reservoir transfer events do not match ordered action resolution")]
    InvalidMaterialReservoirEventSet,
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
    #[error("local-weather ruleset requires a tick-zero source-bound weather baseline")]
    MissingLocalWeather,
    #[error("invalid local-weather baseline: {0}")]
    InvalidLocalWeather(String),
    #[error("invalid local-surface baseline: {0}")]
    InvalidLocalSurface(String),
    #[error("local atmospheric flux is unsupported by this ruleset")]
    LocalAtmosphericFluxUnsupported,
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
    #[error("movement-direction learning is unsupported by this ruleset")]
    MovementDirectionLearningUnsupported,
    #[error("organism {0} has an invalid movement-direction value state")]
    InvalidMovementDirectionValueState(EntityId),
    #[error("organism {0} has an invalid movement-direction value transition")]
    InvalidMovementDirectionValueTransition(EntityId),
    #[error("organism {0} movement-direction observation count overflowed")]
    MovementDirectionValueObservationOverflow(EntityId),
    #[error("social learning is unsupported by this ruleset")]
    SocialLearningUnsupported,
    #[error("organism {0} has an invalid social-action-value state")]
    InvalidSocialActionValueState(EntityId),
    #[error("organism {0} has an invalid social-action-value transition")]
    InvalidSocialActionValueTransition(EntityId),
    #[error("signal-action association is unsupported by this ruleset")]
    SignalActionAssociationUnsupported,
    #[error("organism {0} has an invalid signal-action association")]
    InvalidSignalActionAssociation(EntityId),
    #[error("external cognition is unsupported by this ruleset")]
    CognitionUnsupported,
    #[error("cognition request selection is invalid: {0}")]
    InvalidCognitionSelection(String),
    #[error("a world-total cognition request is already pending")]
    CognitionRequestAlreadyPending,
    #[error("cognition request {0} was selected more than once")]
    DuplicateCognitionRequest(Uuid),
    #[error("a fixed-deadline cognition input is required before this tick can advance")]
    CognitionInputRequired,
    #[error("no cognition input is due for this tick")]
    UnexpectedCognitionInput,
    #[error("cognition deadline input is invalid: {0}")]
    InvalidCognitionInput(String),
    #[error("cognition request {0} is not pending")]
    UnknownCognitionRequest(Uuid),
    #[error("reproductive physiology is unsupported by this ruleset")]
    ReproductivePhysiologyUnsupported,
    #[error("organism {0} lacks its species-bound reproductive commitment")]
    MissingReproductiveCommitment(EntityId),
    #[error("organism {0} has an invalid reproductive commitment")]
    InvalidReproductiveCommitment(EntityId),
    #[error("reproductive arithmetic overflowed")]
    ReproductiveArithmetic,
    #[error("deterministic reproductive identity collided")]
    ReproductiveIdentityCollision,
    #[error("reproductive development {0} is invalid")]
    InvalidReproductiveDevelopment(EntityId),
    #[error("reproductive development {0} is unknown")]
    UnknownReproductiveDevelopment(EntityId),
    #[error("reproductive development {0} passed its due tick")]
    OverdueReproductiveDevelopment(EntityId),
    #[error("birth {0} is not bound to a pending reproductive development")]
    UnboundReproductiveBirth(EntityId),
    #[error("birth {0} does not exactly resolve its pending reproductive development")]
    InvalidReproductiveBirth(EntityId),
    #[error("the tick's reproductive event set is missing, reordered, or fabricated")]
    InvalidReproductiveEventSet,
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
    #[error("an experiment bootstrap requires ruleset thirty-seven or later")]
    WorldExperimentRequiresNewerRuleset,
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
    WorldManifest(#[from] WorldManifestError),
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
        ProvisionalLocalSurfaceBaseline, ProvisionalLocalWeatherBaseline,
        ProvisionalWorldCompositionReference, S2Projection, SchedulerKind, SituatedPerception,
        SpatialGrid, TdbSecondsSinceJ2000, WorldDataBundleReference, WorldSeed, WorldStatus,
    };

    #[test]
    fn signal_convention_reuse_is_neutral_weighting_with_a_fixed_live_boundary() {
        assert!(!signal_convention_reuse_active(
            CLOSE_KIN_EXCLUSION_RULESET_VERSION,
            SimTick::new(RULESET_33_SIGNAL_CONVENTION_ACTIVATION_TICK - 1),
        ));
        assert!(signal_convention_reuse_active(
            CLOSE_KIN_EXCLUSION_RULESET_VERSION,
            SimTick::new(RULESET_33_SIGNAL_CONVENTION_ACTIVATION_TICK),
        ));
        assert!(signal_convention_reuse_active(
            SIGNAL_CONVENTION_REUSE_RULESET_VERSION,
            SimTick::ZERO,
        ));

        let association = SignalActionAssociationState {
            association_schema_version: SIGNAL_MOTOR_ASSOCIATION_SCHEMA_VERSION,
            signal_intensity: 7,
            action_kind: PrimitiveActionKind::Move,
            movement_direction: Some(2),
            observations: 8,
            value: 32,
        };
        assert_eq!(signal_convention_candidate_weight(2, 7, None, 6, None), 2);
        assert_eq!(
            signal_convention_candidate_weight(2, 7, Some(7), 0, None),
            2 + SIGNAL_IMITATION_WEIGHT_BONUS
        );
        assert_eq!(
            signal_convention_candidate_weight(2, 7, None, 6, Some(association)),
            14
        );
        assert_eq!(
            signal_convention_candidate_weight(2, 7, Some(7), 6, Some(association)),
            30
        );
        assert_eq!(
            signal_convention_candidate_weight(2, 8, Some(7), 6, Some(association)),
            2
        );

        assert!(!local_interaction_active(
            CLOSE_KIN_EXCLUSION_RULESET_VERSION,
            SimTick::new(RULESET_33_LOCAL_INTERACTION_ACTIVATION_TICK - 1),
        ));
        assert!(local_interaction_active(
            CLOSE_KIN_EXCLUSION_RULESET_VERSION,
            SimTick::new(RULESET_33_LOCAL_INTERACTION_ACTIVATION_TICK),
        ));
        assert!(local_interaction_active(
            LOCAL_INTERACTION_RULESET_VERSION,
            SimTick::ZERO,
        ));

        let patch: S2CellId = "0000000000004000".parse().expect("L23 patch");
        let target = s2_edge_neighbors(patch).expect("neighbor patches")[0];
        let mut movement = (0..4_u8)
            .map(|direction| PolicyCandidate {
                action: PrimitiveAction {
                    kind: PrimitiveActionKind::Move,
                    target_id: None,
                    intensity: 1,
                    contact_region: None,
                    movement_direction: Some(direction),
                },
                weight: 2,
            })
            .collect::<Vec<_>>();
        apply_local_cohesion_weights(patch, target, &mut movement).expect("cohesion weighting");
        assert_eq!(movement[0].weight, 2 + LOCAL_COHESION_WEIGHT_BONUS);
        assert!(movement[1..].iter().all(|candidate| candidate.weight == 2));
    }

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
            adult_body_mass: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
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

    fn weather_provisional_full_earth_configuration() -> WorldConfiguration {
        let environmental = environmental_provisional_full_earth_configuration();
        let environment = environmental
            .local_environment_baseline()
            .expect("environment")
            .clone();
        WorldConfiguration::new_provisional_full_earth_with_weather_baseline(
            environmental.tick_duration_seconds,
            environmental.full_earth_grid().expect("grid").clone(),
            environmental
                .provisional_world_composition()
                .expect("composition")
                .clone(),
            environmental
                .partitioned_execution()
                .expect("execution")
                .clone(),
            environment.clone(),
            ProvisionalLocalWeatherBaseline {
                status: "provisional-weather-input-not-scientifically-admitted".to_owned(),
                source_normals_digest: Digest::sha256(b"fixed point ERA5 normals"),
                evidence_patch: environment.evidence_patch,
                active_patch: environment.active_patch,
                air_temperature_unit: "degC".to_owned(),
                air_temperature_decimal_places: 3,
                air_temperature_normal_minimum: [10_000; 12],
                air_temperature_normal_mean: [15_000; 12],
                air_temperature_normal_maximum: [20_000; 12],
                precipitation_unit: "m".to_owned(),
                precipitation_decimal_places: 6,
                precipitation_normal_mean: [1_000; 12],
                eastward_wind_unit: "m/s".to_owned(),
                eastward_wind_decimal_places: 3,
                eastward_wind_normal_mean: [500; 12],
                northward_wind_unit: "m/s".to_owned(),
                northward_wind_decimal_places: 3,
                northward_wind_normal_mean: [-500; 12],
            },
        )
        .expect("weather provisional config")
    }

    fn surface_provisional_full_earth_configuration() -> WorldConfiguration {
        let weather_configuration = weather_provisional_full_earth_configuration();
        let environment = weather_configuration
            .local_environment_baseline()
            .expect("environment")
            .clone();
        let weather = weather_configuration
            .local_weather_baseline()
            .expect("weather")
            .clone();
        WorldConfiguration::new_provisional_full_earth_with_surface_baseline(
            weather_configuration.tick_duration_seconds,
            weather_configuration
                .full_earth_grid()
                .expect("grid")
                .clone(),
            weather_configuration
                .provisional_world_composition()
                .expect("composition")
                .clone(),
            weather_configuration
                .partitioned_execution()
                .expect("execution")
                .clone(),
            environment.clone(),
            weather,
            ProvisionalLocalSurfaceBaseline {
                status: "provisional-surface-input-not-scientifically-admitted".to_owned(),
                source_evidence_digest: environment.source_evidence_digest,
                evidence_patch: environment.evidence_patch,
                active_patch: environment.active_patch,
                terrain_minimum_millimetres: 2_228_633,
                terrain_mean_millimetres: 2_296_048,
                terrain_maximum_millimetres: 2_364_719,
                surface_water_occurrence_source_code: 0,
                topsoil_source_quantiles: [[1, 2, 3]; 9],
            },
        )
        .expect("surface provisional config")
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
            evidence_basis: world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption,
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

    fn adult_body_mass_fixture(species: SpeciesIdentity) -> AdultBodyMassCommitment {
        AdultBodyMassCommitment {
            commitment_schema_version: world_domain::ADULT_BODY_MASS_COMMITMENT_SCHEMA_VERSION,
            species,
            evidence_basis: world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption,
            profile_set_digest: Digest::sha256(b"adult body mass fixture set"),
            source_record_id: "adult-mass-fixture-row".to_owned(),
            source_record_digest: Digest::sha256(b"adult body mass fixture row"),
            mass_grams_value: 70_000,
            mass_grams_decimal_places: 0,
        }
    }

    #[test]
    fn ruleset_twenty_seven_temperature_is_seeded_bounded_and_replay_stable() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x127));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(0x005e_ed27),
            LOCAL_WEATHER_RULESET_VERSION,
        );
        let configuration = weather_provisional_full_earth_configuration();
        let mut first = EngineState::new(manifest.clone());
        let mut second = EngineState::new(manifest);
        let mut readings = Vec::new();
        for tick in 0..=288 {
            first.tick = SimTick::new(tick);
            second.tick = SimTick::new(tick);
            let first_reading = first
                .local_temperature_at_tick(&configuration)
                .expect("first temperature");
            let second_reading = second
                .local_temperature_at_tick(&configuration)
                .expect("replayed temperature");
            assert_eq!(first_reading, second_reading);
            assert_eq!(first_reading.1, 3);
            assert!((10_000..=20_000).contains(&first_reading.0));
            readings.push(first_reading.0);
        }
        assert!(
            readings
                .windows(2)
                .all(|pair| pair[0].abs_diff(pair[1]) <= 35)
        );

        let snapshot = Snapshot::new(
            EngineState::new(WorldManifest::new(
                world_id,
                WorldSeed::new(0x005e_ed27),
                LOCAL_WEATHER_RULESET_VERSION,
            )),
            EventSequence::ZERO,
            Digest::ZERO,
        )
        .expect("weather schema snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            LOCAL_WEATHER_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("weather snapshot integrity");
    }

    #[test]
    fn ruleset_twenty_eight_flux_is_source_bound_conservative_and_replay_stable() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x128));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(0x005e_ed28),
            LOCAL_ATMOSPHERIC_FLUX_RULESET_VERSION,
        );
        let configuration = weather_provisional_full_earth_configuration();
        let mut first = EngineState::new(manifest.clone());
        let mut second = EngineState::new(manifest);

        first.tick = SimTick::ZERO;
        second.tick = SimTick::ZERO;
        let first_day = first
            .local_atmospheric_flux_at_tick(&configuration)
            .expect("first-day flux");
        assert_eq!(
            first_day,
            second
                .local_atmospheric_flux_at_tick(&configuration)
                .expect("replayed first-day flux")
        );
        first.tick = SimTick::new(288);
        second.tick = SimTick::new(288);
        let second_day = first
            .local_atmospheric_flux_at_tick(&configuration)
            .expect("second-day flux");
        assert_eq!(
            second_day,
            second
                .local_atmospheric_flux_at_tick(&configuration)
                .expect("replayed second-day flux")
        );
        assert_eq!(first_day.0 + second_day.0, 2_000);
        assert_eq!(first_day.1, 1_000);
        assert_eq!(second_day.1, 1_000);

        let legacy = EngineState::new(WorldManifest::new(
            world_id,
            WorldSeed::new(0x005e_ed27),
            LOCAL_WEATHER_RULESET_VERSION,
        ));
        assert!(matches!(
            legacy.local_atmospheric_flux_at_tick(&configuration),
            Err(EngineError::LocalAtmosphericFluxUnsupported)
        ));

        let snapshot = Snapshot::new(
            EngineState::new(WorldManifest::new(
                world_id,
                WorldSeed::new(0x005e_ed28),
                LOCAL_ATMOSPHERIC_FLUX_RULESET_VERSION,
            )),
            EventSequence::ZERO,
            Digest::ZERO,
        )
        .expect("atmospheric-flux schema snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            LOCAL_ATMOSPHERIC_FLUX_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("atmospheric-flux snapshot integrity");
    }

    #[test]
    fn ruleset_twenty_six_retains_the_environment_normal_temperature() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x126));
        let state = EngineState::new(WorldManifest::new(
            world_id,
            WorldSeed::new(0x005e_ed26),
            PERSON_COGNITION_RULESET_VERSION,
        ));
        assert_eq!(
            state
                .local_temperature_at_tick(&environmental_provisional_full_earth_configuration())
                .expect("legacy normal temperature"),
            (2, 1)
        );
    }

    fn reproductive_fixture_profile(species: SpeciesIdentity) -> ReproductivePhysiologyCommitment {
        ReproductivePhysiologyCommitment {
            commitment_schema_version:
                world_domain::LEGACY_REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION,
            profile_id: "reproductive-fixture-v1".to_owned(),
            profile_digest: Digest::sha256(b"explicit reproductive fixture assumptions"),
            species,
            evidence_basis: world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption,
            tick_duration_seconds: 300,
            maturity_age_ticks: 10,
            category_maturity: Vec::new(),
            development_ticks: 2,
            recovery_ticks: 2,
            opportunity_interval_ticks: 1,
            initiation_probability_millionths: REPRODUCTIVE_PROBABILITY_SCALE,
            compatible_pairs: vec![world_domain::ReproductiveCategoryPair {
                first: BirthCategory::new("female").expect("category"),
                second: BirthCategory::new("male").expect("category"),
                developing_parent: BirthCategory::new("female").expect("category"),
            }],
            offspring_categories: vec![
                world_domain::OffspringCategoryWeight {
                    category: BirthCategory::new("female").expect("category"),
                    weight: 1,
                },
                world_domain::OffspringCategoryWeight {
                    category: BirthCategory::new("male").expect("category"),
                    weight: 1,
                },
            ],
        }
    }

    fn heritable_fixture_profile(species: SpeciesIdentity) -> HeritableDispositionProfile {
        HeritableDispositionProfile {
            profile_schema_version: world_domain::HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
            profile_id: "heritable-fixture-v1".to_owned(),
            profile_digest: Digest::sha256(b"explicit heritable disposition fixture assumptions"),
            species,
            evidence_basis: world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption,
            minimum_action_weight: 4,
            neutral_action_weight: 16,
            maximum_action_weight: 28,
            founder_variation_steps: 3,
            mutation_probability_millionths: 100_000,
            mutation_max_step: 2,
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
                    contact_region: None,
                    movement_direction: None,
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
            evidence_basis: world_domain::PhysiologicalEvidenceBasis::SourceMeasurement,
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
            development_id: None,
            species: other_species,
            role: OrganismRole::Fauna,
            birth_category: BirthCategory::new("unspecified").expect("category"),
            parent_ids: vec![parent_id],
            location_id: None,
            embodied_patch: None,
            metabolic_rate: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
            heritable_disposition: None,
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
                    contact_region: None,
                    movement_direction: None,
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
                    contact_region: None,
                    movement_direction: None,
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
                    contact_region: None,
                    movement_direction: None,
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
    fn local_interaction_reaches_nearby_landscape_cells_but_remains_bounded() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x135));
        let legacy_manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(0x10ca1),
            SIGNAL_PROPAGATION_RULESET_VERSION,
        );
        let source_patch: S2CellId = "0000000000004000".parse().expect("L23 source patch");
        let source_landscape = source_patch.ancestor(18).expect("L18 landscape");
        let neighboring_landscape =
            s2_edge_neighbors(source_landscape).expect("landscape neighbors")[0];
        let nearby_patch = neighboring_landscape
            .descendants_at(23)
            .expect("nearby embodied descendants")[0];
        let audible_landscapes = std::iter::once(source_landscape)
            .chain(s2_edge_neighbors(source_landscape).expect("audible neighbors"))
            .collect::<BTreeSet<_>>();
        let remote_landscape = s2_edge_neighbors(neighboring_landscape)
            .expect("second landscape ring")
            .into_iter()
            .find(|candidate| !audible_landscapes.contains(candidate))
            .expect("a landscape beyond the audible ring");
        let remote_patch = remote_landscape
            .descendants_at(23)
            .expect("remote embodied descendants")[0];

        let mut source = full_earth_person(world_id);
        source.organism_id = EntityId::from_uuid(Uuid::from_u128(0x13501));
        source.embodied_patch = Some(source_patch);
        let mut organisms = vec![source.clone()];
        for ordinal in 0..10_u128 {
            let mut recipient = full_earth_person(world_id);
            recipient.organism_id = EntityId::from_uuid(Uuid::from_u128(0x13510 + ordinal));
            recipient.embodied_patch = Some(nearby_patch);
            organisms.push(recipient);
        }
        let mut remote = full_earth_person(world_id);
        remote.organism_id = EntityId::from_uuid(Uuid::from_u128(0x135ff));
        remote.embodied_patch = Some(remote_patch);
        organisms.push(remote.clone());

        let initial = EngineState::new(legacy_manifest);
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                organisms,
            )
            .expect("local-interaction fixture genesis");
        let (mut running, _) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("local-interaction fixture commit");
        running.manifest.ruleset_version = LOCAL_INTERACTION_RULESET_VERSION;

        let signal_events = running
            .plan_action(
                source.organism_id,
                PrimitiveAction {
                    kind: PrimitiveActionKind::EmitSignal,
                    target_id: None,
                    intensity: 7,
                    contact_region: None,
                    movement_direction: None,
                },
            )
            .expect("nearby signal plan");
        let recipients = signal_events
            .iter()
            .filter_map(|event| match event {
                DomainEvent::OrganismPerceived { organism_id, .. } => Some(*organism_id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(recipients.len(), MAX_LOCAL_SIGNAL_RECIPIENTS);
        assert!(!recipients.contains(&remote.organism_id));
        assert!(recipients.iter().all(|recipient_id| {
            running.organisms[recipient_id].embodied_patch == Some(nearby_patch)
        }));
        let mut after_signal = running.clone();
        after_signal
            .apply_events(&signal_events)
            .expect("direct sound becomes private perception memory");
        let recipient = after_signal
            .organisms
            .get(&recipients[0])
            .expect("bounded recipient");
        assert_eq!(
            after_signal
                .nearest_recent_signal_source_patch(recipient)
                .expect("heard-source target"),
            Some(source_patch)
        );

        let actions = running
            .organisms
            .keys()
            .copied()
            .map(|organism_id| {
                (
                    organism_id,
                    PrimitiveAction {
                        kind: PrimitiveActionKind::Rest,
                        target_id: None,
                        intensity: 1,
                        contact_region: None,
                        movement_direction: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let observations = running
            .social_observations(&actions, None)
            .expect("nearby social observations");
        assert_ne!(
            observations.get(&source.organism_id),
            Some(&remote.organism_id)
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
            development_id: None,
            species: person.species.clone(),
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("female").expect("category"),
            parent_ids: vec![person.organism_id],
            location_id: None,
            embodied_patch: person.embodied_patch,
            metabolic_rate: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
            heritable_disposition: None,
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
                        contact_region: None,
                        movement_direction: None,
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
                    contact_region: None,
                    movement_direction: None,
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
                    contact_region: None,
                    movement_direction: None,
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
    fn ruleset_fourteen_reproduces_only_through_private_committed_development() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x118));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(13503953896175478587),
            REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION,
        );
        assert!(EngineState::new(manifest.clone()).validate().is_ok());
        let fabricated_personless_genesis = vec![
            DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            },
            DomainEvent::WorldConfigured {
                configuration: environmental_provisional_full_earth_configuration(),
            },
            DomainEvent::WorldExtinct,
            DomainEvent::WorldArchived,
        ];
        assert!(matches!(
            EngineState::new(manifest.clone()).commit(
                EventSequence::new(1),
                Digest::ZERO,
                fabricated_personless_genesis,
            ),
            Err(EngineError::MissingInitialPeople)
        ));
        let mut forged_personless_archive = EngineState::new(manifest.clone());
        forged_personless_archive.status = WorldStatus::Archived;
        assert!(matches!(
            Snapshot::new(
                forged_personless_archive,
                EventSequence::new(1),
                Digest::sha256(b"forged personless archive"),
            ),
            Err(EngineError::MissingInitialPeople)
        ));
        assert!(matches!(
            EngineState::new(manifest.clone()).plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                Vec::new(),
            ),
            Err(EngineError::MissingInitialPeople)
        ));
        let mut developing_parent =
            regulated_full_earth_person(world_id, 0x601, 10_000_000_000, 10_000_000);
        developing_parent.birth_category = BirthCategory::new("female").expect("category");
        developing_parent.initial_age_ticks = 20;
        let profile = reproductive_fixture_profile(developing_parent.species.clone());
        developing_parent.reproductive_physiology = Some(profile.clone());
        let mut other_parent =
            regulated_full_earth_person(world_id, 0x602, 10_000_000_000, 10_000_000);
        other_parent.birth_category = BirthCategory::new("male").expect("category");
        other_parent.initial_age_ticks = 20;
        other_parent.reproductive_physiology = Some(profile);

        let mut mismatched_tick_parent = developing_parent.clone();
        mismatched_tick_parent
            .reproductive_physiology
            .as_mut()
            .expect("reproductive profile")
            .tick_duration_seconds = 301;
        assert!(matches!(
            EngineState::new(manifest.clone()).plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![mismatched_tick_parent, other_parent.clone()],
            ),
            Err(EngineError::InvalidReproductiveCommitment(id))
                if id == developing_parent.organism_id
        ));

        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![developing_parent.clone(), other_parent.clone()],
            )
            .expect("reproductive genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("reproductive genesis");
        assert_eq!(
            genesis.event_schema_version,
            REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            running.state_hash_schema_version(),
            REPRODUCTIVE_PHYSIOLOGY_STATE_HASH_SCHEMA_VERSION
        );
        let injected_id = EntityId::from_uuid(Uuid::from_u128(0x603));
        let injected = DomainEvent::OrganismInitialized {
            organism_id: injected_id,
            species: developing_parent.species.clone(),
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("female").expect("category"),
            initial_age_ticks: 20,
            location_id: None,
            embodied_patch: developing_parent.embodied_patch,
            metabolic_rate: developing_parent.metabolic_rate.clone(),
            physiological_regulation: developing_parent.physiological_regulation.clone(),
            reproductive_physiology: developing_parent.reproductive_physiology.clone(),
            heritable_disposition_profile: None,
            heritable_disposition: None,
        };
        assert!(matches!(
            running.commit(EventSequence::new(2), genesis.batch_hash, vec![injected]),
            Err(EngineError::OrganismInitializationOutsideGenesis)
        ));

        let first_tick_events = running
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(300),
                CartesianMillimetres::new(1, 2, 3),
                CartesianMillimetres::new(4, 5, 6),
            ))
            .expect("first reproductive tick");
        let mut double_advance = first_tick_events.clone();
        double_advance.insert(
            1,
            DomainEvent::TickAdvanced {
                from: SimTick::new(1),
                to: SimTick::new(2),
            },
        );
        assert!(matches!(
            running.commit(EventSequence::new(2), genesis.batch_hash, double_advance,),
            Err(EngineError::InvalidTickAdvanceEventSet)
        ));
        let mut late_advance = first_tick_events.clone();
        late_advance.swap(0, 1);
        assert!(matches!(
            running.commit(EventSequence::new(2), genesis.batch_hash, late_advance,),
            Err(EngineError::InvalidTickAdvanceEventSet)
        ));
        let start = first_tick_events
            .iter()
            .find_map(|event| match event {
                DomainEvent::ReproductiveDevelopmentStarted {
                    development_id,
                    offspring_id,
                    parent_ids,
                    developing_parent_id,
                    due_tick,
                    ..
                } => Some((
                    *development_id,
                    *offspring_id,
                    parent_ids.clone(),
                    *developing_parent_id,
                    *due_tick,
                )),
                _ => None,
            })
            .expect("eligible co-located pair begins private development");
        assert_eq!(start.2.len(), 2);
        assert_eq!(start.3, developing_parent.organism_id);
        assert_eq!(start.4, SimTick::new(3));

        let omitted = first_tick_events
            .iter()
            .filter(|event| !matches!(event, DomainEvent::ReproductiveDevelopmentStarted { .. }))
            .cloned()
            .collect::<Vec<_>>();
        assert!(matches!(
            running.commit(EventSequence::new(2), genesis.batch_hash, omitted),
            Err(EngineError::InvalidReproductiveEventSet)
        ));
        let (after_start, first_tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, first_tick_events)
            .expect("committed private development");
        assert!(
            after_start
                .pending_reproductive_developments
                .contains_key(&start.0)
        );
        let mut fabricated_snapshot_state = after_start.clone();
        fabricated_snapshot_state
            .pending_reproductive_developments
            .get_mut(&start.0)
            .expect("pending development")
            .started_at = SimTick::ZERO;
        assert!(matches!(
            Snapshot::new(
                fabricated_snapshot_state,
                first_tick.sequence,
                first_tick.batch_hash,
            ),
            Err(EngineError::InvalidReproductiveDevelopment(id)) if id == start.0
        ));
        let mut mismatched_tick_snapshot_state = after_start.clone();
        mismatched_tick_snapshot_state
            .organisms
            .get_mut(&developing_parent.organism_id)
            .expect("developing parent")
            .reproductive_physiology
            .as_mut()
            .expect("reproductive profile")
            .tick_duration_seconds = 301;
        assert!(matches!(
            Snapshot::new(
                mismatched_tick_snapshot_state,
                first_tick.sequence,
                first_tick.batch_hash,
            ),
            Err(EngineError::InvalidReproductiveCommitment(id))
                if id == developing_parent.organism_id
        ));

        let non_developing_parent_id = start
            .2
            .iter()
            .copied()
            .find(|parent_id| *parent_id != start.3)
            .expect("other parent");
        let first_death_events = after_start
            .plan_death(
                non_developing_parent_id,
                DeathCause {
                    mechanism: "extinction_fixture".to_owned(),
                },
            )
            .expect("first parent death plan");
        let (one_parent_left, first_death) = after_start
            .commit(
                EventSequence::new(3),
                first_tick.batch_hash,
                first_death_events,
            )
            .expect("first parent death commit");
        let extinction_events = one_parent_left
            .plan_death(
                start.3,
                DeathCause {
                    mechanism: "extinction_fixture".to_owned(),
                },
            )
            .expect("last parent death plan");
        assert!(matches!(
            extinction_events.as_slice(),
            [
                DomainEvent::OrganismDied { .. },
                DomainEvent::ReproductiveDevelopmentEnded { .. },
                DomainEvent::WorldExtinct,
                DomainEvent::WorldArchived,
            ]
        ));
        let extinction_without_lifecycle =
            extinction_events[..extinction_events.len() - 2].to_vec();
        assert!(matches!(
            one_parent_left.commit(
                EventSequence::new(4),
                first_death.batch_hash,
                extinction_without_lifecycle,
            ),
            Err(EngineError::InvalidWorldLifecycleEventSet)
        ));
        let (archived_branch, _) = one_parent_left
            .commit(
                EventSequence::new(4),
                first_death.batch_hash,
                extinction_events,
            )
            .expect("exact extinction suffix commits");
        assert_eq!(archived_branch.status(), WorldStatus::Archived);
        let mut transient_extinct_snapshot = archived_branch.clone();
        transient_extinct_snapshot.status = WorldStatus::Extinct;
        assert!(matches!(
            Snapshot::new(
                transient_extinct_snapshot,
                first_death.sequence,
                first_death.batch_hash,
            ),
            Err(EngineError::UnarchivedWorldExtinction)
        ));

        let cancellation_events = after_start
            .plan_death(
                start.3,
                DeathCause {
                    mechanism: "test_fixture".to_owned(),
                },
            )
            .expect("developing-parent death plan");
        assert!(matches!(
            cancellation_events.as_slice(),
            [
                DomainEvent::OrganismDied { .. },
                DomainEvent::ReproductiveDevelopmentEnded {
                    development_id,
                    reason: ReproductiveDevelopmentEnd::DevelopingParentUnavailable,
                    ..
                }
            ] if *development_id == start.0
        ));
        let reordered_cancellation = vec![
            cancellation_events[0].clone(),
            DomainEvent::WorldExtinct,
            DomainEvent::WorldArchived,
            cancellation_events[1].clone(),
        ];
        assert!(matches!(
            after_start.commit(
                EventSequence::new(3),
                first_tick.batch_hash,
                reordered_cancellation,
            ),
            Err(EngineError::InvalidReproductiveEventSet)
        ));
        let (cancelled, _) = after_start
            .commit(
                EventSequence::new(3),
                first_tick.batch_hash,
                cancellation_events,
            )
            .expect("private development cancellation");
        assert!(cancelled.pending_reproductive_developments.is_empty());

        let second_tick_events = after_start
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(600),
                CartesianMillimetres::new(2, 3, 4),
                CartesianMillimetres::new(5, 6, 7),
            ))
            .expect("second reproductive tick");
        assert!(
            !second_tick_events
                .iter()
                .any(|event| matches!(event, DomainEvent::OrganismBorn { .. }))
        );
        let (after_second, second_tick) = after_start
            .commit(
                EventSequence::new(3),
                first_tick.batch_hash,
                second_tick_events,
            )
            .expect("second reproductive tick commit");
        let third_tick_events = after_second
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(900),
                CartesianMillimetres::new(3, 4, 5),
                CartesianMillimetres::new(6, 7, 8),
            ))
            .expect("birth tick");
        let birth_index = third_tick_events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DomainEvent::OrganismBorn {
                        organism_id,
                        development_id: Some(development_id),
                        parent_ids,
                        ..
                    } if *organism_id == start.1
                        && *development_id == start.0
                        && *parent_ids == start.2
                )
            })
            .expect("development resolves as a bound birth");
        let mut reordered = third_tick_events.clone();
        reordered.swap(birth_index - 1, birth_index);
        assert!(matches!(
            after_second.commit(EventSequence::new(4), second_tick.batch_hash, reordered),
            Err(EngineError::InvalidReproductiveEventSet)
        ));
        let (after_birth, third_tick) = after_second
            .commit(
                EventSequence::new(4),
                second_tick.batch_hash,
                third_tick_events,
            )
            .expect("bound birth commit");
        assert!(after_birth.pending_reproductive_developments.is_empty());
        assert_eq!(
            after_birth
                .organisms()
                .filter(|organism| organism.is_alive())
                .count(),
            3
        );
        let newborn = after_birth
            .organisms
            .get(&start.1)
            .expect("durable newborn identity");
        assert_eq!(newborn.parent_ids, start.2);
        assert_eq!(newborn.born_at, Some(SimTick::new(3)));

        let snapshot = Snapshot::new(
            after_birth.clone(),
            third_tick.sequence,
            third_tick.batch_hash,
        )
        .expect("reproductive snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            REPRODUCTIVE_PHYSIOLOGY_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot.verify_integrity().expect("snapshot integrity");
        assert_eq!(
            replay(
                manifest.clone(),
                &[genesis.clone(), first_tick.clone(), second_tick, third_tick,],
            )
            .expect("reproductive replay")
            .state,
            after_birth
        );

        let downgraded = EventBatch::new(
            ACTION_LEARNING_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"downgraded reproductive state"),
        )
        .expect("internally valid pre-reproductive batch");
        assert!(matches!(
            replay(manifest, &[downgraded]),
            Err(EngineError::BatchEventSchemaMismatch {
                expected: REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION,
                actual: ACTION_LEARNING_EVENT_SCHEMA_VERSION,
            })
        ));
    }

    #[test]
    fn ruleset_fifteen_inherits_only_bounded_dispositions_and_replays() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x120));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(13503953896175478591),
            HERITABLE_DISPOSITION_RULESET_VERSION,
        );
        let mut developing_parent =
            regulated_full_earth_person(world_id, 0x701, 10_000_000_000, 10_000_000);
        developing_parent.birth_category = BirthCategory::new("female").expect("category");
        developing_parent.initial_age_ticks = 20;
        let reproductive_profile = reproductive_fixture_profile(developing_parent.species.clone());
        let heritable_profile = heritable_fixture_profile(developing_parent.species.clone());
        assert_eq!(
            bounded_mutated_weight(
                heritable_profile.minimum_action_weight,
                heritable_profile.mutation_max_step,
                false,
                &heritable_profile,
            ),
            heritable_profile.minimum_action_weight
        );
        assert_eq!(
            bounded_mutated_weight(
                heritable_profile.maximum_action_weight,
                heritable_profile.mutation_max_step,
                true,
                &heritable_profile,
            ),
            heritable_profile.maximum_action_weight
        );
        developing_parent.reproductive_physiology = Some(reproductive_profile.clone());
        developing_parent.heritable_disposition_profile = Some(heritable_profile.clone());

        let mut other_parent =
            regulated_full_earth_person(world_id, 0x702, 10_000_000_000, 10_000_000);
        other_parent.birth_category = BirthCategory::new("male").expect("category");
        other_parent.initial_age_ticks = 20;
        other_parent.reproductive_physiology = Some(reproductive_profile);
        other_parent.heritable_disposition_profile = Some(heritable_profile.clone());

        let mut missing_profile = developing_parent.clone();
        missing_profile.heritable_disposition_profile = None;
        assert!(matches!(
            EngineState::new(manifest.clone()).plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![missing_profile, other_parent.clone()],
            ),
            Err(EngineError::MissingHeritableDispositionProfile(id))
                if id == developing_parent.organism_id
        ));

        let mut mixed_profile_parent = other_parent.clone();
        let mut mixed_profile = heritable_profile.clone();
        mixed_profile.profile_id = "heritable-fixture-v2".to_owned();
        mixed_profile.profile_digest = Digest::sha256(b"different disposition assumptions");
        mixed_profile_parent.heritable_disposition_profile = Some(mixed_profile);
        assert!(matches!(
            EngineState::new(manifest.clone()).plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![developing_parent.clone(), mixed_profile_parent],
            ),
            Err(EngineError::InvalidHeritableDisposition)
        ));

        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![developing_parent.clone(), other_parent.clone()],
            )
            .expect("heritable genesis plan");
        let reverse_events = EngineState::new(manifest.clone())
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![other_parent.clone(), developing_parent.clone()],
            )
            .expect("input-order-independent heritable genesis plan");
        assert_eq!(genesis_events, reverse_events);

        let founder_dispositions = genesis_events
            .iter()
            .filter_map(|event| match event {
                DomainEvent::OrganismInitialized {
                    organism_id,
                    heritable_disposition: Some(disposition),
                    ..
                } => Some((*organism_id, disposition.clone())),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(founder_dispositions.len(), 2);
        assert_ne!(
            founder_dispositions[&developing_parent.organism_id],
            founder_dispositions[&other_parent.organism_id]
        );
        assert!(
            founder_dispositions
                .values()
                .all(|disposition| disposition.generation == 0
                    && disposition.derived_at == SimTick::ZERO)
        );

        let mut tampered_genesis = genesis_events.clone();
        let tampered_founder = tampered_genesis
            .iter_mut()
            .find_map(|event| match event {
                DomainEvent::OrganismInitialized {
                    heritable_disposition: Some(disposition),
                    ..
                } => Some(disposition),
                _ => None,
            })
            .expect("founder disposition");
        let founder_weight = &mut tampered_founder.action_weights[0].weight;
        *founder_weight = if *founder_weight < heritable_profile.maximum_action_weight {
            *founder_weight + 1
        } else {
            *founder_weight - 1
        };
        assert!(matches!(
            EngineState::new(manifest.clone()).commit(
                EventSequence::new(1),
                Digest::ZERO,
                tampered_genesis,
            ),
            Err(EngineError::InvalidHeritableDisposition)
        ));

        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("heritable genesis");
        assert_eq!(
            genesis.event_schema_version,
            HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            running.state_hash_schema_version(),
            HERITABLE_DISPOSITION_STATE_HASH_SCHEMA_VERSION
        );
        let founder_candidates = [developing_parent.organism_id, other_parent.organism_id]
            .into_iter()
            .map(|organism_id| {
                let organism = running.organisms.get(&organism_id).expect("founder");
                let disposition = organism
                    .heritable_disposition
                    .as_ref()
                    .expect("founder disposition");
                let candidates = running
                    .deterministic_policy_candidates(organism, organism.initial_age_ticks)
                    .expect("heritable policy candidates");
                for candidate in &candidates {
                    let base = match candidate.action.kind {
                        PrimitiveActionKind::Move
                        | PrimitiveActionKind::Orient
                        | PrimitiveActionKind::EmitSignal => 2,
                        PrimitiveActionKind::Reach | PrimitiveActionKind::Rest => 1,
                        _ => panic!("no material-dependent candidate at genesis"),
                    };
                    assert_eq!(
                        candidate.weight,
                        base * u32::from(
                            disposition
                                .action_weight(candidate.action.kind)
                                .expect("complete disposition"),
                        )
                    );
                }
                candidates
            })
            .collect::<Vec<_>>();
        assert_ne!(founder_candidates[0], founder_candidates[1]);

        let mut parent_ids = vec![developing_parent.organism_id, other_parent.organism_id];
        parent_ids.sort_unstable();
        let probe_offspring = EntityId::deterministic(world_id, b"heredity-learning-probe");
        let before_learning = running
            .offspring_heritable_disposition(
                probe_offspring,
                &parent_ids,
                SimTick::new(1),
                &heritable_profile,
            )
            .expect("offspring disposition without learned state");
        let mut learned_variant = running.clone();
        learned_variant
            .organisms
            .get_mut(&developing_parent.organism_id)
            .expect("parent")
            .action_values = vec![ActionValueState {
            value_schema_version: ACTION_VALUE_STATE_SCHEMA_VERSION,
            action_kind: PrimitiveActionKind::EmitSignal,
            observations: 999,
            value: ACTION_VALUE_MAX,
        }];
        assert_eq!(
            learned_variant
                .offspring_heritable_disposition(
                    probe_offspring,
                    &parent_ids,
                    SimTick::new(1),
                    &heritable_profile,
                )
                .expect("learned history cannot enter heredity"),
            before_learning
        );

        let first_tick_events = running
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(300),
                CartesianMillimetres::new(1, 2, 3),
                CartesianMillimetres::new(4, 5, 6),
            ))
            .expect("first heritable tick");
        let (development_id, offspring_id, committed_offspring) = first_tick_events
            .iter()
            .find_map(|event| match event {
                DomainEvent::ReproductiveDevelopmentStarted {
                    development_id,
                    offspring_id,
                    heritable_disposition_profile: Some(profile),
                    offspring_heritable_disposition: Some(disposition),
                    ..
                } => Some((
                    *development_id,
                    *offspring_id,
                    (profile.clone(), disposition.clone()),
                )),
                _ => None,
            })
            .expect("development commits offspring disposition");
        assert_eq!(committed_offspring.0, heritable_profile);
        assert_eq!(committed_offspring.1.generation, 1);
        assert_eq!(committed_offspring.1.derived_at, SimTick::new(1));

        let mut tampered_start = first_tick_events.clone();
        let tampered_child = tampered_start
            .iter_mut()
            .find_map(|event| match event {
                DomainEvent::ReproductiveDevelopmentStarted {
                    offspring_heritable_disposition: Some(disposition),
                    ..
                } => Some(disposition),
                _ => None,
            })
            .expect("offspring disposition");
        let child_weight = &mut tampered_child.action_weights[0].weight;
        *child_weight = if *child_weight < heritable_profile.maximum_action_weight {
            *child_weight + 1
        } else {
            *child_weight - 1
        };
        assert!(
            running
                .clone()
                .commit(EventSequence::new(2), genesis.batch_hash, tampered_start)
                .is_err()
        );

        let (after_start, first_tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, first_tick_events)
            .expect("committed inherited development");
        let pending = after_start
            .pending_reproductive_developments
            .get(&development_id)
            .expect("pending development");
        assert_eq!(
            pending.offspring_heritable_disposition.as_ref(),
            Some(&committed_offspring.1)
        );
        let mut tampered_pending = after_start.clone();
        tampered_pending
            .pending_reproductive_developments
            .get_mut(&development_id)
            .expect("pending development")
            .offspring_heritable_disposition
            .as_mut()
            .expect("pending disposition")
            .generation = 2;
        assert!(
            Snapshot::new(tampered_pending, first_tick.sequence, first_tick.batch_hash,).is_err()
        );
        let pending_snapshot = Snapshot::new(
            after_start.clone(),
            first_tick.sequence,
            first_tick.batch_hash,
        )
        .expect("pending heredity snapshot");

        let second_tick_events = after_start
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(600),
                CartesianMillimetres::new(2, 3, 4),
                CartesianMillimetres::new(5, 6, 7),
            ))
            .expect("second heritable tick");
        let (after_second, second_tick) = after_start
            .commit(
                EventSequence::new(3),
                first_tick.batch_hash,
                second_tick_events,
            )
            .expect("second heritable tick commit");
        let birth_events = after_second
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(900),
                CartesianMillimetres::new(3, 4, 5),
                CartesianMillimetres::new(6, 7, 8),
            ))
            .expect("heritable birth tick");
        let born_disposition = birth_events
            .iter()
            .find_map(|event| match event {
                DomainEvent::OrganismBorn {
                    organism_id,
                    development_id: Some(event_development_id),
                    heritable_disposition_profile: Some(profile),
                    heritable_disposition: Some(disposition),
                    ..
                } if *organism_id == offspring_id && *event_development_id == development_id => {
                    Some((profile.clone(), disposition.clone()))
                }
                _ => None,
            })
            .expect("birth copies committed disposition");
        assert_eq!(born_disposition, committed_offspring);

        let mut tampered_birth = birth_events.clone();
        tampered_birth
            .iter_mut()
            .find_map(|event| match event {
                DomainEvent::OrganismBorn {
                    organism_id,
                    heritable_disposition: Some(disposition),
                    ..
                } if *organism_id == offspring_id => Some(disposition),
                _ => None,
            })
            .expect("birth disposition")
            .generation = 2;
        assert!(
            after_second
                .clone()
                .commit(
                    EventSequence::new(4),
                    second_tick.batch_hash,
                    tampered_birth,
                )
                .is_err()
        );

        let (after_birth, birth_tick) = after_second
            .commit(EventSequence::new(4), second_tick.batch_hash, birth_events)
            .expect("heritable birth commit");
        let newborn = after_birth.organisms.get(&offspring_id).expect("newborn");
        assert_eq!(
            newborn.heritable_disposition.as_ref(),
            Some(&committed_offspring.1)
        );
        assert!(newborn.perception_memory.is_empty());
        assert!(newborn.action_values.is_empty());
        assert!(newborn.bodily_regulation.is_clear());
        assert_eq!(newborn.bodily_regulated_at, None);
        assert_eq!(newborn.action_values_updated_at, None);

        let mut tampered_born_state = after_birth.clone();
        tampered_born_state
            .organisms
            .get_mut(&offspring_id)
            .expect("newborn")
            .heritable_disposition
            .as_mut()
            .expect("newborn disposition")
            .generation = 2;
        assert!(
            Snapshot::new(
                tampered_born_state,
                birth_tick.sequence,
                birth_tick.batch_hash,
            )
            .is_err()
        );

        let snapshot = Snapshot::new(
            after_birth.clone(),
            birth_tick.sequence,
            birth_tick.batch_hash,
        )
        .expect("heritable snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            HERITABLE_DISPOSITION_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot.verify_integrity().expect("snapshot integrity");
        assert_eq!(
            replay_from_snapshot(
                &pending_snapshot,
                &[second_tick.clone(), birth_tick.clone()],
            )
            .expect("heritable snapshot plus tail replay")
            .state,
            after_birth
        );
        assert_eq!(
            replay(
                manifest.clone(),
                &[genesis.clone(), first_tick, second_tick, birth_tick],
            )
            .expect("heritable replay")
            .state,
            after_birth
        );

        let downgraded = EventBatch::new(
            REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            HERITABLE_DISPOSITION_RULESET_VERSION,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"downgraded heredity state"),
        )
        .expect("internally valid pre-heredity batch");
        assert!(matches!(
            replay(manifest, &[downgraded]),
            Err(EngineError::BatchEventSchemaMismatch {
                expected: HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION,
                actual: REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION,
            })
        ));
    }

    #[test]
    fn ruleset_sixteen_selects_only_canonical_bounded_cognition_inputs() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x121));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(13503953896175478592),
            COGNITION_RULESET_VERSION,
        );
        let mut founder = regulated_full_earth_person(world_id, 0x711, 10_000_000_000, 10_000_000);
        founder.birth_category = BirthCategory::new("female").expect("category");
        founder.initial_age_ticks = 20;
        founder.reproductive_physiology =
            Some(reproductive_fixture_profile(founder.species.clone()));
        founder.heritable_disposition_profile =
            Some(heritable_fixture_profile(founder.species.clone()));
        let organism_id = founder.organism_id;

        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![founder],
            )
            .expect("cognition genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("cognition genesis");
        assert_eq!(genesis.event_schema_version, COGNITION_EVENT_SCHEMA_VERSION);
        assert_eq!(
            running.state_hash_schema_version(),
            COGNITION_STATE_HASH_SCHEMA_VERSION
        );

        let selection_events = running
            .plan_cognition_request(organism_id)
            .expect("canonical cognition selection");
        assert_eq!(
            running
                .plan_scheduled_cognition_request()
                .expect("world-selected cognition subject"),
            selection_events
        );
        let DomainEvent::CognitionRequestSelected { selection } = &selection_events[0] else {
            panic!("selection planner emitted a different event")
        };
        assert_eq!(selection.world_id, world_id);
        assert_eq!(selection.organism_id, organism_id);
        assert_eq!(selection.selected_at_tick, SimTick::ZERO);
        assert_eq!(
            selection.deadline_tick,
            SimTick::new(COGNITION_RESPONSE_WINDOW_TICKS)
        );
        assert_eq!(selection.ordinal, COGNITION_REQUEST_ORDINAL);
        assert_eq!(selection.memory_max_tokens, COGNITION_MEMORY_MAX_TOKENS);
        assert_eq!(
            selection.model_max_output_tokens,
            COGNITION_MODEL_MAX_OUTPUT_TOKENS
        );
        assert_eq!(selection.bodily_needs, BodilyNeedState::default());
        assert!(selection.readings.is_empty());
        assert!(selection.action_values.is_empty());

        let mut forged_events = selection_events.clone();
        let DomainEvent::CognitionRequestSelected { selection } = &mut forged_events[0] else {
            unreachable!()
        };
        selection.memory_query = "observer supplied objective".to_owned();
        assert!(matches!(
            running
                .clone()
                .commit(EventSequence::new(2), genesis.batch_hash, forged_events,),
            Err(EngineError::InvalidCognitionSelection(_))
        ));

        let selected_request_id = match &selection_events[0] {
            DomainEvent::CognitionRequestSelected { selection } => selection.request_id,
            _ => unreachable!(),
        };
        let (pending, selection_batch) = running
            .commit(EventSequence::new(2), genesis.batch_hash, selection_events)
            .expect("canonical cognition selection commit");
        assert_eq!(
            selection_batch.event_schema_version,
            COGNITION_EVENT_SCHEMA_VERSION
        );
        assert!(
            pending
                .pending_cognition_requests
                .contains_key(&selected_request_id)
        );
        assert!(matches!(
            pending.plan_cognition_request(organism_id),
            Err(EngineError::CognitionRequestAlreadyPending)
        ));

        let snapshot = Snapshot::new(
            pending.clone(),
            selection_batch.sequence,
            selection_batch.batch_hash,
        )
        .expect("pending cognition snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            COGNITION_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot.verify_integrity().expect("snapshot integrity");
        assert_eq!(
            replay(manifest.clone(), &[genesis, selection_batch])
                .expect("cognition request replay")
                .state,
            pending
        );

        let legacy_manifest = WorldManifest::new(
            world_id,
            manifest.seed,
            HERITABLE_DISPOSITION_RULESET_VERSION,
        );
        assert!(matches!(
            EngineState::new(legacy_manifest).plan_cognition_request(organism_id),
            Err(EngineError::CognitionUnsupported)
        ));
    }

    #[test]
    fn ruleset_twenty_six_reserves_external_cognition_for_people() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x126));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(18111088317882099744),
            PERSON_COGNITION_RULESET_VERSION,
        );
        let mut person = regulated_full_earth_person(world_id, 0x1261, 10_000_000_000, 10_000_000);
        person.birth_category = BirthCategory::new("female").expect("category");
        person.initial_age_ticks = 20;
        person.reproductive_physiology = Some(reproductive_fixture_profile(person.species.clone()));
        person.heritable_disposition_profile =
            Some(heritable_fixture_profile(person.species.clone()));
        let person_id = person.organism_id;

        let mut fauna = person.clone();
        fauna.organism_id = EntityId::from_uuid(Uuid::from_u128(0x1262));
        fauna.role = OrganismRole::Fauna;
        let fauna_id = fauna.organism_id;

        let material = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("real water identity");
        let patch = person.embodied_patch.expect("founder patch");
        let oral_profile = OralTransferCommitment {
            commitment_schema_version: world_domain::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
            profile_id: "person-cognition-water-oral-v1".to_owned(),
            profile_digest: Digest::sha256(b"person cognition water oral fixture"),
            material: material.clone(),
            species: person.species.clone(),
            evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
            transfer_mass_milligrams: 1,
            recoverable_energy_joules: 1,
            hydration_recovery_seconds: 1,
        };
        let initial_material = InitialMaterialInstance {
            object_id: EntityId::deterministic(world_id, b"person-cognition-water"),
            material: material.clone(),
            embodied_patch: patch,
            initial_mass_milligrams: Some(1_000_000),
            oral_transfer_profiles: vec![oral_profile],
            reservoir: Some(MaterialReservoirCommitment {
                commitment_schema_version:
                    world_domain::MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
                profile_id: "person-cognition-water-v1".to_owned(),
                profile_digest: Digest::sha256(b"person cognition water fixture"),
                material,
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                coverage_patch: patch.ancestor(10).expect("L10 coverage"),
                maximum_mass_milligrams: 2_000_000,
                replenishment_mass_milligrams_per_tick: 1,
            }),
        };

        let initial = EngineState::new(manifest);
        let genesis_events = initial
            .plan_configured_genesis_with_materials(
                environmental_provisional_full_earth_configuration(),
                vec![person, fauna],
                vec![initial_material],
            )
            .expect("person-plus-fauna cognition genesis");
        let (running, _) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("person-plus-fauna cognition genesis commit");

        let scheduled = running
            .plan_scheduled_cognition_request()
            .expect("scheduled person cognition");
        let DomainEvent::CognitionRequestSelected { selection } = &scheduled[0] else {
            panic!("selection planner emitted a different event")
        };
        assert_eq!(selection.organism_id, person_id);
        assert!(matches!(
            running.plan_cognition_request(fauna_id),
            Err(EngineError::InvalidCognitionSelection(_))
        ));
    }

    #[test]
    fn ruleset_eighteen_retains_bounded_social_learning_and_shared_resources() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x123));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(13503953896175478594),
            SOCIAL_LEARNING_RULESET_VERSION,
        );
        let mut first = regulated_full_earth_person(world_id, 0x721, 10_000_000, 1_000_000);
        let mut second = regulated_full_earth_person(world_id, 0x722, 10_000_000, 1_000_000);
        first.birth_category = BirthCategory::new("female").expect("category");
        second.birth_category = BirthCategory::new("male").expect("category");
        for founder in [&mut first, &mut second] {
            founder.reproductive_physiology =
                Some(reproductive_fixture_profile(founder.species.clone()));
            founder.heritable_disposition_profile =
                Some(heritable_fixture_profile(founder.species.clone()));
        }
        let patch = first.embodied_patch.expect("founder patch");
        let material = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("real water identity");
        let profile = OralTransferCommitment {
            commitment_schema_version: world_domain::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
            profile_id: "water-human-reservoir-fixture-v1".to_owned(),
            profile_digest: Digest::sha256(b"ruleset seventeen water response"),
            material: material.clone(),
            species: human(),
            evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
            transfer_mass_milligrams: 250_000,
            recoverable_energy_joules: 100,
            hydration_recovery_seconds: 200,
        };
        let object_id = EntityId::deterministic(world_id, b"shared-water-reservoir");
        let initial_material = InitialMaterialInstance {
            object_id,
            material: material.clone(),
            embodied_patch: patch,
            initial_mass_milligrams: Some(500_000),
            oral_transfer_profiles: vec![profile],
            reservoir: Some(MaterialReservoirCommitment {
                commitment_schema_version:
                    world_domain::MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
                profile_id: "shared-water-reservoir-v1".to_owned(),
                profile_digest: Digest::sha256(b"ruleset seventeen shared reservoir"),
                material,
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                coverage_patch: patch.ancestor(10).expect("L10 coverage"),
                maximum_mass_milligrams: 1_000_000,
                replenishment_mass_milligrams_per_tick: 100_000,
            }),
        };

        let probe_initial = EngineState::new(manifest.clone());
        let probe_events = probe_initial
            .plan_configured_genesis_with_materials(
                environmental_provisional_full_earth_configuration(),
                vec![first.clone(), second.clone()],
                vec![initial_material.clone()],
            )
            .expect("reservoir probe genesis");
        let (probe, _) = probe_initial
            .commit(EventSequence::new(1), Digest::ZERO, probe_events)
            .expect("reservoir probe commit");
        for founder in [&mut first, &mut second] {
            let organism = probe
                .organisms
                .get(&founder.organism_id)
                .expect("probe founder");
            let swallow_age = (1..=10_000)
                .find(|age_ticks| {
                    probe
                        .deterministic_policy_action(organism, *age_ticks)
                        .is_ok_and(|action| {
                            action.kind == PrimitiveActionKind::Swallow
                                && action.target_id == Some(object_id)
                        })
                })
                .expect("seeded policy eventually selects reservoir swallow");
            founder.initial_age_ticks = swallow_age - 1;
        }

        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis_with_materials(
                environmental_provisional_full_earth_configuration(),
                vec![first.clone(), second.clone()],
                vec![initial_material],
            )
            .expect("reservoir genesis");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("reservoir genesis commit");
        assert_eq!(
            genesis.event_schema_version,
            SOCIAL_LEARNING_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            running.state_hash_schema_version(),
            SOCIAL_LEARNING_STATE_HASH_SCHEMA_VERSION
        );

        let tick_events = running
            .plan_next_tick_with_celestial_and_cognition(
                CelestialState::new(
                    TdbSecondsSinceJ2000::new(300),
                    CartesianMillimetres::new(1, 2, 3),
                    CartesianMillimetres::new(4, 5, 6),
                ),
                &[],
            )
            .expect("shared reservoir tick");
        let transfers = tick_events
            .iter()
            .filter_map(|event| match event {
                DomainEvent::MaterialReservoirOralPortionTransferred { .. } => Some(event.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(transfers.len(), 2);
        let social = tick_events
            .iter()
            .filter(|event| matches!(event, DomainEvent::OrganismSocialActionValueChanged { .. }))
            .count();
        assert_eq!(
            social, 2,
            "each co-located founder attends to one other actor"
        );
        let mut missing_social = tick_events.clone();
        let social_index = missing_social
            .iter()
            .position(|event| matches!(event, DomainEvent::OrganismSocialActionValueChanged { .. }))
            .expect("social event index");
        missing_social.remove(social_index);
        assert!(matches!(
            running
                .clone()
                .commit(EventSequence::new(2), genesis.batch_hash, missing_social,),
            Err(EngineError::InvalidSocialActionValueTransition(_))
        ));
        assert!(matches!(
            transfers[0],
            DomainEvent::MaterialReservoirOralPortionTransferred {
                settled_from_tick,
                settled_to_tick,
                from_mass_milligrams: 500_000,
                replenished_mass_milligrams: 100_000,
                transferred_mass_milligrams: 250_000,
                to_mass_milligrams: 350_000,
                ..
            } if settled_from_tick == SimTick::ZERO && settled_to_tick == SimTick::new(1)
        ));
        assert!(matches!(
            transfers[1],
            DomainEvent::MaterialReservoirOralPortionTransferred {
                settled_from_tick,
                settled_to_tick,
                from_mass_milligrams: 350_000,
                replenished_mass_milligrams: 0,
                transferred_mass_milligrams: 250_000,
                to_mass_milligrams: 100_000,
                ..
            } if settled_from_tick == SimTick::new(1) && settled_to_tick == SimTick::new(1)
        ));

        let mut missing_transfer = tick_events.clone();
        let second_index = missing_transfer
            .iter()
            .rposition(|event| {
                matches!(
                    event,
                    DomainEvent::MaterialReservoirOralPortionTransferred { .. }
                )
            })
            .expect("second transfer index");
        missing_transfer.remove(second_index);
        assert!(matches!(
            running
                .clone()
                .commit(EventSequence::new(2), genesis.batch_hash, missing_transfer),
            Err(EngineError::InvalidMaterialReservoirEventSet)
        ));

        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, tick_events)
            .expect("shared reservoir tick commit");
        let reservoir = after_tick
            .material_instances
            .get(&object_id)
            .expect("shared reservoir state");
        assert_eq!(reservoir.remaining_mass_milligrams(), Some(100_000));
        assert_eq!(reservoir.reservoir_settled_at, Some(SimTick::new(1)));
        let snapshot = Snapshot::new(after_tick.clone(), tick.sequence, tick.batch_hash)
            .expect("reservoir snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            SOCIAL_LEARNING_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(
            replay(manifest, &[genesis, tick])
                .expect("reservoir replay")
                .state,
            after_tick
        );
    }

    #[test]
    fn ruleset_twenty_five_associates_signals_with_movement_directions() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x126));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(13_503_953_896_175_478_597),
            SIGNAL_MOTOR_ASSOCIATION_RULESET_VERSION,
        );
        let mut first = regulated_full_earth_person(world_id, 0x741, 10_000_000, 1_000_000);
        let mut second = regulated_full_earth_person(world_id, 0x742, 10_000_000, 1_000_000);
        first.birth_category = BirthCategory::new("female").expect("category");
        second.birth_category = BirthCategory::new("male").expect("category");
        for founder in [&mut first, &mut second] {
            founder.reproductive_physiology =
                Some(reproductive_fixture_profile(founder.species.clone()));
            founder.heritable_disposition_profile =
                Some(heritable_fixture_profile(founder.species.clone()));
        }
        let patch = first.embodied_patch.expect("founder patch");
        let water = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("water identity");
        let reservoir = InitialMaterialInstance {
            object_id: EntityId::deterministic(world_id, b"association-water"),
            material: water.clone(),
            embodied_patch: patch,
            initial_mass_milligrams: Some(1_000_000),
            oral_transfer_profiles: vec![OralTransferCommitment {
                commitment_schema_version: world_domain::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
                profile_id: "association-water-v1".to_owned(),
                profile_digest: Digest::sha256(b"association water response"),
                material: water.clone(),
                species: human(),
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                transfer_mass_milligrams: 10_000,
                recoverable_energy_joules: 100,
                hydration_recovery_seconds: 200,
            }],
            reservoir: Some(MaterialReservoirCommitment {
                commitment_schema_version:
                    world_domain::MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
                profile_id: "association-reservoir-v1".to_owned(),
                profile_digest: Digest::sha256(b"association reservoir"),
                material: water,
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                coverage_patch: patch.ancestor(10).expect("L10 coverage"),
                maximum_mass_milligrams: 1_000_000,
                replenishment_mass_milligrams_per_tick: 10_000,
            }),
        };

        let probe_initial = EngineState::new(manifest.clone());
        let probe_events = probe_initial
            .plan_configured_genesis_with_materials(
                environmental_provisional_full_earth_configuration(),
                vec![first.clone(), second.clone()],
                vec![reservoir.clone()],
            )
            .expect("association probe genesis");
        let (probe, _) = probe_initial
            .commit(EventSequence::new(1), Digest::ZERO, probe_events)
            .expect("association probe");
        for founder in [&mut first, &mut second] {
            let organism = probe.organisms.get(&founder.organism_id).expect("founder");
            let signal_age = (1..=10_000)
                .find(|age_ticks| {
                    probe
                        .deterministic_policy_action(organism, *age_ticks)
                        .is_ok_and(|action| action.kind == PrimitiveActionKind::EmitSignal)
                })
                .expect("policy eventually emits a signal");
            founder.initial_age_ticks = signal_age - 1;
        }

        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis_with_materials(
                environmental_provisional_full_earth_configuration(),
                vec![first, second],
                vec![reservoir],
            )
            .expect("association genesis");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("association genesis commit");
        assert_eq!(
            genesis.event_schema_version,
            SIGNAL_MOTOR_ASSOCIATION_EVENT_SCHEMA_VERSION
        );
        let founder = running.organisms.values().next().expect("founder");
        let movement_base = running
            .deterministic_policy_candidates_with_cognition(
                founder,
                founder.initial_age_ticks,
                None,
            )
            .expect("movement candidates");
        let movement_biased = running
            .deterministic_policy_candidates_with_cognition(
                founder,
                founder.initial_age_ticks,
                Some(CognitionMotorPreference {
                    action_kind: PrimitiveActionKind::Move,
                    contact_region: None,
                    signal_intensity: None,
                    movement_direction: Some(2),
                }),
            )
            .expect("direction-biased candidates");
        assert_eq!(
            movement_base
                .iter()
                .filter(|candidate| candidate.action.kind == PrimitiveActionKind::Move)
                .filter_map(|candidate| candidate.action.movement_direction)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        for (base, biased) in movement_base.iter().zip(&movement_biased) {
            let receives_bonus = base.action.kind == PrimitiveActionKind::Move
                && base.action.movement_direction == Some(2);
            assert_eq!(
                biased.weight,
                base.weight
                    + if receives_bonus {
                        COGNITION_ACTION_WEIGHT_BONUS
                    } else {
                        0
                    }
            );
        }
        let mut experienced_founder = founder.clone();
        experienced_founder.movement_direction_values = vec![MovementDirectionValueState {
            value_schema_version: MOVEMENT_DIRECTION_VALUE_SCHEMA_VERSION,
            movement_direction: 2,
            observations: 4,
            value: 128,
        }];
        let experienced = running
            .deterministic_policy_candidates_with_cognition(
                &experienced_founder,
                experienced_founder.initial_age_ticks,
                None,
            )
            .expect("direction-experienced candidates");
        for (base, candidate) in movement_base.iter().zip(&experienced) {
            let direction_bonus = if base.action.kind == PrimitiveActionKind::Move
                && base.action.movement_direction == Some(2)
            {
                16
            } else {
                0
            };
            assert_eq!(candidate.weight, base.weight + direction_bonus);
        }
        assert!((1..=10_000).any(|age_ticks| {
            running
                .deterministic_policy_action_with_cognition(
                    founder,
                    age_ticks,
                    Some(CognitionMotorPreference {
                        action_kind: PrimitiveActionKind::EmitSignal,
                        contact_region: None,
                        signal_intensity: Some(8),
                        movement_direction: None,
                    }),
                )
                .is_ok_and(|action| {
                    action.kind == PrimitiveActionKind::EmitSignal && action.intensity == 8
                })
        }));
        assert!((1..=10_000).any(|age_ticks| {
            running
                .deterministic_policy_action_with_cognition(
                    founder,
                    age_ticks,
                    Some(CognitionMotorPreference {
                        action_kind: PrimitiveActionKind::Move,
                        contact_region: None,
                        signal_intensity: None,
                        movement_direction: Some(2),
                    }),
                )
                .is_ok_and(|action| {
                    action.kind == PrimitiveActionKind::Move && action.movement_direction == Some(2)
                })
        }));
        let first_events = running
            .plan_next_tick_with_celestial_and_cognition(
                CelestialState::new(
                    TdbSecondsSinceJ2000::new(300),
                    CartesianMillimetres::new(1, 2, 3),
                    CartesianMillimetres::new(4, 5, 6),
                ),
                &[],
            )
            .expect("signal tick");
        assert_eq!(
            first_events
                .iter()
                .filter(|event| matches!(event, DomainEvent::OrganismActed { action, .. } if action.kind == PrimitiveActionKind::EmitSignal))
                .count(),
            2
        );
        let (after_signal, first_tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, first_events)
            .expect("signal tick commit");
        let signalled = after_signal
            .organisms
            .values()
            .next()
            .expect("signalled founder");
        let heard_intensity = signalled
            .recent_signal(after_signal.tick)
            .expect("directly heard amplitude");
        let unassociated_candidates = after_signal
            .deterministic_policy_candidates_with_cognition(
                signalled,
                signalled.age_ticks.expect("ruleset age"),
                None,
            )
            .expect("unassociated candidates");
        let mut motor_associated = signalled.clone();
        motor_associated.signal_action_associations = vec![SignalActionAssociationState {
            association_schema_version: SIGNAL_MOTOR_ASSOCIATION_SCHEMA_VERSION,
            signal_intensity: heard_intensity,
            action_kind: PrimitiveActionKind::Move,
            movement_direction: Some(2),
            observations: 4,
            value: 128,
        }];
        let associated_candidates = after_signal
            .deterministic_policy_candidates_with_cognition(
                &motor_associated,
                motor_associated.age_ticks.expect("ruleset age"),
                None,
            )
            .expect("motor-associated candidates");
        for (base, candidate) in unassociated_candidates.iter().zip(&associated_candidates) {
            let association_bonus = if base.action.kind == PrimitiveActionKind::Move
                && base.action.movement_direction == Some(2)
            {
                16
            } else {
                0
            };
            assert_eq!(candidate.weight, base.weight + association_bonus);
        }
        let second_events = after_signal
            .plan_next_tick_with_celestial_and_cognition(
                CelestialState::new(
                    TdbSecondsSinceJ2000::new(600),
                    CartesianMillimetres::new(2, 3, 4),
                    CartesianMillimetres::new(5, 6, 7),
                ),
                &[],
            )
            .expect("association tick");
        let movement_actions = second_events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    DomainEvent::OrganismActed { action, .. }
                        if action.kind == PrimitiveActionKind::Move
                )
            })
            .count();
        let movement_updates = second_events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    DomainEvent::OrganismMovementDirectionValueChanged { .. }
                )
            })
            .count();
        assert_eq!(movement_updates, movement_actions);
        assert_eq!(
            second_events
                .iter()
                .filter(|event| matches!(
                    event,
                    DomainEvent::OrganismSignalActionAssociationChanged { .. }
                ))
                .count(),
            2
        );
        let mut missing = second_events.clone();
        missing.retain(|event| {
            !matches!(
                event,
                DomainEvent::OrganismSignalActionAssociationChanged { .. }
            )
        });
        assert!(matches!(
            after_signal
                .clone()
                .commit(EventSequence::new(3), first_tick.batch_hash, missing),
            Err(EngineError::InvalidSignalActionAssociation(_))
        ));
        let mut redirected = second_events.clone();
        let redirected_to = redirected
            .iter_mut()
            .find_map(|event| match event {
                DomainEvent::OrganismSignalActionAssociationChanged { to, .. } => Some(to),
                _ => None,
            })
            .expect("signal association update");
        redirected_to.movement_direction = Some(
            redirected_to
                .movement_direction
                .map_or(0, |direction| (direction + 1) % 4),
        );
        assert!(
            after_signal
                .clone()
                .commit(EventSequence::new(3), first_tick.batch_hash, redirected)
                .is_err()
        );
        let (mut associated, second_tick) = after_signal
            .commit(EventSequence::new(3), first_tick.batch_hash, second_events)
            .expect("association tick commit");
        assert!(associated.organisms.values().all(|organism| {
            organism.signal_action_associations.len() == 1
                && organism.signal_action_associations[0].association_schema_version
                    == SIGNAL_MOTOR_ASSOCIATION_SCHEMA_VERSION
                && organism.signal_action_associations[0].observations == 1
        }));
        let mut batches = vec![genesis.clone(), first_tick.clone(), second_tick];
        let mut learned_direction = false;
        for _ in 0..56 {
            let next_tick = associated.tick.checked_next().expect("bounded test tick");
            let events = associated
                .plan_next_tick_with_celestial_and_cognition(
                    CelestialState::new(
                        TdbSecondsSinceJ2000::new(i128::from(next_tick.get()) * 300),
                        CartesianMillimetres::new(i128::from(next_tick.get()), 2, 3),
                        CartesianMillimetres::new(4, i128::from(next_tick.get()) + 5, 6),
                    ),
                    &[],
                )
                .expect("direction-learning tick");
            let moves = events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        DomainEvent::OrganismActed { action, .. }
                            if action.kind == PrimitiveActionKind::Move
                    )
                })
                .count();
            let updates = events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        DomainEvent::OrganismMovementDirectionValueChanged { .. }
                    )
                })
                .count();
            assert_eq!(updates, moves);
            if moves > 0 {
                let mut missing_direction_value = events.clone();
                let update_index = missing_direction_value
                    .iter()
                    .position(|event| {
                        matches!(
                            event,
                            DomainEvent::OrganismMovementDirectionValueChanged { .. }
                        )
                    })
                    .expect("movement value update");
                missing_direction_value.remove(update_index);
                assert!(matches!(
                    associated.clone().commit(
                        EventSequence::new(
                            u64::try_from(batches.len()).expect("bounded history") + 1
                        ),
                        batches.last().expect("history").batch_hash,
                        missing_direction_value,
                    ),
                    Err(EngineError::InvalidMovementDirectionValueTransition(_))
                ));
            }
            let (next, batch) = associated
                .commit(
                    EventSequence::new(u64::try_from(batches.len()).expect("bounded history") + 1),
                    batches.last().expect("history").batch_hash,
                    events,
                )
                .expect("direction-learning commit");
            associated = next;
            batches.push(batch);
            if moves > 0 {
                learned_direction = true;
                break;
            }
        }
        assert!(learned_direction);
        assert!(associated.organisms.values().any(|organism| {
            !organism.movement_direction_values.is_empty()
                && organism
                    .movement_direction_values
                    .iter()
                    .all(|entry| entry.validate().is_ok())
        }));
        let last = batches.last().expect("history tail");
        let snapshot = Snapshot::new(associated.clone(), last.sequence, last.batch_hash)
            .expect("association snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            SIGNAL_MOTOR_ASSOCIATION_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(
            replay(manifest, &batches)
                .expect("association replay")
                .state,
            associated
        );
    }

    #[test]
    fn ruleset_nineteen_leaves_only_force_caused_perceptible_surface_traces() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x124));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(13503953896175478595),
            MATERIAL_SURFACE_TRACE_RULESET_VERSION,
        );
        let mut founder =
            regulated_full_earth_person(world_id, 0x731, 10_000_000_000, 1_000_000_000);
        founder.birth_category = BirthCategory::new("female").expect("category");
        founder.reproductive_physiology =
            Some(reproductive_fixture_profile(founder.species.clone()));
        founder.heritable_disposition_profile =
            Some(heritable_fixture_profile(founder.species.clone()));
        let organism_id = founder.organism_id;
        let patch = founder.embodied_patch.expect("founder patch");

        let water = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("real water identity");
        let water_profile = OralTransferCommitment {
            commitment_schema_version: world_domain::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
            profile_id: "surface-trace-water-fixture-v1".to_owned(),
            profile_digest: Digest::sha256(b"surface trace water response"),
            material: water.clone(),
            species: human(),
            evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
            transfer_mass_milligrams: 10_000,
            recoverable_energy_joules: 100,
            hydration_recovery_seconds: 200,
        };
        let reservoir_id = EntityId::deterministic(world_id, b"surface-trace-water-reservoir");
        let reservoir = InitialMaterialInstance {
            object_id: reservoir_id,
            material: water.clone(),
            embodied_patch: patch,
            initial_mass_milligrams: Some(1_000_000),
            oral_transfer_profiles: vec![water_profile],
            reservoir: Some(MaterialReservoirCommitment {
                commitment_schema_version:
                    world_domain::MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
                profile_id: "surface-trace-water-reservoir-v1".to_owned(),
                profile_digest: Digest::sha256(b"surface trace reservoir"),
                material: water,
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                coverage_patch: patch.ancestor(10).expect("L10 coverage"),
                maximum_mass_milligrams: 1_000_000,
                replenishment_mass_milligrams_per_tick: 10_000,
            }),
        };
        let object_id = EntityId::deterministic(world_id, b"surface-trace-quartz-object");
        let quartz = InitialMaterialInstance {
            object_id,
            material: MaterialIdentity::new(
                "pubchem",
                "24261",
                "silicon dioxide",
                "https://pubchem.ncbi.nlm.nih.gov/compound/24261",
            )
            .expect("real silicon-dioxide identity"),
            embodied_patch: patch,
            initial_mass_milligrams: Some(100_000),
            oral_transfer_profiles: Vec::new(),
            reservoir: None,
        };

        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis_with_materials(
                environmental_provisional_full_earth_configuration(),
                vec![founder],
                vec![reservoir, quartz],
            )
            .expect("surface-trace genesis plan");
        let (mut state, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("surface-trace genesis");
        assert_eq!(
            genesis.event_schema_version,
            MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION
        );
        let mut batches = vec![genesis];
        let mut traced_transition = None;
        for tick in 1..=2_000_u64 {
            let celestial = CelestialState::new(
                TdbSecondsSinceJ2000::new(i128::from(tick * 300)),
                CartesianMillimetres::new(i128::from(tick), 2, 3),
                CartesianMillimetres::new(4, i128::from(tick) + 5, 6),
            );
            let events = state
                .plan_next_tick_with_celestial_and_cognition(celestial, &[])
                .expect("surface-trace tick plan");
            let has_trace = events.iter().any(|event| {
                matches!(
                    event,
                    DomainEvent::MaterialSurfaceTraceChanged {
                        object_id: event_object,
                        organism_id: event_organism,
                        ..
                    } if *event_object == object_id && *event_organism == organism_id
                )
            });
            let sequence =
                EventSequence::new(u64::try_from(batches.len()).expect("bounded history") + 1);
            let previous_hash = batches.last().expect("genesis exists").batch_hash;
            let before = state.clone();
            let (next, batch) = state
                .commit(sequence, previous_hash, events.clone())
                .expect("surface-trace tick commit");
            state = next;
            batches.push(batch);
            if has_trace {
                traced_transition = Some((before, sequence, previous_hash, events));
                break;
            }
        }
        let (before_trace, trace_sequence, previous_hash, trace_events) =
            traced_transition.expect("seeded freeform policy eventually applies force to quartz");
        let instance = state
            .material_instances
            .get(&object_id)
            .expect("traced object remains present");
        assert!(instance.surface_trace_units() > 0);
        let memory = &state
            .organisms
            .get(&organism_id)
            .expect("founder")
            .perception_memory;
        assert!(memory.iter().any(|entry| {
            entry.subject_id == Some(object_id)
                && entry.channel == PerceptionChannel::Touch
                && entry.property_code == "surface_trace"
                && entry.quantized_value
                    == i32::try_from(instance.surface_trace_units()).expect("bounded trace")
        }));

        let mut missing_perception = trace_events;
        missing_perception.retain(|event| {
            !matches!(
                event,
                DomainEvent::OrganismPerceived { perception, .. }
                    if perception.subject_id == Some(object_id)
                        && perception.readings.iter().any(|reading| reading.property_code == "surface_trace")
            )
        });
        assert!(matches!(
            before_trace.commit(trace_sequence, previous_hash, missing_perception),
            Err(EngineError::InvalidMaterialSurfaceTraceEventSet)
        ));

        let snapshot = Snapshot::new(
            state.clone(),
            batches.last().expect("trace batch").sequence,
            batches.last().expect("trace batch").batch_hash,
        )
        .expect("surface-trace snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            MATERIAL_SURFACE_TRACE_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(
            replay(manifest, &batches)
                .expect("surface-trace replay")
                .state,
            state
        );
    }

    #[test]
    fn ruleset_twenty_retains_selectable_label_free_surface_arrangements() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x125));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(13503953896175478596),
            MATERIAL_SURFACE_REGIONS_RULESET_VERSION,
        );
        let mut founder =
            regulated_full_earth_person(world_id, 0x732, 10_000_000_000, 1_000_000_000);
        founder.birth_category = BirthCategory::new("female").expect("category");
        founder.reproductive_physiology =
            Some(reproductive_fixture_profile(founder.species.clone()));
        founder.heritable_disposition_profile =
            Some(heritable_fixture_profile(founder.species.clone()));
        let organism_id = founder.organism_id;
        let patch = founder.embodied_patch.expect("founder patch");
        let water = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("water identity");
        let reservoir = InitialMaterialInstance {
            object_id: EntityId::deterministic(world_id, b"surface-regions-water"),
            material: water.clone(),
            embodied_patch: patch,
            initial_mass_milligrams: Some(1_000_000),
            oral_transfer_profiles: vec![OralTransferCommitment {
                commitment_schema_version: world_domain::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
                profile_id: "surface-regions-water-v1".to_owned(),
                profile_digest: Digest::sha256(b"surface regions water response"),
                material: water.clone(),
                species: human(),
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                transfer_mass_milligrams: 10_000,
                recoverable_energy_joules: 100,
                hydration_recovery_seconds: 200,
            }],
            reservoir: Some(MaterialReservoirCommitment {
                commitment_schema_version:
                    world_domain::MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
                profile_id: "surface-regions-reservoir-v1".to_owned(),
                profile_digest: Digest::sha256(b"surface regions reservoir"),
                material: water,
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                coverage_patch: patch.ancestor(10).expect("L10 coverage"),
                maximum_mass_milligrams: 1_000_000,
                replenishment_mass_milligrams_per_tick: 10_000,
            }),
        };
        let object_id = EntityId::deterministic(world_id, b"surface-regions-quartz");
        let quartz = InitialMaterialInstance {
            object_id,
            material: MaterialIdentity::new(
                "pubchem",
                "24261",
                "silicon dioxide",
                "https://pubchem.ncbi.nlm.nih.gov/compound/24261",
            )
            .expect("silicon-dioxide identity"),
            embodied_patch: patch,
            initial_mass_milligrams: Some(100_000),
            oral_transfer_profiles: Vec::new(),
            reservoir: None,
        };
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis_with_materials(
                environmental_provisional_full_earth_configuration(),
                vec![founder],
                vec![reservoir, quartz],
            )
            .expect("surface-regions genesis plan");
        let (mut state, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("surface-regions genesis");
        assert_eq!(
            genesis.event_schema_version,
            MATERIAL_SURFACE_REGIONS_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            state
                .material_instances
                .get(&object_id)
                .expect("object")
                .surface_region_trace_units(),
            &[0; MATERIAL_SURFACE_REGION_COUNT]
        );
        let mut batches = vec![genesis];
        let mut traced_transition = None;
        for tick in 1..=2_000_u64 {
            let celestial = CelestialState::new(
                TdbSecondsSinceJ2000::new(i128::from(tick * 300)),
                CartesianMillimetres::new(i128::from(tick), 2, 3),
                CartesianMillimetres::new(4, i128::from(tick) + 5, 6),
            );
            let events = state
                .plan_next_tick_with_celestial_and_cognition(celestial, &[])
                .expect("surface-regions tick plan");
            let has_trace = events.iter().any(|event| {
                matches!(
                    event,
                    DomainEvent::MaterialSurfaceRegionTraceChanged {
                        object_id: event_object,
                        organism_id: event_organism,
                        contact_region: 0..=7,
                        ..
                    } if *event_object == object_id && *event_organism == organism_id
                )
            });
            let sequence =
                EventSequence::new(u64::try_from(batches.len()).expect("bounded history") + 1);
            let previous_hash = batches.last().expect("genesis exists").batch_hash;
            let before = state.clone();
            let (next, batch) = state
                .commit(sequence, previous_hash, events.clone())
                .expect("surface-regions tick commit");
            state = next;
            batches.push(batch);
            if has_trace {
                traced_transition = Some((before, sequence, previous_hash, events));
                break;
            }
        }
        let (before_trace, sequence, previous_hash, trace_events) =
            traced_transition.expect("freeform policy eventually selects a surface region");
        let instance = state.material_instances.get(&object_id).expect("object");
        assert_eq!(instance.surface_region_trace_units().len(), 8);
        assert_eq!(
            instance
                .surface_region_trace_units()
                .iter()
                .copied()
                .sum::<u32>(),
            instance.surface_trace_units()
        );
        assert!(instance.surface_trace_units() > 0);
        let organism = state.organisms.get(&organism_id).expect("founder");
        let base_candidates = state
            .deterministic_policy_candidates_with_cognition(organism, 1, None)
            .expect("surface-region candidates");
        let region_biased = state
            .deterministic_policy_candidates_with_cognition(
                organism,
                1,
                Some(CognitionMotorPreference {
                    action_kind: PrimitiveActionKind::ApplyForce,
                    contact_region: Some(3),
                    signal_intensity: None,
                    movement_direction: None,
                }),
            )
            .expect("region-biased candidates");
        assert_eq!(base_candidates.len(), region_biased.len());
        for (base, biased) in base_candidates.iter().zip(&region_biased) {
            let receives_bonus = base.action.kind == PrimitiveActionKind::ApplyForce
                && base.action.contact_region == Some(3);
            assert_eq!(
                biased.weight,
                base.weight
                    + if receives_bonus {
                        COGNITION_ACTION_WEIGHT_BONUS
                    } else {
                        0
                    }
            );
        }
        assert_eq!(
            base_candidates
                .iter()
                .filter(|candidate| candidate.action.kind == PrimitiveActionKind::ApplyForce)
                .count(),
            MATERIAL_SURFACE_REGION_COUNT
        );
        let mut acoustic_state = state.clone();
        acoustic_state.manifest.ruleset_version = ACOUSTIC_VARIATION_RULESET_VERSION;
        let acoustic_organism = acoustic_state
            .organisms
            .get(&organism_id)
            .expect("acoustic founder");
        let acoustic_base = acoustic_state
            .deterministic_policy_candidates_with_cognition(acoustic_organism, 1, None)
            .expect("acoustic candidates");
        let acoustic_biased = acoustic_state
            .deterministic_policy_candidates_with_cognition(
                acoustic_organism,
                1,
                Some(CognitionMotorPreference {
                    action_kind: PrimitiveActionKind::EmitSignal,
                    contact_region: None,
                    signal_intensity: Some(6),
                    movement_direction: None,
                }),
            )
            .expect("intensity-biased acoustic candidates");
        assert_eq!(
            acoustic_base
                .iter()
                .filter(|candidate| candidate.action.kind == PrimitiveActionKind::EmitSignal)
                .map(|candidate| candidate.action.intensity)
                .collect::<Vec<_>>(),
            (1..=SIGNAL_INTENSITY_VARIANT_COUNT).collect::<Vec<_>>()
        );
        assert_eq!(
            acoustic_base
                .iter()
                .filter(|candidate| candidate.action.kind == PrimitiveActionKind::EmitSignal)
                .count(),
            usize::from(world_domain::SIGNAL_FORM_VARIANT_COUNT),
            "the world exposes a bounded vocabulary of physically distinct, semantically blank forms"
        );
        for (base, biased) in acoustic_base.iter().zip(&acoustic_biased) {
            let receives_bonus =
                base.action.kind == PrimitiveActionKind::EmitSignal && base.action.intensity == 6;
            assert_eq!(
                biased.weight,
                base.weight
                    + if receives_bonus {
                        COGNITION_ACTION_WEIGHT_BONUS
                    } else {
                        0
                    }
            );
        }
        assert!(
            state
                .organisms
                .get(&organism_id)
                .expect("founder")
                .perception_memory
                .iter()
                .any(|entry| entry.subject_id == Some(object_id)
                    && entry.property_code.starts_with("surface_region_"))
        );

        let mut missing_perception = trace_events;
        missing_perception.retain(|event| {
            !matches!(
                event,
                DomainEvent::OrganismPerceived { perception, .. }
                    if perception.subject_id == Some(object_id)
                        && perception.readings.iter().any(|reading| reading.property_code.starts_with("surface_region_"))
            )
        });
        assert!(matches!(
            before_trace.commit(sequence, previous_hash, missing_perception),
            Err(EngineError::InvalidMaterialSurfaceRegionEventSet)
        ));
        let snapshot = Snapshot::new(
            state.clone(),
            batches.last().expect("trace batch").sequence,
            batches.last().expect("trace batch").batch_hash,
        )
        .expect("surface-regions snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            MATERIAL_SURFACE_REGIONS_SNAPSHOT_SCHEMA_VERSION
        );
        assert_eq!(
            replay(manifest, &batches)
                .expect("surface-regions replay")
                .state,
            state
        );
    }

    #[test]
    fn cognition_deadline_is_required_replayable_and_only_biases_valid_actions() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x122));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(13503953896175478593),
            COGNITION_RULESET_VERSION,
        );
        let mut founder = regulated_full_earth_person(world_id, 0x712, 10_000_000_000, 10_000_000);
        founder.birth_category = BirthCategory::new("female").expect("category");
        founder.initial_age_ticks = 20;
        founder.reproductive_physiology =
            Some(reproductive_fixture_profile(founder.species.clone()));
        founder.heritable_disposition_profile =
            Some(heritable_fixture_profile(founder.species.clone()));
        let organism_id = founder.organism_id;

        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![founder],
            )
            .expect("cognition genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("cognition genesis");
        let selection_events = running
            .plan_cognition_request(organism_id)
            .expect("cognition request");
        let selection = match &selection_events[0] {
            DomainEvent::CognitionRequestSelected { selection } => selection.clone(),
            _ => unreachable!(),
        };
        let (mut pending, selection_batch) = running
            .commit(EventSequence::new(2), genesis.batch_hash, selection_events)
            .expect("cognition selection commit");

        let organism = pending.organisms.get(&organism_id).expect("founder");
        let base = pending
            .deterministic_policy_candidates_with_cognition(organism, 21, None)
            .expect("baseline candidates");
        let biased = pending
            .deterministic_policy_candidates_with_cognition(
                organism,
                21,
                Some(CognitionMotorPreference {
                    action_kind: PrimitiveActionKind::Rest,
                    contact_region: None,
                    signal_intensity: None,
                    movement_direction: None,
                }),
            )
            .expect("biased candidates");
        assert_eq!(base.len(), biased.len());
        for (base, biased) in base.iter().zip(&biased) {
            assert_eq!(base.action, biased.action);
            let expected = if base.action.kind == PrimitiveActionKind::Rest {
                base.weight + COGNITION_ACTION_WEIGHT_BONUS
            } else {
                base.weight
            };
            assert_eq!(biased.weight, expected);
        }

        let early = CognitionDeadlineInput::unavailable(
            &selection,
            Digest::ZERO,
            Digest::ZERO,
            Digest::ZERO,
            CognitionUnavailableReason::DeadlineNoResult,
        )
        .expect("valid unavailable input");
        assert!(matches!(
            pending.plan_next_tick_with_celestial_and_cognition(
                CelestialState::new(
                    TdbSecondsSinceJ2000::new(300),
                    CartesianMillimetres::new(1, 2, 3),
                    CartesianMillimetres::new(4, 5, 6),
                ),
                &[early],
            ),
            Err(EngineError::UnexpectedCognitionInput)
        ));

        let mut batches = vec![genesis, selection_batch];
        let mut previous_hash = batches.last().expect("selection batch").batch_hash;
        let mut next_sequence = 3_u64;
        for tick in 1..COGNITION_RESPONSE_WINDOW_TICKS {
            let celestial = CelestialState::new(
                TdbSecondsSinceJ2000::new(i128::from(tick * 300)),
                CartesianMillimetres::new(i128::from(tick), 2, 3),
                CartesianMillimetres::new(4, i128::from(tick) + 5, 6),
            );
            let events = pending
                .plan_next_tick_with_celestial(celestial)
                .expect("pre-deadline tick");
            let (next, batch) = pending
                .commit(EventSequence::new(next_sequence), previous_hash, events)
                .expect("pre-deadline commit");
            previous_hash = batch.batch_hash;
            next_sequence += 1;
            batches.push(batch);
            pending = next;
        }
        assert_eq!(
            pending.tick(),
            SimTick::new(COGNITION_RESPONSE_WINDOW_TICKS - 1)
        );
        let deadline_celestial = CelestialState::new(
            TdbSecondsSinceJ2000::new(i128::from(COGNITION_RESPONSE_WINDOW_TICKS * 300)),
            CartesianMillimetres::new(60, 2, 3),
            CartesianMillimetres::new(4, 65, 6),
        );
        assert!(matches!(
            pending.plan_next_tick_with_celestial(deadline_celestial),
            Err(EngineError::CognitionInputRequired)
        ));
        let input = CognitionDeadlineInput::unavailable(
            &selection,
            Digest::ZERO,
            Digest::ZERO,
            Digest::ZERO,
            CognitionUnavailableReason::DeadlineNoResult,
        )
        .expect("deadline fallback input");
        let events = pending
            .plan_next_tick_with_celestial_and_cognition(
                deadline_celestial,
                std::slice::from_ref(&input),
            )
            .expect("deadline tick with input");
        assert!(matches!(
            events.get(1),
            Some(DomainEvent::CognitionInputRecorded { input: recorded }) if recorded == &input
        ));
        let (resolved, deadline_batch) = pending
            .commit(EventSequence::new(next_sequence), previous_hash, events)
            .expect("deadline input commit");
        assert!(resolved.pending_cognition_requests.is_empty());
        batches.push(deadline_batch);
        assert_eq!(
            replay(manifest, &batches)
                .expect("cognition deadline replay")
                .state,
            resolved
        );

        let death_events = running
            .commit(
                EventSequence::new(2),
                batches[0].batch_hash,
                running
                    .plan_cognition_request(organism_id)
                    .expect("second branch request"),
            )
            .expect("second branch selection")
            .0
            .plan_death(
                organism_id,
                DeathCause {
                    mechanism: "test_mechanical_unavailability".to_owned(),
                },
            )
            .expect("early subject death");
        assert!(death_events.iter().any(|event| matches!(
            event,
            DomainEvent::CognitionInputRecorded { input }
                if matches!(
                    input.outcome,
                    CognitionInputOutcome::Unavailable {
                        reason: CognitionUnavailableReason::SubjectUnavailable
                    }
                )
        )));
    }

    #[test]
    fn ruleset_thirteen_preserves_legacy_post_genesis_initialization_replay() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x119));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(13503953896175478588),
            ACTION_LEARNING_RULESET_VERSION,
        );
        let founder = regulated_full_earth_person(world_id, 0x611, 10_000_000_000, 10_000_000);
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![founder.clone()],
            )
            .expect("legacy genesis plan");
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("legacy genesis");
        let mut later_founder = founder;
        later_founder.organism_id = EntityId::from_uuid(Uuid::from_u128(0x612));
        let initialization = DomainEvent::OrganismInitialized {
            organism_id: later_founder.organism_id,
            species: later_founder.species,
            role: later_founder.role,
            birth_category: later_founder.birth_category,
            initial_age_ticks: later_founder.initial_age_ticks,
            location_id: later_founder.location_id,
            embodied_patch: later_founder.embodied_patch,
            metabolic_rate: later_founder.metabolic_rate,
            physiological_regulation: later_founder.physiological_regulation,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
            heritable_disposition: None,
        };
        let (with_legacy_initialization, initialization_batch) = running
            .commit(
                EventSequence::new(2),
                genesis.batch_hash,
                vec![initialization],
            )
            .expect("legacy initialization remains accepted");
        assert_eq!(with_legacy_initialization.organisms().count(), 2);
        assert_eq!(
            replay(manifest, &[genesis, initialization_batch])
                .expect("legacy initialization replay")
                .state,
            with_legacy_initialization
        );
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
    fn atmospheric_flux_ruleset_emits_canonically_ordered_physical_readings() {
        let mut manifest = manifest();
        manifest.ruleset_version = LOCAL_ATMOSPHERIC_FLUX_RULESET_VERSION;
        let mut person =
            regulated_full_earth_person(manifest.world_id, 0x1281, 10_000_000_000, 10_000_000);
        person.birth_category = BirthCategory::new("female").expect("category");
        person.reproductive_physiology = Some(reproductive_fixture_profile(person.species.clone()));
        person.heritable_disposition_profile =
            Some(heritable_fixture_profile(person.species.clone()));
        let patch = person.embodied_patch.expect("founder patch");
        let water = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("real water identity");
        let material = InitialMaterialInstance {
            object_id: EntityId::deterministic(manifest.world_id, b"flux-test-water"),
            material: water.clone(),
            embodied_patch: patch,
            initial_mass_milligrams: Some(1_000_000),
            oral_transfer_profiles: vec![OralTransferCommitment {
                commitment_schema_version: world_domain::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
                profile_id: "flux-test-water-oral-v1".to_owned(),
                profile_digest: Digest::sha256(b"flux test water oral fixture"),
                material: water.clone(),
                species: person.species.clone(),
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                transfer_mass_milligrams: 1,
                recoverable_energy_joules: 1,
                hydration_recovery_seconds: 1,
            }],
            reservoir: Some(MaterialReservoirCommitment {
                commitment_schema_version:
                    world_domain::MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
                profile_id: "flux-test-water-reservoir-v1".to_owned(),
                profile_digest: Digest::sha256(b"flux test water reservoir fixture"),
                material: water,
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                coverage_patch: patch.ancestor(10).expect("L10 coverage"),
                maximum_mass_milligrams: 2_000_000,
                replenishment_mass_milligrams_per_tick: 1,
            }),
        };
        let initial = EngineState::new(manifest.clone());
        let (running, genesis) = initial
            .commit(
                EventSequence::new(1),
                Digest::ZERO,
                initial
                    .plan_configured_genesis_with_materials(
                        weather_provisional_full_earth_configuration(),
                        vec![person],
                        vec![material],
                    )
                    .expect("atmospheric-flux genesis"),
            )
            .expect("commit atmospheric-flux genesis");
        let events = running
            .plan_next_tick_with_celestial(CelestialState::new(
                TdbSecondsSinceJ2000::new(123),
                CartesianMillimetres::new(1, 2, 3),
                CartesianMillimetres::new(4, 5, 6),
            ))
            .expect("atmospheric-flux tick");
        let readings = events.iter().find_map(|event| match event {
            DomainEvent::OrganismPerceived { perception, .. } if perception.readings.len() == 3 => {
                Some(&perception.readings)
            }
            _ => None,
        });
        assert_eq!(
            readings
                .expect("combined physical perception")
                .iter()
                .map(|reading| reading.property_code.as_str())
                .collect::<Vec<_>>(),
            ["air_motion", "temperature", "water_flux"]
        );
        let (after_tick, tick) = running
            .commit(EventSequence::new(2), genesis.batch_hash, events)
            .expect("commit atmospheric-flux tick");
        assert_eq!(
            tick.event_schema_version,
            LOCAL_ATMOSPHERIC_FLUX_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            replay(manifest, &[genesis, tick])
                .expect("atmospheric-flux replay")
                .state,
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
                        reproductive_physiology: None,
                        heritable_disposition_profile: None,
                        heritable_disposition: None,
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

    #[test]
    fn terrain_relief_adds_only_proportional_movement_exposure() {
        assert_eq!(
            terrain_adjusted_movement_exposure(1_000, 5, 5).expect("flat relief"),
            1_000
        );
        assert_eq!(
            terrain_adjusted_movement_exposure(1_000_000, 2_228_633, 2_364_719)
                .expect("canonical relief"),
            1_136_086
        );
        assert!(terrain_adjusted_movement_exposure(1, 10, 9).is_err());
    }

    #[test]
    fn topsoil_coarse_fragments_add_only_proportional_movement_exposure() {
        let mut quantiles = [[1, 2, 3]; 9];
        quantiles[2] = [0, 30, 574];
        assert_eq!(
            topsoil_adjusted_movement_exposure(1_000_000, &quantiles)
                .expect("canonical topsoil coarse-fragment median"),
            1_030_000
        );
        quantiles[2][1] = -1;
        assert!(topsoil_adjusted_movement_exposure(1, &quantiles).is_err());
        quantiles[2][1] = 1_001;
        assert!(topsoil_adjusted_movement_exposure(1, &quantiles).is_err());
    }

    #[test]
    fn ruleset_twenty_nine_binds_terrain_and_makes_movement_privately_costlier() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x129));
        let terrain_manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(0x005e_ed29),
            TERRAIN_MOVEMENT_RULESET_VERSION,
        );
        let regulated_manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(0x005e_ed29),
            BODILY_REGULATION_RULESET_VERSION,
        );
        let initial = EngineState::new(regulated_manifest);
        let person = regulated_full_earth_person(world_id, 0x229, 60_000, 60_000);
        let events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![person.clone()],
            )
            .expect("regulated genesis");
        let (mut running, _) = initial
            .commit(EventSequence::new(1), Digest::ZERO, events)
            .expect("regulated commit");
        running.manifest = terrain_manifest.clone();
        running.configuration = Some(surface_provisional_full_earth_configuration());
        assert_eq!(
            running.state_hash_schema_version(),
            TERRAIN_MOVEMENT_STATE_HASH_SCHEMA_VERSION
        );
        let organism = running
            .organisms
            .get(&person.organism_id)
            .expect("initialized person");
        let ordinary = running
            .next_bodily_regulation(
                organism,
                &PrimitiveAction {
                    kind: PrimitiveActionKind::ApplyForce,
                    target_id: None,
                    intensity: 1,
                    contact_region: None,
                    movement_direction: None,
                },
                OralRecovery::default(),
            )
            .expect("ordinary exposure");
        let moving = running
            .next_bodily_regulation(
                organism,
                &PrimitiveAction {
                    kind: PrimitiveActionKind::Move,
                    target_id: None,
                    intensity: 1,
                    contact_region: None,
                    movement_direction: Some(0),
                },
                OralRecovery::default(),
            )
            .expect("terrain movement exposure");
        assert!(
            moving.fatigue_load_second_squared > ordinary.fatigue_load_second_squared,
            "terrain affects only the private bodily outcome"
        );
        let empty_terrain_state = EngineState::new(terrain_manifest);
        assert_eq!(
            empty_terrain_state.event_schema_version(),
            TERRAIN_MOVEMENT_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            latest_ruleset_event_schema_for_replay(&empty_terrain_state),
            Some(TERRAIN_MOVEMENT_EVENT_SCHEMA_VERSION)
        );
        let snapshot = Snapshot::new(empty_terrain_state, EventSequence::ZERO, Digest::ZERO)
            .expect("terrain snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            TERRAIN_MOVEMENT_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("terrain snapshot integrity");
    }

    #[test]
    fn ruleset_thirty_binds_topsoil_and_stacks_only_private_movement_cost() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x130));
        let topsoil_manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(0x005e_ed30),
            TOPSOIL_MOVEMENT_RULESET_VERSION,
        );
        let regulated_manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(0x005e_ed30),
            BODILY_REGULATION_RULESET_VERSION,
        );
        let initial = EngineState::new(regulated_manifest);
        let person = regulated_full_earth_person(world_id, 0x230, 60_000, 60_000);
        let events = initial
            .plan_configured_genesis(
                environmental_provisional_full_earth_configuration(),
                vec![person.clone()],
            )
            .expect("regulated genesis");
        let (mut running, _) = initial
            .commit(EventSequence::new(1), Digest::ZERO, events)
            .expect("regulated commit");
        running.manifest = topsoil_manifest.clone();
        running.configuration = Some(surface_provisional_full_earth_configuration());

        let organism = running
            .organisms
            .get(&person.organism_id)
            .expect("initialized person");
        let moving = running
            .next_bodily_regulation(
                organism,
                &PrimitiveAction {
                    kind: PrimitiveActionKind::Move,
                    target_id: None,
                    intensity: 1,
                    contact_region: None,
                    movement_direction: Some(0),
                },
                OralRecovery::default(),
            )
            .expect("topsoil movement exposure");
        assert_eq!(moving.fatigue_load_second_squared, 204_903);

        let empty_topsoil_state = EngineState::new(topsoil_manifest);
        assert_eq!(
            empty_topsoil_state.event_schema_version(),
            TOPSOIL_MOVEMENT_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            empty_topsoil_state.state_hash_schema_version(),
            TOPSOIL_MOVEMENT_STATE_HASH_SCHEMA_VERSION
        );
        assert_eq!(
            latest_ruleset_event_schema_for_replay(&empty_topsoil_state),
            Some(TOPSOIL_MOVEMENT_EVENT_SCHEMA_VERSION)
        );
        let snapshot = Snapshot::new(empty_topsoil_state, EventSequence::ZERO, Digest::ZERO)
            .expect("topsoil snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            TOPSOIL_MOVEMENT_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("topsoil snapshot integrity");
    }

    #[test]
    fn mass_scaled_metabolism_ruleset_has_distinct_replay_schemas() {
        let state = EngineState::new(WorldManifest::new(
            WorldId::from_uuid(Uuid::from_u128(0x31)),
            WorldSeed::new(0x31),
            MASS_SCALED_METABOLISM_RULESET_VERSION,
        ));
        assert_eq!(
            state.event_schema_version(),
            MASS_SCALED_METABOLISM_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            state.state_hash_schema_version(),
            MASS_SCALED_METABOLISM_STATE_HASH_SCHEMA_VERSION
        );
        assert_eq!(
            latest_ruleset_event_schema_for_replay(&state),
            Some(MASS_SCALED_METABOLISM_EVENT_SCHEMA_VERSION)
        );
        let snapshot = Snapshot::new(state, EventSequence::ZERO, Digest::ZERO)
            .expect("mass-scaled metabolism snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            MASS_SCALED_METABOLISM_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("mass-scaled metabolism snapshot integrity");
    }

    #[test]
    fn adult_body_mass_is_canonical_event_snapshot_and_replay_state() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x32));
        let manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(0x32),
            ADULT_BODY_MASS_STATE_RULESET_VERSION,
        );
        let mut founder = regulated_full_earth_person(world_id, 0x3201, 604_800, 86_400);
        founder.initial_age_ticks = 20;
        founder.adult_body_mass = Some(adult_body_mass_fixture(founder.species.clone()));
        founder.reproductive_physiology =
            Some(reproductive_fixture_profile(founder.species.clone()));
        founder.heritable_disposition_profile =
            Some(heritable_fixture_profile(founder.species.clone()));
        let founder_id = founder.organism_id;
        let patch = founder.embodied_patch.expect("founder patch");
        let water = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("water identity");
        let material_id = EntityId::deterministic(world_id, b"mass-state-water-reservoir");
        let reservoir = InitialMaterialInstance {
            object_id: material_id,
            material: water.clone(),
            embodied_patch: patch,
            initial_mass_milligrams: Some(1_000_000),
            oral_transfer_profiles: vec![OralTransferCommitment {
                commitment_schema_version: world_domain::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
                profile_id: "mass-state-water-oral-v1".to_owned(),
                profile_digest: Digest::sha256(b"mass state water oral profile"),
                material: water.clone(),
                species: human(),
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                transfer_mass_milligrams: 700_000,
                recoverable_energy_joules: 0,
                hydration_recovery_seconds: 21_600,
            }],
            reservoir: Some(MaterialReservoirCommitment {
                commitment_schema_version:
                    world_domain::MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
                profile_id: "mass-state-water-reservoir-v1".to_owned(),
                profile_digest: Digest::sha256(b"mass state water reservoir"),
                material: water,
                evidence_basis: world_domain::OralTransferEvidenceBasis::EngineeringAssumption,
                coverage_patch: patch.ancestor(10).expect("coverage patch"),
                maximum_mass_milligrams: 1_000_000,
                replenishment_mass_milligrams_per_tick: 1_000,
            }),
        };
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial
            .plan_configured_genesis_with_materials(
                surface_provisional_full_earth_configuration(),
                vec![founder],
                vec![reservoir],
            )
            .expect("ruleset-32 genesis plan");
        assert!(genesis_events.iter().any(|event| matches!(
            event,
            DomainEvent::OrganismAdultBodyMassCommitted { organism_id, .. }
                if *organism_id == founder_id
        )));
        let (running, genesis) = initial
            .commit(EventSequence::new(1), Digest::ZERO, genesis_events)
            .expect("ruleset-32 genesis commit");
        assert_eq!(
            genesis.event_schema_version,
            ADULT_BODY_MASS_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            running
                .organisms
                .get(&founder_id)
                .and_then(|organism| organism.adult_body_mass.as_ref())
                .map(|commitment| commitment.mass_grams_value),
            Some(70_000)
        );
        assert_eq!(
            replay(manifest, std::slice::from_ref(&genesis))
                .expect("ruleset-32 replay")
                .state,
            running
        );
        let snapshot = Snapshot::new(running, genesis.sequence, genesis.batch_hash)
            .expect("ruleset-32 snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            ADULT_BODY_MASS_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("ruleset-32 snapshot integrity");
    }

    #[test]
    fn adult_body_mass_ruleset_has_distinct_replay_schemas() {
        let state = EngineState::new(WorldManifest::new(
            WorldId::from_uuid(Uuid::from_u128(0x3200)),
            WorldSeed::new(0x3200),
            ADULT_BODY_MASS_STATE_RULESET_VERSION,
        ));
        assert_eq!(
            state.event_schema_version(),
            ADULT_BODY_MASS_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            state.state_hash_schema_version(),
            ADULT_BODY_MASS_STATE_HASH_SCHEMA_VERSION
        );
        assert_eq!(
            latest_ruleset_event_schema_for_replay(&state),
            Some(ADULT_BODY_MASS_EVENT_SCHEMA_VERSION)
        );
        let snapshot = Snapshot::new(state, EventSequence::ZERO, Digest::ZERO)
            .expect("adult-body-mass snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            ADULT_BODY_MASS_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("adult-body-mass snapshot integrity");
    }

    #[test]
    fn close_kin_exclusion_covers_direct_shared_and_first_cousin_ancestry() {
        let legacy_manifest = manifest();
        let initial = EngineState::new(legacy_manifest.clone());
        let founder = initial_person(legacy_manifest.world_id);
        let founder_a = founder.organism_id;
        let (mut state, _) = initial
            .commit(
                EventSequence::new(1),
                Digest::ZERO,
                initial.plan_genesis(vec![founder]).expect("genesis plan"),
            )
            .expect("genesis commit");
        state.manifest = WorldManifest::new(
            legacy_manifest.world_id,
            legacy_manifest.seed,
            CLOSE_KIN_EXCLUSION_RULESET_VERSION,
        );

        let founder_b = EntityId::deterministic(state.world_id(), b"kin-founder-b");
        let unrelated_a = EntityId::deterministic(state.world_id(), b"kin-unrelated-a");
        let unrelated_b = EntityId::deterministic(state.world_id(), b"kin-unrelated-b");
        let sibling_a = EntityId::deterministic(state.world_id(), b"kin-sibling-a");
        let sibling_b = EntityId::deterministic(state.world_id(), b"kin-sibling-b");
        let cousin_a = EntityId::deterministic(state.world_id(), b"kin-cousin-a");
        let cousin_b = EntityId::deterministic(state.world_id(), b"kin-cousin-b");
        let template = state.organisms[&founder_a].clone();
        for (organism_id, parent_ids) in [
            (founder_b, vec![]),
            (unrelated_a, vec![]),
            (unrelated_b, vec![]),
            (sibling_a, vec![founder_a, founder_b]),
            (sibling_b, vec![founder_a, founder_b]),
            (cousin_a, vec![sibling_a, unrelated_a]),
            (cousin_b, vec![sibling_b, unrelated_b]),
        ] {
            let mut organism = template.clone();
            organism.organism_id = organism_id;
            organism.parent_ids = parent_ids;
            state.organisms.insert(organism_id, organism);
        }

        assert!(!state.are_close_kin(&state.organisms[&founder_a], &state.organisms[&unrelated_a]));
        assert!(state.are_close_kin(&state.organisms[&sibling_a], &state.organisms[&cousin_a]));
        assert!(state.are_close_kin(&state.organisms[&sibling_a], &state.organisms[&sibling_b]));
        assert!(state.are_close_kin(&state.organisms[&sibling_b], &state.organisms[&cousin_a]));
        assert!(state.are_close_kin(&state.organisms[&cousin_a], &state.organisms[&cousin_b]));
    }

    #[test]
    fn stateless_rulesets_reuse_the_latest_compatible_schemas() {
        for ruleset_version in [
            CLOSE_KIN_EXCLUSION_RULESET_VERSION,
            SIGNAL_CONVENTION_REUSE_RULESET_VERSION,
            LOCAL_INTERACTION_RULESET_VERSION,
        ] {
            let state = EngineState::new(WorldManifest::new(
                WorldId::from_uuid(Uuid::from_u128(0x3300 + u128::from(ruleset_version))),
                WorldSeed::new(u64::from(ruleset_version)),
                ruleset_version,
            ));
            assert_eq!(
                state.event_schema_version(),
                ADULT_BODY_MASS_EVENT_SCHEMA_VERSION
            );
            assert_eq!(
                state.state_hash_schema_version(),
                ADULT_BODY_MASS_STATE_HASH_SCHEMA_VERSION
            );
            assert_eq!(
                latest_ruleset_event_schema_for_replay(&state),
                Some(ADULT_BODY_MASS_EVENT_SCHEMA_VERSION)
            );
            let snapshot = Snapshot::new(state, EventSequence::ZERO, Digest::ZERO)
                .expect("stateless-ruleset snapshot");
            assert_eq!(
                snapshot.snapshot_schema_version,
                ADULT_BODY_MASS_SNAPSHOT_SCHEMA_VERSION
            );
            snapshot
                .verify_integrity()
                .expect("stateless-ruleset snapshot integrity");
        }
    }

    #[test]
    fn cancer_research_bootstrap_is_ruleset_gated_and_hash_distinct() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0xCA4CE2));
        let mut legacy_manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(37),
            GROUNDED_PREDICTIVE_COGNITION_RULESET_VERSION,
        );
        legacy_manifest.experiment = Some(world_domain::WorldExperimentCommitment::CancerResearch(
            world_domain::CancerResearchBootstrap::english_literate_abundant_world(),
        ));
        assert!(matches!(
            EngineState::new(legacy_manifest).validate(),
            Err(EngineError::WorldExperimentRequiresNewerRuleset)
        ));

        let mut manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(37),
            CANCER_RESEARCH_WORLD_RULESET_VERSION,
        );
        manifest.experiment = Some(world_domain::WorldExperimentCommitment::CancerResearch(
            world_domain::CancerResearchBootstrap::english_literate_abundant_world(),
        ));
        let state = EngineState::new(manifest);
        state
            .validate()
            .expect("ruleset thirty-seven experiment state");
        assert_eq!(
            state.event_schema_version(),
            CANCER_RESEARCH_COHORT_EVENT_SCHEMA_VERSION
        );
        assert_eq!(
            state.state_hash_schema_version(),
            CANCER_RESEARCH_COHORT_STATE_HASH_SCHEMA_VERSION
        );
        let snapshot =
            Snapshot::new(state, EventSequence::ZERO, Digest::ZERO).expect("experiment snapshot");
        assert_eq!(
            snapshot.snapshot_schema_version,
            CANCER_RESEARCH_COHORT_SNAPSHOT_SCHEMA_VERSION
        );
        snapshot
            .verify_integrity()
            .expect("valid experiment snapshot");
    }

    #[test]
    fn cancer_research_cohort_is_exact_stratified_and_seed_deterministic() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0xCA4CE3));
        let people = (0..1_000_u32)
            .map(|ordinal| {
                (
                    EntityId::deterministic(
                        world_id,
                        format!("cancer-resident-{ordinal:04}").as_bytes(),
                    ),
                    if ordinal.is_multiple_of(2) {
                        "female"
                    } else {
                        "male"
                    },
                )
            })
            .collect::<Vec<_>>();

        let first = seeded_cancer_research_cohort(WorldSeed::new(37), people.iter().copied())
            .expect("valid initial cohort");
        let repeated = seeded_cancer_research_cohort(WorldSeed::new(37), people.iter().copied())
            .expect("repeat cohort");
        let other_seed = seeded_cancer_research_cohort(WorldSeed::new(38), people.iter().copied())
            .expect("other-seed cohort");

        assert_eq!(first, repeated);
        assert_ne!(first, other_seed);
        assert_eq!(first.len(), 500);
        let affected = first.into_iter().collect::<BTreeSet<_>>();
        let affected_female = people
            .iter()
            .filter(|(resident_id, category)| {
                *category == "female" && affected.contains(resident_id)
            })
            .count();
        let affected_male = people
            .iter()
            .filter(|(resident_id, category)| *category == "male" && affected.contains(resident_id))
            .count();
        assert_eq!((affected_female, affected_male), (250, 250));
    }

    #[test]
    fn cancer_biology_seeds_the_exact_cohort_and_advances_replay_stably() {
        let template_manifest = manifest();
        let template_initial = EngineState::new(template_manifest.clone());
        let (template_state, _) = template_initial
            .commit(
                EventSequence::new(1),
                Digest::ZERO,
                template_initial
                    .plan_genesis(vec![initial_person(template_manifest.world_id)])
                    .expect("template genesis plan"),
            )
            .expect("template genesis");
        let template = template_state
            .organisms
            .values()
            .next()
            .expect("template person")
            .clone();

        let world_id = WorldId::from_uuid(Uuid::from_u128(0xCA4CE4));
        let mut manifest =
            WorldManifest::new(world_id, WorldSeed::new(38), CANCER_BIOLOGY_RULESET_VERSION);
        manifest.experiment = Some(WorldExperimentCommitment::CancerResearch(
            world_domain::CancerResearchBootstrap::english_literate_abundant_world(),
        ));
        let mut state = EngineState::new(manifest);
        for ordinal in 0..CANCER_RESEARCH_INITIAL_RESIDENTS {
            let mut person = template.clone();
            person.organism_id = EntityId::deterministic(
                world_id,
                format!("cancer-biology-resident-{ordinal:04}").as_bytes(),
            );
            person.birth_category = BirthCategory::new(if ordinal.is_multiple_of(2) {
                "female"
            } else {
                "male"
            })
            .expect("valid birth category");
            state.organisms.insert(person.organism_id, person);
        }
        let affected = seeded_cancer_research_cohort(
            state.manifest.seed,
            state
                .organisms
                .values()
                .map(|organism| (organism.organism_id, organism.birth_category.as_str())),
        )
        .expect("seeded cohort");
        state
            .apply_event(&DomainEvent::CancerResearchCohortCommitted {
                affected_resident_ids: affected,
            })
            .expect("cohort and burdens");
        assert_eq!(state.cancer_burdens.len(), 500);
        assert!(state.cancer_burdens.values().all(|burden| {
            burden.target == world_domain::CancerResearchTarget::AdultGlioblastoma
                && burden.observed_at == SimTick::ZERO
        }));

        state.configuration = Some(provisional_full_earth_configuration());
        state.status = WorldStatus::Running;
        state.tick = SimTick::new(288);
        let events = state
            .plan_cancer_burden_events()
            .expect("first daily burden transitions");
        let mut first = state.clone();
        let mut repeated = state;
        first.apply_events(&events).expect("first application");
        repeated
            .apply_events(&events)
            .expect("replayed application");
        assert_eq!(first.cancer_burdens, repeated.cancer_burdens);
        assert_eq!(
            first.state_hash().expect("first hash"),
            repeated.state_hash().expect("replay hash")
        );
        assert!(
            first
                .cancer_burdens
                .values()
                .all(|burden| burden.observed_at == SimTick::new(288))
        );
    }
}
