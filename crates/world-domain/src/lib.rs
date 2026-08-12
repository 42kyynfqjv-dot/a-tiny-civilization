//! Durable, replay-safe domain primitives shared by the engine and its adapters.

mod cancer;
mod celestial;
mod cognition;
mod configuration;
mod embodiment;
mod environment;
mod event;
mod geographic;
mod hash;
mod heredity;
mod identity;
mod illumination;
mod manifest;
mod material;
mod reproduction;
mod research;
mod solar;
mod spatial;
mod tide;
mod time;

pub use cancer::{
    CANCER_BURDEN_PARTS_PER_MILLION_MAX, CANCER_BURDEN_SCHEMA_VERSION,
    CANCER_TERMINAL_BURDEN_PARTS_PER_MILLION, CancerBurdenError, CancerBurdenState,
    CancerBurdenTransition, CancerTrajectory,
};
pub use celestial::{
    CartesianAxis, CartesianMillimetres, CelestialError, CelestialState, TdbSecondsSinceJ2000,
    TickDurationSeconds, tdb_seconds_at_tick,
};
pub use cognition::{
    COGNITION_INPUT_SCHEMA_VERSION, COGNITION_SELECTION_SCHEMA_VERSION, CognitionContractError,
    CognitionDeadlineInput, CognitionInputOutcome, CognitionModelEvidence, CognitionReading,
    CognitionRequestSelection, CognitionUnavailableReason, MAX_COGNITION_RECALL_TOKENS,
    MAX_COGNITION_SELECTION_OUTPUT_TOKENS, MAX_COGNITION_SELECTION_QUERY_BYTES,
    MAX_COGNITION_SELECTION_READINGS, cognition_request_id,
};
pub use configuration::{
    CapacityExhaustionPolicy, EarthResolutionLevels, ExecutionScale, FullEarthGrid,
    LEGACY_WORLD_CONFIGURATION_SCHEMA_VERSION,
    PROVISIONAL_ENVIRONMENT_WORLD_CONFIGURATION_SCHEMA_VERSION,
    PROVISIONAL_SURFACE_WORLD_CONFIGURATION_SCHEMA_VERSION,
    PROVISIONAL_WEATHER_WORLD_CONFIGURATION_SCHEMA_VERSION,
    PROVISIONAL_WORLD_CONFIGURATION_SCHEMA_VERSION, PartitionedExecution, PersonRepresentation,
    ProvisionalWorldCompositionReference, S2Projection, SchedulerKind, SpatialGrid,
    WORLD_CONFIGURATION_SCHEMA_VERSION, WorldConfiguration, WorldConfigurationError,
    WorldDataBundleReference, WorldGeometry, WorldInputReference,
};
pub use embodiment::{
    ACTION_VALUE_MAX, ACTION_VALUE_MIN, ACTION_VALUE_STATE_SCHEMA_VERSION,
    ADULT_BODY_MASS_COMMITMENT_SCHEMA_VERSION, ActionValueState, AdultBodyMassCommitment,
    BodilyNeedState, BodilyRegulationState, COMPETITIVE_SIGNAL_ASSOCIATION_SCHEMA_VERSION,
    EmbodimentError, LEGACY_METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION,
    METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION, MOVEMENT_DIRECTION_VALUE_SCHEMA_VERSION,
    MetabolicRateCommitment, MovementDirectionValueState, NeedKind, NeedSignal,
    PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION, PerceptionChannel,
    PhysiologicalEvidenceBasis, PhysiologicalRegulationCommitment, PrimitiveAction,
    PrimitiveActionKind, PropertyReading, SIGNAL_ACTION_ASSOCIATION_SCHEMA_VERSION,
    SIGNAL_FORM_VARIANT_COUNT, SIGNAL_MOTOR_ASSOCIATION_SCHEMA_VERSION,
    SignalActionAssociationState, SituatedPerception,
};
pub use environment::{
    LocalEnvironmentError, NORMAL_YEAR_PHASE_COUNT, ProvisionalLocalEnvironmentBaseline,
    ProvisionalLocalSurfaceBaseline, ProvisionalLocalWeatherBaseline,
};
pub use event::{
    ACTION_LEARNING_EVENT_SCHEMA_VERSION, ADULT_BODY_MASS_EVENT_SCHEMA_VERSION,
    BODILY_REGULATION_EVENT_SCHEMA_VERSION, BODY_PROVENANCE_EVENT_SCHEMA_VERSION, BirthCategory,
    CANCER_BURDEN_EVENT_SCHEMA_VERSION, CANCER_RESEARCH_COHORT_EVENT_SCHEMA_VERSION,
    CELESTIAL_STATE_EVENT_SCHEMA_VERSION, COGNITION_EVENT_SCHEMA_VERSION,
    COMPETITIVE_SIGNAL_LEARNING_EVENT_SCHEMA_VERSION, CONFIGURED_EVENT_SCHEMA_VERSION,
    CategoryError, DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION, DeathCause, DomainEvent,
    EMBODIED_POSITION_EVENT_SCHEMA_VERSION, EVENT_SCHEMA_VERSION, EventBatch, EventBatchError,
    EventRecord, HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION, LEGACY_EVENT_SCHEMA_VERSION,
    LOCAL_ATMOSPHERIC_FLUX_EVENT_SCHEMA_VERSION, LOCAL_WEATHER_EVENT_SCHEMA_VERSION,
    MASS_SCALED_METABOLISM_EVENT_SCHEMA_VERSION, MATERIAL_HANDLING_EVENT_SCHEMA_VERSION,
    MATERIAL_INGESTION_EVENT_SCHEMA_VERSION, MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION,
    MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION, MATERIAL_SURFACE_REGIONS_EVENT_SCHEMA_VERSION,
    MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION, MOVEMENT_DIRECTION_LEARNING_EVENT_SCHEMA_VERSION,
    OrganismRole, PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION,
    REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION, SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION,
    SELECTABLE_MOVEMENT_EVENT_SCHEMA_VERSION, SIGNAL_ACTION_ASSOCIATION_EVENT_SCHEMA_VERSION,
    SIGNAL_MOTOR_ASSOCIATION_EVENT_SCHEMA_VERSION, SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION,
    SOCIAL_LEARNING_EVENT_SCHEMA_VERSION, TERRAIN_MOVEMENT_EVENT_SCHEMA_VERSION,
    TOPSOIL_MOVEMENT_EVENT_SCHEMA_VERSION, WORLD_EXPERIMENT_EVENT_SCHEMA_VERSION,
    WORLD_SUCCESSOR_RETIREMENT_EVENT_SCHEMA_VERSION,
};
pub use geographic::{
    GeographicCoordinateE7, GeographicCoordinateHalfArcsecond, GeographicRoutingError, S2FaceIj,
    S2FaceRay, S2FaceUv, decode_s2_face_ij, route_geographic_to_s2, route_half_arcsecond_to_s2,
    s2_edge_neighbors, s2_face_ij_center_uv, s2_face_ij_vertex_uv, s2_face_uv_to_ray,
    s2_ray_to_geographic_e7,
};
pub use hash::{CanonicalHashError, Digest};
pub use heredity::{
    HERITABLE_ACTION_KINDS, HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
    HERITABLE_DISPOSITION_SCHEMA_VERSION, HERITABLE_PROBABILITY_SCALE, HeredityError,
    HeritableActionWeight, HeritableDisposition, HeritableDispositionProfile,
    MAX_HERITABLE_ACTION_WEIGHT_RATIO, MAX_HERITABLE_MUTATION_PROBABILITY_MILLIONTHS,
};
pub use identity::{EntityId, EventId, WorldId};
pub use illumination::{
    EarthFixedSunVector, EcefSurfacePosition, IlluminationGeometryError, LocalIlluminationGeometry,
    RadialHorizonClassification, SunVectorFrame,
};
pub use manifest::{
    CANCER_RESEARCH_BOOTSTRAP_SCHEMA_VERSION, CANCER_RESEARCH_INITIAL_AFFECTED_RESIDENTS,
    CANCER_RESEARCH_INITIAL_RESIDENTS, CancerCohortAssignment, CancerPerception,
    CancerResearchBootstrap, CancerResearchTarget, CancerTerminalObjective, DiseaseScope,
    ObjectivePriority, ResearchCognitionRoute, ResearchCommunication, ResearchDiversity,
    ResearchEvidenceProtocol, ResearchLanguage, SpeciesIdentity, SpeciesIdentityError,
    SurvivalEnvironment, WorldExperimentCommitment, WorldManifest, WorldManifestError,
};
pub use material::{
    MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION, MaterialIdentity, MaterialIdentityError,
    MaterialReservoirCommitment, MaterialReservoirCommitmentError,
    ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION, OralTransferCommitment, OralTransferCommitmentError,
    OralTransferEvidenceBasis,
};
pub use reproduction::{
    LEGACY_REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION, OffspringCategoryWeight,
    REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION, REPRODUCTIVE_PROBABILITY_SCALE,
    ReproductionError, ReproductiveCategoryMaturityCommitment, ReproductiveCategoryPair,
    ReproductiveDevelopmentEnd, ReproductivePhysiologyCommitment,
};
pub use research::{
    CAMPAIGN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION,
    CANCER_NCI60_RESPONSE_PREDICTION_SCHEMA_VERSION,
    CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION,
    CANCER_NCI60_RESPONSE_QUALIFICATION_SCHEMA_VERSION,
    CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION, CANCER_RESEARCH_NOVELTY_AUDIT_SCHEMA_VERSION,
    CANCER_RESEARCH_NOVELTY_METHOD_VERSION, CANCER_RESEARCH_PROFILE_SCHEMA_VERSION,
    CANCER_RESEARCH_TURN_SCHEMA_VERSION, CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION,
    CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION, CANCER_VIRTUAL_LAB_METHOD_VERSION,
    CANCER_VIRTUAL_MECHANISTIC_READOUT_SCHEMA_VERSION, CancerNci60CnsLine, CancerNci60ObservedRank,
    CancerNci60ResponsePrediction, CancerNci60ResponseQualification, CancerNciInterventionIdentity,
    CancerResearchArtifactKind, CancerResearchClaim, CancerResearchContractError,
    CancerResearchContribution, CancerResearchEvidenceKind, CancerResearchEvidenceReference,
    CancerResearchInferenceTier, CancerResearchNoveltyAudit, CancerResearchNoveltyMatch,
    CancerResearchNoveltyStatus, CancerResearchProfile, CancerResearchProgram,
    CancerResearchSpecialty, CancerResearchStage, CancerResearchTask, CancerResearchTurnSelection,
    CancerVirtualCalibrationGrade, CancerVirtualCloneFractions, CancerVirtualEndpoint,
    CancerVirtualExperimentInterpretation, CancerVirtualExperimentPlan,
    CancerVirtualExperimentResult, CancerVirtualInterventionModality, CancerVirtualLabFidelity,
    CancerVirtualMechanismTarget, CancerVirtualMechanisticReadout, CancerVirtualPkReadout,
    CancerVirtualSubjectModel, LEGACY_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION,
    LEGACY_CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION, MAX_CANCER_RESEARCH_NOVELTY_MATCHES,
    MAX_CANCER_RESEARCH_NOVELTY_QUERY_TERMS, MAX_CANCER_RESEARCH_NOVELTY_WARNINGS,
    MAX_RESEARCH_CITATIONS, MAX_RESEARCH_CLAIMS, MAX_RESEARCH_EVIDENCE_REFERENCES,
    MAX_RESEARCH_MODEL_OUTPUT_TOKENS, VIRTUAL_PLAN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION,
    cancer_research_request_id,
};
pub use solar::{
    CanonicalPositiveRational, CanonicalPositiveRationalError, PinnedSolarReferenceDistance,
    SolarDistanceForcing, SolarDistanceForcingError,
};
pub use spatial::{MAX_S2_LEVEL, S2CellId, S2CellIdError};
pub use tide::{
    SignedSquaredMillimetres, SquaredMillimetres, TideBody, TideBodyGeometry, TideGeometry,
    TideGeometryError,
};
pub use time::{EventSequence, SequenceOverflow, SimTick, TimeOverflow, WorldSeed};

use serde::{Deserialize, Serialize};

/// Durable lifecycle state for a world.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldStatus {
    Initializing,
    Running,
    Extinct,
    Archived,
    /// A populated historical world deliberately closed in favor of a disclosed
    /// successor. This is distinct from both extinction and a natural archive.
    Retired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_status_uses_stable_snake_case_json() {
        let encoded = serde_json::to_string(&WorldStatus::Initializing);
        assert!(matches!(encoded.as_deref(), Ok("\"initializing\"")));
    }
}
