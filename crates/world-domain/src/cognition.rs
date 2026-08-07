use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    ActionValueState, BodilyNeedState, CanonicalHashError, Digest, EntityId, PerceptionChannel,
    SimTick, WorldId,
};

pub const COGNITION_SELECTION_SCHEMA_VERSION: u16 = 1;
pub const MAX_COGNITION_SELECTION_READINGS: usize = 32;
pub const MAX_COGNITION_SELECTION_QUERY_BYTES: usize = 4 * 1024;
pub const MAX_COGNITION_SELECTION_OUTPUT_TOKENS: u16 = 64;
pub const MAX_COGNITION_RECALL_TOKENS: u32 = 4_096;

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
    #[error("cognition request identity is not deterministic")]
    InvalidRequestIdentity,
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
}
