use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    ActionValueState, BodilyNeedState, CanonicalHashError, Digest, EntityId, PerceptionChannel,
    PrimitiveActionKind, SimTick, WorldId,
};

pub const COGNITION_SELECTION_SCHEMA_VERSION: u16 = 1;
pub const COGNITION_INPUT_SCHEMA_VERSION: u16 = 1;
pub const MAX_COGNITION_SELECTION_READINGS: usize = 32;
pub const MAX_COGNITION_SELECTION_QUERY_BYTES: usize = 4 * 1024;
pub const MAX_COGNITION_SELECTION_OUTPUT_TOKENS: u16 = 64;
pub const MAX_COGNITION_RECALL_TOKENS: u32 = 4_096;
const MAX_COGNITION_PROVIDER_SLUG_BYTES: usize = 64;
const MAX_COGNITION_MODEL_ID_BYTES: usize = 256;
const MAX_COGNITION_ADAPTER_VERSION_BYTES: usize = 128;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionReading {
    pub subject_id: Option<EntityId>,
    pub channel: PerceptionChannel,
    pub property_code: String,
    pub quantized_value: i32,
    pub uncertainty: u16,
    pub observed_at: SimTick,
}

impl CognitionReading {
    fn key(&self) -> (Option<EntityId>, PerceptionChannel, &str) {
        (self.subject_id, self.channel, self.property_code.as_str())
    }
}

/// Exact body-owned information selected for one optional external-cognition job.
/// This is a request fact, not a model response and not a cultural concept.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionRequestSelection {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub world_id: WorldId,
    pub organism_id: EntityId,
    pub selected_at_tick: SimTick,
    pub deadline_tick: SimTick,
    pub ordinal: u32,
    pub bodily_needs: BodilyNeedState,
    pub readings: Vec<CognitionReading>,
    pub action_values: Vec<ActionValueState>,
    pub memory_query: String,
    pub memory_max_tokens: u32,
    pub model_max_output_tokens: u16,
}

impl CognitionRequestSelection {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world_id: WorldId,
        organism_id: EntityId,
        selected_at_tick: SimTick,
        deadline_tick: SimTick,
        ordinal: u32,
        bodily_needs: BodilyNeedState,
        readings: Vec<CognitionReading>,
        action_values: Vec<ActionValueState>,
        memory_query: impl Into<String>,
        memory_max_tokens: u32,
        model_max_output_tokens: u16,
    ) -> Result<Self, CognitionContractError> {
        let selection = Self {
            schema_version: COGNITION_SELECTION_SCHEMA_VERSION,
            request_id: cognition_request_id(world_id, organism_id, selected_at_tick, ordinal),
            world_id,
            organism_id,
            selected_at_tick,
            deadline_tick,
            ordinal,
            bodily_needs,
            readings,
            action_values,
            memory_query: memory_query.into(),
            memory_max_tokens,
            model_max_output_tokens,
        };
        selection.validate()?;
        Ok(selection)
    }

    pub fn validate(&self) -> Result<(), CognitionContractError> {
        if self.schema_version != COGNITION_SELECTION_SCHEMA_VERSION {
            return Err(CognitionContractError::UnsupportedSelectionSchema(
                self.schema_version,
            ));
        }
        if self.request_id
            != cognition_request_id(
                self.world_id,
                self.organism_id,
                self.selected_at_tick,
                self.ordinal,
            )
        {
            return Err(CognitionContractError::InvalidRequestIdentity);
        }
        if self.deadline_tick <= self.selected_at_tick {
            return Err(CognitionContractError::InvalidDeadline);
        }
        if self.memory_query.trim().is_empty()
            || self.memory_query.len() > MAX_COGNITION_SELECTION_QUERY_BYTES
        {
            return Err(CognitionContractError::InvalidMemoryQuery);
        }
        if self.memory_max_tokens == 0 || self.memory_max_tokens > MAX_COGNITION_RECALL_TOKENS {
            return Err(CognitionContractError::InvalidMemoryTokenBudget);
        }
        if self.model_max_output_tokens == 0
            || self.model_max_output_tokens > MAX_COGNITION_SELECTION_OUTPUT_TOKENS
        {
            return Err(CognitionContractError::InvalidModelTokenBudget);
        }
        if self.readings.len() > MAX_COGNITION_SELECTION_READINGS
            || self
                .readings
                .windows(2)
                .any(|pair| pair[0].key() >= pair[1].key())
            || self.readings.iter().any(|reading| {
                reading.observed_at > self.selected_at_tick
                    || reading.property_code.trim().is_empty()
                    || reading.property_code.len() > 128
            })
        {
            return Err(CognitionContractError::InvalidReadings);
        }
        if self
            .action_values
            .windows(2)
            .any(|pair| pair[0].action_kind >= pair[1].action_kind)
            || self
                .action_values
                .iter()
                .any(|value| value.validate().is_err())
        {
            return Err(CognitionContractError::InvalidActionValues);
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<Digest, CognitionContractError> {
        self.validate()?;
        Digest::canonical(self).map_err(CognitionContractError::Hash)
    }
}

/// Why a fixed-deadline cognition request produced no external action bias.
/// These are infrastructure outcomes recorded for replay, never concepts exposed
/// to an organism inside the world.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CognitionUnavailableReason {
    DeadlineNoResult,
    LadderExhausted,
    BudgetDenied,
    SubjectUnavailable,
    WorldArchived,
}

/// Bounded provenance for one admitted model result. Provider prose and raw error
/// bodies are deliberately excluded from canonical history.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionModelEvidence {
    pub provider_slug: String,
    pub requested_model: String,
    pub resolved_model: String,
    pub provider_response_hash: Digest,
    pub adapter_version: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub billed_micro_usd: u64,
    pub action_kind: PrimitiveActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_region: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_intensity: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement_direction: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum CognitionInputOutcome {
    Model(CognitionModelEvidence),
    Unavailable { reason: CognitionUnavailableReason },
}

/// The exact one-way hand-off admitted at a request's deterministic deadline.
/// Only `action_kind` can influence planning, and only as a bounded weight among
/// primitive actions already available to the organism.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionDeadlineInput {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub world_id: WorldId,
    pub organism_id: EntityId,
    pub selected_at_tick: SimTick,
    pub deadline_tick: SimTick,
    pub ordinal: u32,
    pub selection_hash: Digest,
    pub recall_outcome_hash: Digest,
    pub route_registry_hash: Digest,
    pub result_hash: Digest,
    pub outcome: CognitionInputOutcome,
}

impl CognitionDeadlineInput {
    #[allow(clippy::too_many_arguments)]
    pub fn model(
        selection: &CognitionRequestSelection,
        recall_outcome_hash: Digest,
        route_registry_hash: Digest,
        result_hash: Digest,
        evidence: CognitionModelEvidence,
    ) -> Result<Self, CognitionContractError> {
        let input = Self::from_selection(
            selection,
            recall_outcome_hash,
            route_registry_hash,
            result_hash,
            CognitionInputOutcome::Model(evidence),
        )?;
        input.validate_against(selection)?;
        Ok(input)
    }

    pub fn unavailable(
        selection: &CognitionRequestSelection,
        recall_outcome_hash: Digest,
        route_registry_hash: Digest,
        result_hash: Digest,
        reason: CognitionUnavailableReason,
    ) -> Result<Self, CognitionContractError> {
        let input = Self::from_selection(
            selection,
            recall_outcome_hash,
            route_registry_hash,
            result_hash,
            CognitionInputOutcome::Unavailable { reason },
        )?;
        input.validate_against(selection)?;
        Ok(input)
    }

    fn from_selection(
        selection: &CognitionRequestSelection,
        recall_outcome_hash: Digest,
        route_registry_hash: Digest,
        result_hash: Digest,
        outcome: CognitionInputOutcome,
    ) -> Result<Self, CognitionContractError> {
        selection.validate()?;
        Ok(Self {
            schema_version: COGNITION_INPUT_SCHEMA_VERSION,
            request_id: selection.request_id,
            world_id: selection.world_id,
            organism_id: selection.organism_id,
            selected_at_tick: selection.selected_at_tick,
            deadline_tick: selection.deadline_tick,
            ordinal: selection.ordinal,
            selection_hash: selection.canonical_hash()?,
            recall_outcome_hash,
            route_registry_hash,
            result_hash,
            outcome,
        })
    }

    pub fn validate(&self) -> Result<(), CognitionContractError> {
        if self.schema_version != COGNITION_INPUT_SCHEMA_VERSION {
            return Err(CognitionContractError::UnsupportedInputSchema(
                self.schema_version,
            ));
        }
        if self.request_id
            != cognition_request_id(
                self.world_id,
                self.organism_id,
                self.selected_at_tick,
                self.ordinal,
            )
        {
            return Err(CognitionContractError::InvalidInputIdentity);
        }
        if self.deadline_tick <= self.selected_at_tick || self.selection_hash == Digest::ZERO {
            return Err(CognitionContractError::InvalidDeadlineInput);
        }
        match &self.outcome {
            CognitionInputOutcome::Model(evidence) => {
                let provider_valid = !evidence.provider_slug.is_empty()
                    && evidence.provider_slug.len() <= MAX_COGNITION_PROVIDER_SLUG_BYTES
                    && evidence.provider_slug.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'-' | b'_')
                    });
                let model_valid = |value: &str| {
                    !value.is_empty()
                        && value.trim() == value
                        && value.len() <= MAX_COGNITION_MODEL_ID_BYTES
                };
                if !provider_valid
                    || !model_valid(&evidence.requested_model)
                    || !model_valid(&evidence.resolved_model)
                    || evidence.provider_response_hash == Digest::ZERO
                    || evidence.adapter_version.is_empty()
                    || evidence.adapter_version.trim() != evidence.adapter_version
                    || evidence.adapter_version.len() > MAX_COGNITION_ADAPTER_VERSION_BYTES
                    || evidence.completion_tokens > u32::from(MAX_COGNITION_SELECTION_OUTPUT_TOKENS)
                    || evidence.contact_region.is_some_and(|region| {
                        evidence.action_kind != PrimitiveActionKind::ApplyForce || region >= 8
                    })
                    || evidence.signal_intensity.is_some_and(|intensity| {
                        evidence.action_kind != PrimitiveActionKind::EmitSignal
                            || !(1..=8).contains(&intensity)
                    })
                    || evidence.movement_direction.is_some_and(|direction| {
                        evidence.action_kind != PrimitiveActionKind::Move || direction >= 4
                    })
                    || self.recall_outcome_hash == Digest::ZERO
                    || self.route_registry_hash == Digest::ZERO
                    || self.result_hash == Digest::ZERO
                {
                    return Err(CognitionContractError::InvalidModelEvidence);
                }
            }
            CognitionInputOutcome::Unavailable { .. } => {
                if (self.result_hash == Digest::ZERO) != (self.route_registry_hash == Digest::ZERO)
                {
                    return Err(CognitionContractError::InvalidUnavailableEvidence);
                }
            }
        }
        Ok(())
    }

    pub fn validate_against(
        &self,
        selection: &CognitionRequestSelection,
    ) -> Result<(), CognitionContractError> {
        self.validate()?;
        selection.validate()?;
        if self.request_id != selection.request_id
            || self.world_id != selection.world_id
            || self.organism_id != selection.organism_id
            || self.selected_at_tick != selection.selected_at_tick
            || self.deadline_tick != selection.deadline_tick
            || self.ordinal != selection.ordinal
            || self.selection_hash != selection.canonical_hash()?
            || matches!(
                &self.outcome,
                CognitionInputOutcome::Model(evidence)
                    if evidence.completion_tokens > u32::from(selection.model_max_output_tokens)
            )
        {
            return Err(CognitionContractError::InvalidDeadlineInput);
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<Digest, CognitionContractError> {
        self.validate()?;
        Digest::canonical(self).map_err(CognitionContractError::Hash)
    }

    #[must_use]
    pub const fn action_kind(&self) -> Option<PrimitiveActionKind> {
        match &self.outcome {
            CognitionInputOutcome::Model(evidence) => Some(evidence.action_kind),
            CognitionInputOutcome::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn contact_region(&self) -> Option<u8> {
        match &self.outcome {
            CognitionInputOutcome::Model(evidence) => evidence.contact_region,
            CognitionInputOutcome::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn signal_intensity(&self) -> Option<u8> {
        match &self.outcome {
            CognitionInputOutcome::Model(evidence) => evidence.signal_intensity,
            CognitionInputOutcome::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn movement_direction(&self) -> Option<u8> {
        match &self.outcome {
            CognitionInputOutcome::Model(evidence) => evidence.movement_direction,
            CognitionInputOutcome::Unavailable { .. } => None,
        }
    }
}

#[must_use]
pub fn cognition_request_id(
    world_id: WorldId,
    organism_id: EntityId,
    selected_at_tick: SimTick,
    ordinal: u32,
) -> Uuid {
    Uuid::new_v5(
        &world_id.as_uuid(),
        format!(
            "bounded-cognition:{organism_id}:{}:{ordinal}",
            selected_at_tick.get()
        )
        .as_bytes(),
    )
}

#[derive(Debug, Error)]
pub enum CognitionContractError {
    #[error("unsupported cognition-selection schema {0}")]
    UnsupportedSelectionSchema(u16),
    #[error("unsupported cognition deadline-input schema {0}")]
    UnsupportedInputSchema(u16),
    #[error("cognition request identity is not deterministic")]
    InvalidRequestIdentity,
    #[error("cognition deadline input has an invalid request identity")]
    InvalidInputIdentity,
    #[error("cognition deadline must be after its selection tick")]
    InvalidDeadline,
    #[error("cognition memory query must contain 1 to 4096 bytes")]
    InvalidMemoryQuery,
    #[error("cognition memory token budget must be between 1 and 4096")]
    InvalidMemoryTokenBudget,
    #[error("cognition model output budget must be between 1 and 64 tokens")]
    InvalidModelTokenBudget,
    #[error("cognition readings are oversized, reordered, duplicated, or invalid")]
    InvalidReadings,
    #[error("cognition action values are invalid or non-canonical")]
    InvalidActionValues,
    #[error("cognition deadline input is invalid or does not match its selection")]
    InvalidDeadlineInput,
    #[error("cognition model evidence is incomplete or oversized")]
    InvalidModelEvidence,
    #[error("cognition unavailable evidence hashes are inconsistent")]
    InvalidUnavailableEvidence,
    #[error("cognition canonical hashing failed: {0}")]
    Hash(#[from] CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_identity_and_hash_are_stable_and_bounded() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(91));
        let organism_id = EntityId::deterministic(world_id, b"cognition-selection-test");
        let selection = CognitionRequestSelection::new(
            world_id,
            organism_id,
            SimTick::new(3),
            SimTick::new(15),
            0,
            BodilyNeedState::default(),
            Vec::new(),
            Vec::new(),
            "recent direct physical readings",
            256,
            32,
        )
        .expect("valid selection");
        assert!(selection.validate().is_ok());
        assert_ne!(selection.canonical_hash().expect("hash"), Digest::ZERO);

        let mut forged = selection;
        forged.ordinal = 1;
        assert!(matches!(
            forged.validate(),
            Err(CognitionContractError::InvalidRequestIdentity)
        ));
    }

    #[test]
    fn deadline_input_is_typed_bounded_and_selection_bound() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(92));
        let organism_id = EntityId::deterministic(world_id, b"cognition-input-test");
        let selection = CognitionRequestSelection::new(
            world_id,
            organism_id,
            SimTick::new(3),
            SimTick::new(15),
            0,
            BodilyNeedState::default(),
            Vec::new(),
            Vec::new(),
            "recent direct physical readings",
            256,
            32,
        )
        .expect("valid selection");
        let input = CognitionDeadlineInput::model(
            &selection,
            Digest::sha256(b"recall"),
            Digest::sha256(b"registry"),
            Digest::sha256(b"result"),
            CognitionModelEvidence {
                provider_slug: "free_provider".to_owned(),
                requested_model: "model/free".to_owned(),
                resolved_model: "model/free-2026-08".to_owned(),
                provider_response_hash: Digest::sha256(b"provider response"),
                adapter_version: "adapter-v1".to_owned(),
                prompt_tokens: 64,
                completion_tokens: 1,
                billed_micro_usd: 0,
                action_kind: PrimitiveActionKind::Orient,
                contact_region: None,
                signal_intensity: None,
                movement_direction: None,
            },
        )
        .expect("valid model input");
        assert_eq!(input.action_kind(), Some(PrimitiveActionKind::Orient));
        assert_eq!(input.contact_region(), None);
        assert_ne!(input.canonical_hash().expect("input hash"), Digest::ZERO);

        let mut invalid_region = input.clone();
        if let CognitionInputOutcome::Model(evidence) = &mut invalid_region.outcome {
            evidence.contact_region = Some(8);
        }
        assert!(matches!(
            invalid_region.validate(),
            Err(CognitionContractError::InvalidModelEvidence)
        ));
        if let CognitionInputOutcome::Model(evidence) = &mut invalid_region.outcome {
            evidence.action_kind = PrimitiveActionKind::ApplyForce;
            evidence.contact_region = Some(7);
        }
        assert!(invalid_region.validate().is_ok());

        if let CognitionInputOutcome::Model(evidence) = &mut invalid_region.outcome {
            evidence.action_kind = PrimitiveActionKind::EmitSignal;
            evidence.contact_region = None;
            evidence.signal_intensity = Some(8);
        }
        assert!(invalid_region.validate().is_ok());
        assert_eq!(invalid_region.signal_intensity(), Some(8));
        if let CognitionInputOutcome::Model(evidence) = &mut invalid_region.outcome {
            evidence.signal_intensity = Some(0);
        }
        assert!(matches!(
            invalid_region.validate(),
            Err(CognitionContractError::InvalidModelEvidence)
        ));

        let mut forged_selection = selection.clone();
        forged_selection.model_max_output_tokens = 1;
        let mut oversized = input;
        let CognitionInputOutcome::Model(evidence) = &mut oversized.outcome else {
            unreachable!()
        };
        evidence.completion_tokens = 2;
        assert!(oversized.validate_against(&forged_selection).is_err());

        let unavailable = CognitionDeadlineInput::unavailable(
            &selection,
            Digest::ZERO,
            Digest::ZERO,
            Digest::ZERO,
            CognitionUnavailableReason::DeadlineNoResult,
        )
        .expect("explicit local fallback");
        assert_eq!(unavailable.action_kind(), None);
    }
}
