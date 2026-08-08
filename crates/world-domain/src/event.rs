use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;

use crate::{
    ActionValueState, BodilyRegulationState, CanonicalHashError, CelestialState,
    CognitionDeadlineInput, CognitionRequestSelection, Digest, EntityId, EventId, EventSequence,
    HeritableDisposition, HeritableDispositionProfile, MaterialIdentity,
    MaterialReservoirCommitment, MetabolicRateCommitment, OralTransferCommitment,
    PhysiologicalRegulationCommitment, PrimitiveAction, ReproductiveDevelopmentEnd,
    ReproductivePhysiologyCommitment, S2CellId, SimTick, SituatedPerception, SpeciesIdentity,
    WorldConfiguration, WorldId, WorldManifest,
};

pub const LEGACY_EVENT_SCHEMA_VERSION: u16 = 1;
pub const CONFIGURED_EVENT_SCHEMA_VERSION: u16 = 2;
pub const EVENT_SCHEMA_VERSION: u16 = 3;
/// Adds durable S2 embodied-patch positions and movement facts for full-Earth worlds.
pub const EMBODIED_POSITION_EVENT_SCHEMA_VERSION: u16 = 4;
/// Adds an explicitly non-admitted provisional full-Earth configuration input.
pub const PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION: u16 = 5;
/// Adds internal, deterministic organism body-clock transitions for ruleset-two
/// worlds. These facts remain observer-neutral and expose no sensitive mechanism.
pub const SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION: u16 = 6;
/// Adds one pinned, replayable celestial source result per ruleset-three tick.
/// The raw source evaluator remains outside the engine; this event is its exact,
/// hash-chained hand-off into canonical history.
pub const CELESTIAL_STATE_EVENT_SCHEMA_VERSION: u16 = 7;
/// Adds source-pinned organism body provenance. This is an evidentiary commitment,
/// not a claim that metabolism or ecology is scientifically complete.
pub const BODY_PROVENANCE_EVENT_SCHEMA_VERSION: u16 = 8;
/// Adds cited real-world material instances at durable full-Earth patches.
pub const MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION: u16 = 9;
/// Adds resolved neutral handling transitions for material instances.
pub const MATERIAL_HANDLING_EVENT_SCHEMA_VERSION: u16 = 10;
/// Adds deterministic same-patch propagation of neutral emitted signals.
pub const SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION: u16 = 11;
/// Adds source-addressed bodily regulation and exact pressure transitions.
pub const BODILY_REGULATION_EVENT_SCHEMA_VERSION: u16 = 12;
/// Changes live action selection to the seeded, situated baseline policy. No new
/// payload is added, but the semantic boundary is explicit for audit and replay.
pub const DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION: u16 = 13;
/// Adds retained material mass, species-bound oral-transfer commitments, and exact
/// resolved oral mass transfers.
pub const MATERIAL_INGESTION_EVENT_SCHEMA_VERSION: u16 = 14;
/// Adds one bounded, label-free action/outcome association per scheduled organism
/// transition.
pub const ACTION_LEARNING_EVENT_SCHEMA_VERSION: u16 = 15;
/// Adds species-bound reproductive commitments, neutral pending development, and
/// development-bound births.
pub const REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION: u16 = 16;
/// Adds immutable species-bound disposition profiles and deterministic inherited
/// individual action weights.
pub const HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION: u16 = 17;
/// Adds deterministic world-total external-cognition selection facts. Responses are
/// admitted separately at their fixed deadline and replay never calls a provider.
pub const COGNITION_EVENT_SCHEMA_VERSION: u16 = 18;
/// Adds spatially anchored, bounded real-material reservoirs and deterministic
/// replenishment transitions. Quantitative ecology remains evidence-labelled.
pub const MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION: u16 = 19;
/// Adds one bounded, direct same-patch observation of another organism's neutral
/// primitive action per observer tick. No inferred purpose or cultural label exists.
pub const SOCIAL_LEARNING_EVENT_SCHEMA_VERSION: u16 = 20;
/// Adds durable, label-free surface traces produced by primitive force. The trace
/// scalar is a provisional physical response, never an artifact or use label.
pub const MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION: u16 = 21;

/// Engine-level participation tier. This is never exposed as an agent concept.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganismRole {
    Person,
    Fauna,
}

/// Versioned, species-aware category used by reproduction and later observer matching.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct BirthCategory(String);

impl BirthCategory {
    pub fn new(value: impl Into<String>) -> Result<Self, CategoryError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err(CategoryError);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for BirthCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("birth category must be a non-empty lowercase ASCII slug")]
pub struct CategoryError;

/// A versioned physical mortality mechanism code, not observer prose.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeathCause {
    pub mechanism: String,
}

/// Facts that can change canonical world state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum DomainEvent {
    WorldStarted {
        manifest: WorldManifest,
    },
    WorldConfigured {
        configuration: WorldConfiguration,
    },
    OrganismInitialized {
        organism_id: EntityId,
        species: SpeciesIdentity,
        role: OrganismRole,
        birth_category: BirthCategory,
        initial_age_ticks: u64,
        location_id: Option<EntityId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embodied_patch: Option<S2CellId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metabolic_rate: Option<MetabolicRateCommitment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        physiological_regulation: Option<PhysiologicalRegulationCommitment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reproductive_physiology: Option<ReproductivePhysiologyCommitment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        heritable_disposition_profile: Option<HeritableDispositionProfile>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        heritable_disposition: Option<HeritableDisposition>,
    },
    /// A physical material instance. Its identity is citable, but its affordances
    /// and effects are intentionally not inferred by this event.
    MaterialInstanceInitialized {
        object_id: EntityId,
        material: MaterialIdentity,
        embodied_patch: S2CellId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        initial_mass_milligrams: Option<u64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        oral_transfer_profiles: Vec<OralTransferCommitment>,
    },
    /// Commits one material instance as a spatially anchored, replenishing source.
    /// This is private world physics and conveys no use label to an organism.
    MaterialReservoirCommitted {
        object_id: EntityId,
        commitment: MaterialReservoirCommitment,
    },
    /// A material instance became physically held after a neutral grasp action.
    MaterialInstanceHeld {
        object_id: EntityId,
        holder_id: EntityId,
    },
    /// A held material instance was released at the holder's current patch.
    MaterialInstanceReleased {
        object_id: EntityId,
        holder_id: EntityId,
        embodied_patch: S2CellId,
    },
    /// A held object accumulated a bounded physical surface trace after its holder
    /// applied primitive force. This records no tool, mark, symbol, or intended use.
    MaterialSurfaceTraceChanged {
        object_id: EntityId,
        organism_id: EntityId,
        from_trace_units: u32,
        applied_force_units: u16,
        to_trace_units: u32,
    },
    /// Exact physical mass transferred from a held material through one organism's
    /// mouth. The event says nothing about desirability, safety, knowledge, or use.
    MaterialOralPortionTransferred {
        object_id: EntityId,
        organism_id: EntityId,
        profile_digest: Digest,
        from_mass_milligrams: u64,
        transferred_mass_milligrams: u64,
        to_mass_milligrams: u64,
    },
    /// One ordered oral withdrawal from a shared reservoir, including exact lazy
    /// replenishment since that reservoir's previous committed settlement.
    MaterialReservoirOralPortionTransferred {
        object_id: EntityId,
        organism_id: EntityId,
        profile_digest: Digest,
        settled_from_tick: SimTick,
        settled_to_tick: SimTick,
        from_mass_milligrams: u64,
        replenished_mass_milligrams: u64,
        transferred_mass_milligrams: u64,
        to_mass_milligrams: u64,
    },
    TickAdvanced {
        from: SimTick,
        to: SimTick,
    },
    OrganismBorn {
        organism_id: EntityId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        development_id: Option<EntityId>,
        species: SpeciesIdentity,
        role: OrganismRole,
        birth_category: BirthCategory,
        parent_ids: Vec<EntityId>,
        location_id: Option<EntityId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embodied_patch: Option<S2CellId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metabolic_rate: Option<MetabolicRateCommitment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        physiological_regulation: Option<PhysiologicalRegulationCommitment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reproductive_physiology: Option<ReproductivePhysiologyCommitment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        heritable_disposition_profile: Option<HeritableDispositionProfile>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        heritable_disposition: Option<HeritableDisposition>,
    },
    /// A private physiological process began. It carries only canonical identifiers,
    /// timing, and a committed profile—not prose or agent-facing reproductive labels.
    ReproductiveDevelopmentStarted {
        development_id: EntityId,
        offspring_id: EntityId,
        species: SpeciesIdentity,
        role: OrganismRole,
        birth_category: BirthCategory,
        parent_ids: Vec<EntityId>,
        developing_parent_id: EntityId,
        profile_digest: Digest,
        due_tick: SimTick,
        parents_available_at: SimTick,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        heritable_disposition_profile: Option<HeritableDispositionProfile>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offspring_heritable_disposition: Option<HeritableDisposition>,
    },
    /// A pending private process ended without a birth.
    ReproductiveDevelopmentEnded {
        development_id: EntityId,
        developing_parent_id: EntityId,
        reason: ReproductiveDevelopmentEnd,
    },
    OrganismDied {
        organism_id: EntityId,
        cause: DeathCause,
    },
    /// Direct label-free sensory evidence available to one living organism.
    OrganismPerceived {
        organism_id: EntityId,
        perception: SituatedPerception,
    },
    /// One chosen use-neutral bodily operation. World physics resolves its effect.
    OrganismActed {
        organism_id: EntityId,
        action: PrimitiveAction,
    },
    /// A resolved physical relocation between discrete full-Earth embodied patches.
    OrganismMoved {
        organism_id: EntityId,
        from_patch: S2CellId,
        to_patch: S2CellId,
    },
    /// One resolved body-clock increment. The event is internal causal state, not
    /// a public claim about an organism's life or capabilities.
    OrganismAgeAdvanced {
        organism_id: EntityId,
        from_age_ticks: u64,
        to_age_ticks: u64,
    },
    /// One exact, replayable bodily-pressure transition. These normalized internal
    /// values are never an observer narrative or a label for a solution.
    OrganismNeedsChanged {
        organism_id: EntityId,
        from: BodilyRegulationState,
        to: BodilyRegulationState,
    },
    /// One bounded update derived from the organism's own primitive action and total
    /// bodily-pressure change. It carries no material, use, or observer label.
    OrganismActionValueChanged {
        organism_id: EntityId,
        from: Option<ActionValueState>,
        to: ActionValueState,
    },
    /// One attention-bounded direct observation of another organism's primitive
    /// bodily operation. The value is private learning state, not an observer claim.
    OrganismSocialActionValueChanged {
        observer_id: EntityId,
        actor_id: EntityId,
        from: Option<ActionValueState>,
        to: ActionValueState,
    },
    /// One world-total, simulated-time-budgeted optional cognition request. It
    /// contains only body-owned situated inputs and cannot directly change state.
    CognitionRequestSelected {
        selection: CognitionRequestSelection,
    },
    /// One immutable fixed-deadline external input, or an explicit unavailable
    /// outcome. Replay consumes this event and never calls memory or model services.
    CognitionInputRecorded {
        input: CognitionDeadlineInput,
    },
    /// A source-backed Sun/Moon state for the current simulation tick. This is a
    /// physical input only; observer projections must not expose its mechanism.
    CelestialStateRecorded {
        state: CelestialState,
    },
    WorldExtinct,
    WorldArchived,
}

/// An event plus its deterministic identity within a committed batch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventRecord {
    pub event_id: EventId,
    pub index: u32,
    pub event: DomainEvent,
}

/// One atomic deterministic transition and its tamper-evident hash-chain link.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventBatch {
    pub event_schema_version: u16,
    pub world_id: WorldId,
    pub sequence: EventSequence,
    pub tick: SimTick,
    pub ruleset_version: u32,
    pub previous_hash: Digest,
    pub events: Vec<EventRecord>,
    pub post_state_hash: Digest,
    pub batch_hash: Digest,
}

impl EventBatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_schema_version: u16,
        world_id: WorldId,
        sequence: EventSequence,
        tick: SimTick,
        ruleset_version: u32,
        previous_hash: Digest,
        events: Vec<DomainEvent>,
        post_state_hash: Digest,
    ) -> Result<Self, EventBatchError> {
        validate_schema_version(event_schema_version)?;
        for event in &events {
            validate_event_for_schema(event_schema_version, event)?;
        }
        if sequence == EventSequence::ZERO {
            return Err(EventBatchError::ZeroSequence);
        }
        if ruleset_version == 0 {
            return Err(EventBatchError::ZeroRulesetVersion);
        }
        if events.is_empty() {
            return Err(EventBatchError::Empty);
        }

        let records = events
            .into_iter()
            .enumerate()
            .map(|(index, event)| {
                let index = u32::try_from(index).map_err(|_| EventBatchError::TooManyEvents)?;
                Ok(EventRecord {
                    event_id: EventId::for_position(world_id, sequence.get(), index),
                    index,
                    event,
                })
            })
            .collect::<Result<Vec<_>, EventBatchError>>()?;

        let mut batch = Self {
            event_schema_version,
            world_id,
            sequence,
            tick,
            ruleset_version,
            previous_hash,
            events: records,
            post_state_hash,
            batch_hash: Digest::ZERO,
        };
        batch.batch_hash = batch.calculate_hash()?;
        Ok(batch)
    }

    pub fn verify_integrity(&self) -> Result<(), EventBatchError> {
        validate_schema_version(self.event_schema_version)?;
        for record in &self.events {
            validate_event_for_schema(self.event_schema_version, &record.event)?;
        }
        if self.sequence == EventSequence::ZERO {
            return Err(EventBatchError::ZeroSequence);
        }
        if self.ruleset_version == 0 {
            return Err(EventBatchError::ZeroRulesetVersion);
        }
        if self.events.is_empty() {
            return Err(EventBatchError::Empty);
        }

        for (expected_index, record) in self.events.iter().enumerate() {
            let expected_index =
                u32::try_from(expected_index).map_err(|_| EventBatchError::TooManyEvents)?;
            let expected_id =
                EventId::for_position(self.world_id, self.sequence.get(), expected_index);
            if record.index != expected_index || record.event_id != expected_id {
                return Err(EventBatchError::InvalidEventIdentity {
                    index: expected_index,
                });
            }
        }

        let calculated = self.calculate_hash()?;
        if calculated != self.batch_hash {
            return Err(EventBatchError::HashMismatch {
                expected: self.batch_hash,
                calculated,
            });
        }
        Ok(())
    }

    fn calculate_hash(&self) -> Result<Digest, CanonicalHashError> {
        Digest::canonical(&BatchHashMaterial {
            event_schema_version: self.event_schema_version,
            world_id: self.world_id,
            sequence: self.sequence,
            tick: self.tick,
            ruleset_version: self.ruleset_version,
            previous_hash: self.previous_hash,
            events: &self.events,
            post_state_hash: self.post_state_hash,
        })
    }
}

fn validate_schema_version(event_schema_version: u16) -> Result<(), EventBatchError> {
    if !matches!(
        event_schema_version,
        LEGACY_EVENT_SCHEMA_VERSION
            | CONFIGURED_EVENT_SCHEMA_VERSION
            | EVENT_SCHEMA_VERSION
            | EMBODIED_POSITION_EVENT_SCHEMA_VERSION
            | PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION
            | SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION
            | CELESTIAL_STATE_EVENT_SCHEMA_VERSION
            | BODY_PROVENANCE_EVENT_SCHEMA_VERSION
            | MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION
            | MATERIAL_HANDLING_EVENT_SCHEMA_VERSION
            | SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION
            | BODILY_REGULATION_EVENT_SCHEMA_VERSION
            | DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION
            | MATERIAL_INGESTION_EVENT_SCHEMA_VERSION
            | ACTION_LEARNING_EVENT_SCHEMA_VERSION
            | REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION
            | HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION
            | COGNITION_EVENT_SCHEMA_VERSION
            | MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION
            | SOCIAL_LEARNING_EVENT_SCHEMA_VERSION
            | MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION
    ) {
        return Err(EventBatchError::UnsupportedSchema(event_schema_version));
    }
    Ok(())
}

fn validate_event_for_schema(
    event_schema_version: u16,
    event: &DomainEvent,
) -> Result<(), EventBatchError> {
    if event_schema_version == LEGACY_EVENT_SCHEMA_VERSION
        && matches!(event, DomainEvent::WorldConfigured { .. })
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::OrganismPerceived { .. } | DomainEvent::OrganismActed { .. }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < EMBODIED_POSITION_EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::OrganismMoved { .. }
                | DomainEvent::OrganismInitialized {
                    embodied_patch: Some(_),
                    ..
                }
                | DomainEvent::OrganismBorn {
                    embodied_patch: Some(_),
                    ..
                }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::WorldConfigured { configuration }
                if configuration.is_provisional_execution()
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < CELESTIAL_STATE_EVENT_SCHEMA_VERSION
        && matches!(event, DomainEvent::CelestialStateRecorded { .. })
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < SCHEDULED_CAUSAL_EVENT_SCHEMA_VERSION
        && matches!(event, DomainEvent::OrganismAgeAdvanced { .. })
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < BODY_PROVENANCE_EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::OrganismInitialized {
                metabolic_rate: Some(_),
                ..
            } | DomainEvent::OrganismBorn {
                metabolic_rate: Some(_),
                ..
            }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < MATERIAL_INSTANCE_EVENT_SCHEMA_VERSION
        && matches!(event, DomainEvent::MaterialInstanceInitialized { .. })
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < MATERIAL_HANDLING_EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::MaterialInstanceHeld { .. } | DomainEvent::MaterialInstanceReleased { .. }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION
        && matches!(event, DomainEvent::MaterialSurfaceTraceChanged { .. })
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < BODILY_REGULATION_EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::OrganismNeedsChanged { .. }
                | DomainEvent::OrganismInitialized {
                    physiological_regulation: Some(_),
                    ..
                }
                | DomainEvent::OrganismBorn {
                    physiological_regulation: Some(_),
                    ..
                }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    let has_material_ingestion_commitment = match event {
        DomainEvent::MaterialInstanceInitialized {
            initial_mass_milligrams,
            oral_transfer_profiles,
            ..
        } => initial_mass_milligrams.is_some() || !oral_transfer_profiles.is_empty(),
        _ => false,
    };
    if event_schema_version < MATERIAL_INGESTION_EVENT_SCHEMA_VERSION
        && has_material_ingestion_commitment
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < MATERIAL_INGESTION_EVENT_SCHEMA_VERSION
        && matches!(event, DomainEvent::MaterialOralPortionTransferred { .. })
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < ACTION_LEARNING_EVENT_SCHEMA_VERSION
        && matches!(event, DomainEvent::OrganismActionValueChanged { .. })
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::ReproductiveDevelopmentStarted { .. }
                | DomainEvent::ReproductiveDevelopmentEnded { .. }
                | DomainEvent::OrganismInitialized {
                    reproductive_physiology: Some(_),
                    ..
                }
                | DomainEvent::OrganismBorn {
                    development_id: Some(_),
                    ..
                }
                | DomainEvent::OrganismBorn {
                    reproductive_physiology: Some(_),
                    ..
                }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::OrganismInitialized {
                heritable_disposition_profile: Some(_),
                ..
            } | DomainEvent::OrganismInitialized {
                heritable_disposition: Some(_),
                ..
            } | DomainEvent::OrganismBorn {
                heritable_disposition_profile: Some(_),
                ..
            } | DomainEvent::OrganismBorn {
                heritable_disposition: Some(_),
                ..
            } | DomainEvent::ReproductiveDevelopmentStarted {
                heritable_disposition_profile: Some(_),
                ..
            } | DomainEvent::ReproductiveDevelopmentStarted {
                offspring_heritable_disposition: Some(_),
                ..
            }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < COGNITION_EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::CognitionRequestSelected { .. }
                | DomainEvent::CognitionInputRecorded { .. }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::MaterialReservoirCommitted { .. }
                | DomainEvent::MaterialReservoirOralPortionTransferred { .. }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < SOCIAL_LEARNING_EVENT_SCHEMA_VERSION
        && matches!(event, DomainEvent::OrganismSocialActionValueChanged { .. })
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    match event {
        DomainEvent::OrganismInitialized {
            species,
            metabolic_rate,
            physiological_regulation,
            reproductive_physiology,
            heritable_disposition_profile,
            heritable_disposition,
            ..
        }
        | DomainEvent::OrganismBorn {
            species,
            metabolic_rate,
            physiological_regulation,
            reproductive_physiology,
            heritable_disposition_profile,
            heritable_disposition,
            ..
        } => {
            if let Some(metabolic_rate) = metabolic_rate {
                metabolic_rate
                    .validate()
                    .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?;
            }
            if let Some(regulation) = physiological_regulation {
                regulation
                    .validate()
                    .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?;
            }
            if let Some(reproduction) = reproductive_physiology {
                reproduction
                    .validate()
                    .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?;
            }
            match (heritable_disposition_profile, heritable_disposition) {
                (Some(profile), Some(disposition)) => {
                    if profile.species != *species {
                        return Err(EventBatchError::InvalidEmbodiedEvent(
                            "heritable-disposition profile species does not match organism"
                                .to_owned(),
                        ));
                    }
                    disposition.validate_against(profile).map_err(|error| {
                        EventBatchError::InvalidEmbodiedEvent(error.to_string())
                    })?;
                }
                (None, None) => {}
                _ => {
                    return Err(EventBatchError::InvalidEmbodiedEvent(
                        "heritable-disposition profile and state must be committed together"
                            .to_owned(),
                    ));
                }
            }
        }
        DomainEvent::OrganismPerceived { perception, .. } => perception
            .validate()
            .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?,
        DomainEvent::OrganismActed { action, .. } => action
            .validate()
            .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?,
        DomainEvent::MaterialInstanceInitialized {
            material,
            initial_mass_milligrams,
            oral_transfer_profiles,
            ..
        } => {
            material
                .validate()
                .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?;
            validate_oral_transfer_profiles(
                material,
                *initial_mass_milligrams,
                oral_transfer_profiles,
            )?;
        }
        DomainEvent::MaterialReservoirCommitted { commitment, .. } => commitment
            .validate()
            .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?,
        DomainEvent::MaterialSurfaceTraceChanged {
            from_trace_units,
            applied_force_units,
            to_trace_units,
            ..
        } => {
            if *applied_force_units == 0
                || *to_trace_units <= *from_trace_units
                || *to_trace_units > i32::MAX.unsigned_abs()
                || to_trace_units.checked_sub(*from_trace_units)
                    != Some(u32::from(*applied_force_units))
            {
                return Err(EventBatchError::InvalidEmbodiedEvent(
                    "invalid material surface-trace transition".to_owned(),
                ));
            }
        }
        DomainEvent::MaterialOralPortionTransferred {
            profile_digest,
            from_mass_milligrams,
            transferred_mass_milligrams,
            to_mass_milligrams,
            ..
        } => {
            if *profile_digest == Digest::ZERO
                || *transferred_mass_milligrams == 0
                || from_mass_milligrams.checked_sub(*transferred_mass_milligrams)
                    != Some(*to_mass_milligrams)
            {
                return Err(EventBatchError::InvalidEmbodiedEvent(
                    "invalid material oral-transfer arithmetic".to_owned(),
                ));
            }
        }
        DomainEvent::MaterialReservoirOralPortionTransferred {
            profile_digest,
            settled_from_tick,
            settled_to_tick,
            from_mass_milligrams,
            replenished_mass_milligrams,
            transferred_mass_milligrams,
            to_mass_milligrams,
            ..
        } => {
            if *profile_digest == Digest::ZERO
                || settled_to_tick < settled_from_tick
                || *transferred_mass_milligrams == 0
                || from_mass_milligrams
                    .checked_add(*replenished_mass_milligrams)
                    .and_then(|available| available.checked_sub(*transferred_mass_milligrams))
                    != Some(*to_mass_milligrams)
            {
                return Err(EventBatchError::InvalidEmbodiedEvent(
                    "invalid material-reservoir oral-transfer arithmetic".to_owned(),
                ));
            }
        }
        DomainEvent::OrganismActionValueChanged { from, to, .. }
        | DomainEvent::OrganismSocialActionValueChanged { from, to, .. } => {
            to.validate()
                .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?;
            let expected_observations = match from {
                Some(from) => {
                    from.validate().map_err(|error| {
                        EventBatchError::InvalidEmbodiedEvent(error.to_string())
                    })?;
                    if from.action_kind != to.action_kind {
                        return Err(EventBatchError::InvalidEmbodiedEvent(
                            "action-value transition changed action kind".to_owned(),
                        ));
                    }
                    from.observations.checked_add(1).ok_or_else(|| {
                        EventBatchError::InvalidEmbodiedEvent(
                            "action-value observation count overflowed".to_owned(),
                        )
                    })?
                }
                None => 1,
            };
            if to.observations != expected_observations {
                return Err(EventBatchError::InvalidEmbodiedEvent(
                    "action-value transition skipped an observation".to_owned(),
                ));
            }
            if let DomainEvent::OrganismSocialActionValueChanged {
                observer_id,
                actor_id,
                ..
            } = event
            {
                let expected_value = from
                    .map_or(1_i16, |from| from.value.saturating_add(1))
                    .min(crate::ACTION_VALUE_MAX);
                if observer_id == actor_id || to.value != expected_value || to.value <= 0 {
                    return Err(EventBatchError::InvalidEmbodiedEvent(
                        "invalid social action-value transition".to_owned(),
                    ));
                }
            }
        }
        DomainEvent::CognitionRequestSelected { selection } => selection
            .validate()
            .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?,
        DomainEvent::CognitionInputRecorded { input } => input
            .validate()
            .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?,
        DomainEvent::ReproductiveDevelopmentStarted {
            development_id,
            offspring_id,
            species,
            parent_ids,
            developing_parent_id,
            profile_digest,
            due_tick,
            parents_available_at,
            heritable_disposition_profile,
            offspring_heritable_disposition,
            ..
        } => {
            species
                .validate()
                .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?;
            if development_id == offspring_id
                || *profile_digest == Digest::ZERO
                || parent_ids.is_empty()
                || parent_ids.windows(2).any(|pair| pair[0] >= pair[1])
                || !parent_ids.contains(developing_parent_id)
                || due_tick == &SimTick::ZERO
                || parents_available_at <= due_tick
            {
                return Err(EventBatchError::InvalidEmbodiedEvent(
                    "invalid reproductive-development commitment".to_owned(),
                ));
            }
            match (
                heritable_disposition_profile,
                offspring_heritable_disposition,
            ) {
                (Some(profile), Some(disposition)) => {
                    if profile.species != *species {
                        return Err(EventBatchError::InvalidEmbodiedEvent(
                            "offspring disposition profile species does not match development"
                                .to_owned(),
                        ));
                    }
                    disposition.validate_against(profile).map_err(|error| {
                        EventBatchError::InvalidEmbodiedEvent(error.to_string())
                    })?;
                }
                (None, None) => {}
                _ => {
                    return Err(EventBatchError::InvalidEmbodiedEvent(
                        "offspring disposition profile and state must be committed together"
                            .to_owned(),
                    ));
                }
            }
        }
        DomainEvent::OrganismNeedsChanged { from, to, .. } if from == to => {
            return Err(EventBatchError::InvalidEmbodiedEvent(
                "bodily-need transition must change state".to_owned(),
            ));
        }
        _ => {}
    }
    Ok(())
}

fn validate_oral_transfer_profiles(
    material: &MaterialIdentity,
    initial_mass_milligrams: Option<u64>,
    profiles: &[OralTransferCommitment],
) -> Result<(), EventBatchError> {
    if initial_mass_milligrams == Some(0)
        || (initial_mass_milligrams.is_none() && !profiles.is_empty())
    {
        return Err(EventBatchError::InvalidEmbodiedEvent(
            "oral-transfer profiles require positive retained material mass".to_owned(),
        ));
    }
    let mut previous_species_key = None;
    for profile in profiles {
        profile
            .validate()
            .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?;
        if profile.material != *material {
            return Err(EventBatchError::InvalidEmbodiedEvent(
                "oral-transfer material does not match its instance".to_owned(),
            ));
        }
        let mass = initial_mass_milligrams.expect("profiles require retained mass");
        if mass < profile.transfer_mass_milligrams {
            return Err(EventBatchError::InvalidEmbodiedEvent(
                "material mass must contain at least one whole oral-transfer portion".to_owned(),
            ));
        }
        let species_key = species_identity_key(&profile.species);
        if previous_species_key.is_some_and(|previous| previous >= species_key) {
            return Err(EventBatchError::InvalidEmbodiedEvent(
                "oral-transfer profiles must be strictly species ordered".to_owned(),
            ));
        }
        previous_species_key = Some(species_key);
    }
    Ok(())
}

fn species_identity_key(species: &SpeciesIdentity) -> (&str, &str, &str, &str) {
    (
        &species.catalog,
        &species.identifier,
        &species.scientific_name,
        &species.source_url,
    )
}

#[derive(Serialize)]
struct BatchHashMaterial<'a> {
    event_schema_version: u16,
    world_id: WorldId,
    sequence: EventSequence,
    tick: SimTick,
    ruleset_version: u32,
    previous_hash: Digest,
    events: &'a [EventRecord],
    post_state_hash: Digest,
}

#[derive(Debug, Error)]
pub enum EventBatchError {
    #[error("event batch sequence zero is reserved for pre-genesis state")]
    ZeroSequence,
    #[error("ruleset version must be greater than zero")]
    ZeroRulesetVersion,
    #[error("event batch must contain at least one event")]
    Empty,
    #[error("event batch contains more than u32::MAX events")]
    TooManyEvents,
    #[error("event schema version {0} is unsupported")]
    UnsupportedSchema(u16),
    #[error("event requires a newer event schema version")]
    EventRequiresNewerSchema,
    #[error("embodied event is invalid: {0}")]
    InvalidEmbodiedEvent(String),
    #[error("event identity at index {index} is not deterministic")]
    InvalidEventIdentity { index: u32 },
    #[error("event batch hash mismatch: stored {expected}, calculated {calculated}")]
    HashMismatch {
        expected: Digest,
        calculated: Digest,
    },
    #[error(transparent)]
    CanonicalHash(#[from] CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapacityExhaustionPolicy, EarthResolutionLevels, FullEarthGrid, HERITABLE_ACTION_KINDS,
        HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION, HERITABLE_DISPOSITION_SCHEMA_VERSION,
        HeritableActionWeight, PartitionedExecution, PerceptionChannel, PersonRepresentation,
        PhysiologicalEvidenceBasis, PrimitiveAction, PrimitiveActionKind, PropertyReading,
        ProvisionalWorldCompositionReference, S2Projection, SchedulerKind, SituatedPerception,
        SpatialGrid, WorldConfiguration, WorldDataBundleReference, WorldSeed,
    };
    use uuid::Uuid;

    fn manifest() -> WorldManifest {
        WorldManifest::new(
            WorldId::from_uuid(Uuid::from_u128(0x42)),
            WorldSeed::new(17),
            1,
        )
    }

    fn species() -> SpeciesIdentity {
        SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("species")
    }

    fn configuration() -> WorldConfiguration {
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
                "event-schema-test",
                "0.1.0",
                Digest::sha256(b"event schema test data"),
                "https://data.atinycivilization.com/event-schema-test/0.1.0.json",
                "CC-BY-4.0",
            )
            .expect("valid bundle reference"),
            10_000,
        )
        .expect("valid world configuration")
    }

    fn provisional_configuration() -> WorldConfiguration {
        WorldConfiguration::new_provisional_full_earth(
            300,
            FullEarthGrid {
                physics_crs_epsg: 4_978,
                catalog_crs_epsg: 4_979,
                vertical_crs_epsg: 3_855,
                s2_definition_url: "https://s2geometry.io/devguide/s2cell_hierarchy".to_owned(),
                s2_library_revision: "0123456789abcdef".to_owned(),
                s2_definition_hash: Digest::sha256(b"provisional event S2 fixture"),
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
                "event-schema-provisional",
                "0.1.0",
                Digest::sha256(b"provisional event composition"),
            )
            .expect("valid provisional composition reference"),
            PartitionedExecution {
                scheduler_schema_version: 1,
                scheduler: SchedulerKind::DeterministicEventQueue,
                partition_s2_level: 10,
                person_representation: PersonRepresentation::DurableIndividuals,
                capacity_exhaustion: CapacityExhaustionPolicy::PauseAtCommittedBoundary,
                max_events_per_partition_transition: 10_000,
            },
        )
        .expect("valid provisional world configuration")
    }

    #[test]
    fn batch_hash_covers_event_identity_payload_and_post_state() {
        let manifest = manifest();
        let batch = EventBatch::new(
            LEGACY_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            manifest.ruleset_version,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"post-state"),
        );
        let batch = batch.expect("valid test batch");
        assert!(batch.verify_integrity().is_ok());
        assert_eq!(
            batch.batch_hash.to_string(),
            "f205996446dd9978eb0ce784780c0ee689f14625b36bda0d3a9303a017c895a5"
        );
    }

    #[test]
    fn modified_event_is_detected() {
        let manifest = manifest();
        let mut batch = EventBatch::new(
            LEGACY_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            manifest.ruleset_version,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"post-state"),
        )
        .expect("valid test batch");
        batch.events[0].event = DomainEvent::WorldArchived;

        assert!(matches!(
            batch.verify_integrity(),
            Err(EventBatchError::HashMismatch { .. })
        ));
    }

    #[test]
    fn legacy_schema_rejects_world_configuration() {
        let manifest = manifest();
        let result = EventBatch::new(
            LEGACY_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            manifest.ruleset_version,
            Digest::ZERO,
            vec![DomainEvent::WorldConfigured {
                configuration: configuration(),
            }],
            Digest::sha256(b"post-state"),
        );

        assert!(matches!(
            result,
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
    }

    #[test]
    fn embodied_events_require_schema_three_and_reject_privileged_readings() {
        let manifest = manifest();
        let perception = DomainEvent::OrganismPerceived {
            organism_id: EntityId::from_uuid(Uuid::from_u128(3)),
            perception: SituatedPerception {
                subject_id: None,
                readings: vec![PropertyReading {
                    channel: PerceptionChannel::Touch,
                    property_code: "tool".to_owned(),
                    quantized_value: 1,
                    uncertainty: 0,
                }],
            },
        };
        assert!(
            EventBatch::new(
                CONFIGURED_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                manifest.ruleset_version,
                Digest::ZERO,
                vec![perception.clone()],
                Digest::sha256(b"post-state"),
            )
            .is_err()
        );
        assert!(
            EventBatch::new(
                EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                manifest.ruleset_version,
                Digest::ZERO,
                vec![perception],
                Digest::sha256(b"post-state"),
            )
            .is_err()
        );

        let action = DomainEvent::OrganismActed {
            organism_id: EntityId::from_uuid(Uuid::from_u128(3)),
            action: PrimitiveAction {
                kind: PrimitiveActionKind::ApplyForce,
                target_id: None,
                intensity: 1,
            },
        };
        assert!(
            EventBatch::new(
                EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                manifest.ruleset_version,
                Digest::ZERO,
                vec![action],
                Digest::sha256(b"post-state"),
            )
            .is_ok()
        );
    }

    #[test]
    fn durable_patch_movements_require_schema_four() {
        let manifest = manifest();
        let patch = S2CellId::new(1_u64 << 60).expect("face root");
        let movement = DomainEvent::OrganismMoved {
            organism_id: EntityId::from_uuid(Uuid::from_u128(4)),
            from_patch: patch,
            to_patch: patch.children().expect("children")[0],
        };
        assert!(matches!(
            EventBatch::new(
                EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                manifest.ruleset_version,
                Digest::ZERO,
                vec![movement.clone()],
                Digest::sha256(b"post-state"),
            ),
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
        assert!(
            EventBatch::new(
                EMBODIED_POSITION_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                manifest.ruleset_version,
                Digest::ZERO,
                vec![movement],
                Digest::sha256(b"post-state"),
            )
            .is_ok()
        );
    }

    #[test]
    fn provisional_configuration_requires_event_schema_five() {
        let manifest = manifest();
        let event = DomainEvent::WorldConfigured {
            configuration: provisional_configuration(),
        };
        for older_schema in [
            LEGACY_EVENT_SCHEMA_VERSION,
            CONFIGURED_EVENT_SCHEMA_VERSION,
            EVENT_SCHEMA_VERSION,
            EMBODIED_POSITION_EVENT_SCHEMA_VERSION,
        ] {
            assert!(matches!(
                EventBatch::new(
                    older_schema,
                    manifest.world_id,
                    EventSequence::new(1),
                    SimTick::ZERO,
                    manifest.ruleset_version,
                    Digest::ZERO,
                    vec![event.clone()],
                    Digest::sha256(b"provisional post-state"),
                ),
                Err(EventBatchError::EventRequiresNewerSchema)
            ));
        }

        let batch = EventBatch::new(
            PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            manifest.ruleset_version,
            Digest::ZERO,
            vec![event],
            Digest::sha256(b"provisional post-state"),
        );
        assert!(matches!(batch, Ok(value) if value.verify_integrity().is_ok()));
    }

    #[test]
    fn schema_five_remains_forward_compatible_with_admitted_events() {
        let manifest = manifest();
        let batch = EventBatch::new(
            PROVISIONAL_WORLD_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            manifest.ruleset_version,
            Digest::ZERO,
            vec![DomainEvent::WorldConfigured {
                configuration: configuration(),
            }],
            Digest::sha256(b"admitted post-state"),
        );
        assert!(matches!(batch, Ok(value) if value.verify_integrity().is_ok()));
    }

    #[test]
    fn bodily_regulation_requires_schema_twelve() {
        let manifest = manifest();
        let event = DomainEvent::OrganismNeedsChanged {
            organism_id: EntityId::from_uuid(Uuid::from_u128(0xB0D1)),
            from: BodilyRegulationState::default(),
            to: BodilyRegulationState {
                energy_load_scaled_joules: 1,
                ..BodilyRegulationState::default()
            },
        };
        assert!(matches!(
            EventBatch::new(
                SIGNAL_PROPAGATION_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::new(1),
                10,
                Digest::ZERO,
                vec![event.clone()],
                Digest::sha256(b"body state"),
            ),
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
        EventBatch::new(
            BODILY_REGULATION_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::new(1),
            10,
            Digest::ZERO,
            vec![event],
            Digest::sha256(b"body state"),
        )
        .expect("schema twelve accepts an exact body transition");
    }

    #[test]
    fn retained_mass_and_oral_transfers_require_schema_fourteen() {
        let manifest = manifest();
        let material = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("citable water");
        let species = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("citable species");
        let profile = OralTransferCommitment {
            commitment_schema_version: crate::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
            profile_id: "event-water-human-v1".to_owned(),
            profile_digest: Digest::sha256(b"event oral transfer profile"),
            material: material.clone(),
            species,
            evidence_basis: crate::OralTransferEvidenceBasis::EngineeringAssumption,
            transfer_mass_milligrams: 250_000,
            recoverable_energy_joules: 0,
            hydration_recovery_seconds: 200,
        };
        let initialization = DomainEvent::MaterialInstanceInitialized {
            object_id: EntityId::from_uuid(Uuid::from_u128(0x0A11)),
            material,
            embodied_patch: S2CellId::new(1_u64 << 60).expect("face root"),
            initial_mass_milligrams: Some(500_000),
            oral_transfer_profiles: vec![profile.clone()],
        };
        assert!(matches!(
            EventBatch::new(
                DETERMINISTIC_POLICY_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                12,
                Digest::ZERO,
                vec![initialization.clone()],
                Digest::sha256(b"oral state"),
            ),
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
        EventBatch::new(
            MATERIAL_INGESTION_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            12,
            Digest::ZERO,
            vec![initialization],
            Digest::sha256(b"oral state"),
        )
        .expect("schema fourteen accepts retained mass and its response profile");

        let invalid_transfer = DomainEvent::MaterialOralPortionTransferred {
            object_id: EntityId::from_uuid(Uuid::from_u128(0x0A11)),
            organism_id: EntityId::from_uuid(Uuid::from_u128(0x0B0D)),
            profile_digest: profile.profile_digest,
            from_mass_milligrams: 500_000,
            transferred_mass_milligrams: 250_000,
            to_mass_milligrams: 1,
        };
        assert!(matches!(
            EventBatch::new(
                MATERIAL_INGESTION_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(2),
                SimTick::new(1),
                12,
                Digest::ZERO,
                vec![invalid_transfer],
                Digest::sha256(b"invalid oral state"),
            ),
            Err(EventBatchError::InvalidEmbodiedEvent(_))
        ));
    }

    #[test]
    fn action_value_updates_require_schema_fifteen_and_contiguous_observations() {
        let manifest = manifest();
        let first = ActionValueState {
            value_schema_version: crate::ACTION_VALUE_STATE_SCHEMA_VERSION,
            action_kind: crate::PrimitiveActionKind::Rest,
            observations: 1,
            value: 4,
        };
        let event = DomainEvent::OrganismActionValueChanged {
            organism_id: EntityId::from_uuid(Uuid::from_u128(0xA13)),
            from: None,
            to: first,
        };
        assert!(matches!(
            EventBatch::new(
                MATERIAL_INGESTION_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::new(1),
                13,
                Digest::ZERO,
                vec![event.clone()],
                Digest::sha256(b"action value state"),
            ),
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
        EventBatch::new(
            ACTION_LEARNING_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::new(1),
            13,
            Digest::ZERO,
            vec![event],
            Digest::sha256(b"action value state"),
        )
        .expect("schema fifteen accepts a first action observation");

        let skipped = DomainEvent::OrganismActionValueChanged {
            organism_id: EntityId::from_uuid(Uuid::from_u128(0xA13)),
            from: Some(first),
            to: ActionValueState {
                observations: 3,
                ..first
            },
        };
        assert!(matches!(
            EventBatch::new(
                ACTION_LEARNING_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(2),
                SimTick::new(2),
                13,
                Digest::ZERO,
                vec![skipped],
                Digest::sha256(b"skipped action value state"),
            ),
            Err(EventBatchError::InvalidEmbodiedEvent(_))
        ));
    }

    #[test]
    fn social_action_values_require_schema_twenty_and_direct_bounded_change() {
        let manifest = manifest();
        let observer_id = EntityId::from_uuid(Uuid::from_u128(0x501));
        let actor_id = EntityId::from_uuid(Uuid::from_u128(0x502));
        let event = DomainEvent::OrganismSocialActionValueChanged {
            observer_id,
            actor_id,
            from: None,
            to: ActionValueState {
                value_schema_version: crate::ACTION_VALUE_STATE_SCHEMA_VERSION,
                action_kind: crate::PrimitiveActionKind::Rest,
                observations: 1,
                value: 1,
            },
        };
        assert!(matches!(
            EventBatch::new(
                MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::new(1),
                18,
                Digest::ZERO,
                vec![event.clone()],
                Digest::sha256(b"social state"),
            ),
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
        EventBatch::new(
            SOCIAL_LEARNING_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::new(1),
            18,
            Digest::ZERO,
            vec![event],
            Digest::sha256(b"social state"),
        )
        .expect("schema twenty accepts a direct social observation");

        let self_observation = DomainEvent::OrganismSocialActionValueChanged {
            observer_id,
            actor_id: observer_id,
            from: None,
            to: ActionValueState {
                value_schema_version: crate::ACTION_VALUE_STATE_SCHEMA_VERSION,
                action_kind: crate::PrimitiveActionKind::Rest,
                observations: 1,
                value: 1,
            },
        };
        assert!(matches!(
            EventBatch::new(
                SOCIAL_LEARNING_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::new(1),
                18,
                Digest::ZERO,
                vec![self_observation],
                Digest::sha256(b"invalid social state"),
            ),
            Err(EventBatchError::InvalidEmbodiedEvent(_))
        ));
    }

    #[test]
    fn surface_traces_require_schema_twenty_one_and_exact_force_delta() {
        let manifest = manifest();
        let object_id = EntityId::from_uuid(uuid::Uuid::from_u128(0x601));
        let organism_id = EntityId::from_uuid(uuid::Uuid::from_u128(0x602));
        let event = DomainEvent::MaterialSurfaceTraceChanged {
            object_id,
            organism_id,
            from_trace_units: 4,
            applied_force_units: 3,
            to_trace_units: 7,
        };
        assert!(matches!(
            EventBatch::new(
                SOCIAL_LEARNING_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::new(1),
                19,
                Digest::ZERO,
                vec![event.clone()],
                Digest::sha256(b"surface state"),
            ),
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
        EventBatch::new(
            MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::new(1),
            19,
            Digest::ZERO,
            vec![event],
            Digest::sha256(b"surface state"),
        )
        .expect("schema twenty-one accepts exact surface traces");

        let invalid = DomainEvent::MaterialSurfaceTraceChanged {
            object_id,
            organism_id,
            from_trace_units: 4,
            applied_force_units: 3,
            to_trace_units: 8,
        };
        assert!(matches!(
            EventBatch::new(
                MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::new(1),
                19,
                Digest::ZERO,
                vec![invalid],
                Digest::sha256(b"invalid surface state"),
            ),
            Err(EventBatchError::InvalidEmbodiedEvent(_))
        ));
    }

    #[test]
    fn reproductive_development_requires_schema_sixteen() {
        let manifest = manifest();
        let developing_parent_id = EntityId::from_uuid(Uuid::from_u128(0xA14));
        let other_parent_id = EntityId::from_uuid(Uuid::from_u128(0xA15));
        let event = DomainEvent::ReproductiveDevelopmentStarted {
            development_id: EntityId::from_uuid(Uuid::from_u128(0xA16)),
            offspring_id: EntityId::from_uuid(Uuid::from_u128(0xA17)),
            species: species(),
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("female").expect("category"),
            parent_ids: vec![developing_parent_id, other_parent_id],
            developing_parent_id,
            profile_digest: Digest::sha256(b"reproductive profile"),
            due_tick: SimTick::new(2),
            parents_available_at: SimTick::new(3),
            heritable_disposition_profile: None,
            offspring_heritable_disposition: None,
        };
        assert!(matches!(
            EventBatch::new(
                ACTION_LEARNING_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::new(1),
                14,
                Digest::ZERO,
                vec![event.clone()],
                Digest::sha256(b"reproductive state"),
            ),
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
        EventBatch::new(
            REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::new(1),
            14,
            Digest::ZERO,
            vec![event],
            Digest::sha256(b"reproductive state"),
        )
        .expect("schema sixteen accepts private reproductive development");
    }

    #[test]
    fn heritable_dispositions_require_schema_seventeen_and_exact_profile_pairing() {
        let manifest = manifest();
        let species = species();
        let profile = HeritableDispositionProfile {
            profile_schema_version: HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
            profile_id: "event-heredity-fixture".to_owned(),
            profile_digest: Digest::sha256(b"event heredity fixture"),
            species: species.clone(),
            evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
            minimum_action_weight: 4,
            neutral_action_weight: 16,
            maximum_action_weight: 28,
            founder_variation_steps: 3,
            mutation_probability_millionths: 100_000,
            mutation_max_step: 2,
        };
        let disposition = HeritableDisposition {
            disposition_schema_version: HERITABLE_DISPOSITION_SCHEMA_VERSION,
            profile_digest: profile.profile_digest,
            generation: 0,
            derived_at: SimTick::ZERO,
            action_weights: HERITABLE_ACTION_KINDS
                .into_iter()
                .map(|action_kind| HeritableActionWeight {
                    action_kind,
                    weight: profile.neutral_action_weight,
                })
                .collect(),
        };
        let event = DomainEvent::OrganismInitialized {
            organism_id: EntityId::from_uuid(Uuid::from_u128(0xA18)),
            species,
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("female").expect("category"),
            initial_age_ticks: 20,
            location_id: None,
            embodied_patch: None,
            metabolic_rate: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: Some(profile),
            heritable_disposition: Some(disposition),
        };
        assert!(matches!(
            EventBatch::new(
                REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                15,
                Digest::ZERO,
                vec![event.clone()],
                Digest::sha256(b"heritable state"),
            ),
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
        EventBatch::new(
            HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            15,
            Digest::ZERO,
            vec![event],
            Digest::sha256(b"heritable state"),
        )
        .expect("schema seventeen accepts a bounded paired disposition");
    }
}
