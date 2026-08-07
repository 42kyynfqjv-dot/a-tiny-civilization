use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use world_domain::{Digest, EntityId, EventSequence, SimTick, WorldId};

use crate::CognitionMemoryInput;

pub const MEMORY_PAYLOAD_VERSION: u16 = 1;
const MAX_MEMORY_CONTENT_BYTES: usize = 64 * 1024;
const MAX_MEMORY_CONTEXT_BYTES: usize = 512;
const MAX_RECALL_QUERY_BYTES: usize = 4 * 1024;
const MAX_RECALL_RESULTS: usize = 32;
const MAX_REMOTE_MEMORY_ID_BYTES: usize = 256;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryRetain {
    pub payload_version: u16,
    pub operation_id: Uuid,
    pub document_id: Uuid,
    pub world_id: WorldId,
    pub agent_id: EntityId,
    pub source_sequence: EventSequence,
    pub sim_tick: SimTick,
    pub ordinal: u32,
    pub bank_id: String,
    pub content: String,
    pub context: String,
}

impl MemoryRetain {
    pub fn new(
        world_id: WorldId,
        agent_id: EntityId,
        source_sequence: EventSequence,
        sim_tick: SimTick,
        ordinal: u32,
        content: impl Into<String>,
        context: impl Into<String>,
    ) -> Result<Self, MemoryContractError> {
        let operation_id = memory_identity(
            world_id,
            agent_id,
            source_sequence,
            ordinal,
            "memory-operation",
        );
        let document_id = memory_identity(
            world_id,
            agent_id,
            source_sequence,
            ordinal,
            "memory-document",
        );
        let item = Self {
            payload_version: MEMORY_PAYLOAD_VERSION,
            operation_id,
            document_id,
            world_id,
            agent_id,
            source_sequence,
            sim_tick,
            ordinal,
            bank_id: memory_bank_id(world_id, agent_id),
            content: content.into(),
            context: context.into(),
        };
        item.validate()?;
        Ok(item)
    }

    pub fn validate(&self) -> Result<(), MemoryContractError> {
        if self.payload_version != MEMORY_PAYLOAD_VERSION {
            return Err(MemoryContractError::UnsupportedPayloadVersion(
                self.payload_version,
            ));
        }
        if self.source_sequence == EventSequence::ZERO {
            return Err(MemoryContractError::ZeroSourceSequence);
        }
        let expected_operation = memory_identity(
            self.world_id,
            self.agent_id,
            self.source_sequence,
            self.ordinal,
            "memory-operation",
        );
        let expected_document = memory_identity(
            self.world_id,
            self.agent_id,
            self.source_sequence,
            self.ordinal,
            "memory-document",
        );
        if self.operation_id != expected_operation || self.document_id != expected_document {
            return Err(MemoryContractError::InvalidDeterministicIdentity);
        }
        if self.bank_id != memory_bank_id(self.world_id, self.agent_id) {
            return Err(MemoryContractError::InvalidBankId);
        }
        if self.content.trim().is_empty() || self.content.len() > MAX_MEMORY_CONTENT_BYTES {
            return Err(MemoryContractError::InvalidContent);
        }
        if self.context.trim().is_empty() || self.context.len() > MAX_MEMORY_CONTEXT_BYTES {
            return Err(MemoryContractError::InvalidContext);
        }
        Ok(())
    }
}

#[must_use]
pub fn memory_bank_id(world_id: WorldId, agent_id: EntityId) -> String {
    format!("atc-{world_id}-{agent_id}")
}

fn memory_identity(
    world_id: WorldId,
    agent_id: EntityId,
    source_sequence: EventSequence,
    ordinal: u32,
    namespace: &str,
) -> Uuid {
    Uuid::new_v5(
        &world_id.as_uuid(),
        format!("{namespace}:{agent_id}:{}:{ordinal}", source_sequence.get()).as_bytes(),
    )
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionEffects {
    pub memory_retains: Vec<MemoryRetain>,
}

impl TransitionEffects {
    pub fn validate_for(
        &self,
        world_id: WorldId,
        source_sequence: EventSequence,
        sim_tick: SimTick,
    ) -> Result<(), MemoryContractError> {
        let mut operations = BTreeSet::new();
        let mut documents = BTreeSet::new();
        for memory in &self.memory_retains {
            memory.validate()?;
            if memory.world_id != world_id
                || memory.source_sequence != source_sequence
                || memory.sim_tick != sim_tick
            {
                return Err(MemoryContractError::TransitionMismatch);
            }
            if !operations.insert(memory.operation_id) || !documents.insert(memory.document_id) {
                return Err(MemoryContractError::DuplicateIdentity);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryRetainReceipt {
    pub operation_id: Uuid,
    pub remote_operation_id: String,
    pub adapter_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryRecallRequest {
    pub request_id: Uuid,
    pub world_id: WorldId,
    pub agent_id: EntityId,
    pub bank_id: String,
    pub selected_at_tick: SimTick,
    pub deadline_tick: SimTick,
    pub ordinal: u32,
    pub query: String,
    pub max_tokens: u32,
}

impl MemoryRecallRequest {
    pub fn new(
        world_id: WorldId,
        agent_id: EntityId,
        selected_at_tick: SimTick,
        deadline_tick: SimTick,
        ordinal: u32,
        query: impl Into<String>,
        max_tokens: u32,
    ) -> Result<Self, MemoryContractError> {
        let request_id = recall_identity(world_id, agent_id, selected_at_tick, ordinal);
        let request = Self {
            request_id,
            world_id,
            agent_id,
            bank_id: memory_bank_id(world_id, agent_id),
            selected_at_tick,
            deadline_tick,
            ordinal,
            query: query.into(),
            max_tokens,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), MemoryContractError> {
        if self.bank_id != memory_bank_id(self.world_id, self.agent_id) {
            return Err(MemoryContractError::InvalidBankId);
        }
        if self.request_id
            != recall_identity(
                self.world_id,
                self.agent_id,
                self.selected_at_tick,
                self.ordinal,
            )
        {
            return Err(MemoryContractError::InvalidDeterministicIdentity);
        }
        if self.deadline_tick < self.selected_at_tick {
            return Err(MemoryContractError::DeadlineBeforeSelection);
        }
        if self.query.trim().is_empty() || self.query.len() > MAX_RECALL_QUERY_BYTES {
            return Err(MemoryContractError::InvalidQuery);
        }
        if self.max_tokens == 0 || self.max_tokens > 4_096 {
            return Err(MemoryContractError::InvalidTokenBudget);
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<Digest, MemoryContractError> {
        Digest::canonical(self).map_err(|error| MemoryContractError::Hash(error.to_string()))
    }
}

fn recall_identity(
    world_id: WorldId,
    agent_id: EntityId,
    selected_at_tick: SimTick,
    ordinal: u32,
) -> Uuid {
    Uuid::new_v5(
        &world_id.as_uuid(),
        format!(
            "memory-recall:{agent_id}:{}:{ordinal}",
            selected_at_tick.get()
        )
        .as_bytes(),
    )
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryFactKind {
    World,
    Experience,
    Observation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecalledMemory {
    pub rank: u16,
    pub remote_memory_id: String,
    pub document_id: Uuid,
    pub source_sequence: EventSequence,
    pub sim_tick: SimTick,
    pub ordinal: u32,
    pub text: String,
    pub kind: MemoryFactKind,
    pub context: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallUnavailableReason {
    Disabled,
    DeadlineElapsed,
    AdapterUnavailable,
    InvalidResponse,
    MissingRecordedInput,
    RecordedRequestMismatch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MemoryRecallOutcome {
    Available {
        request_id: Uuid,
        request_hash: Digest,
        adapter_version: String,
        response_hash: Digest,
        results: Vec<RecalledMemory>,
    },
    Unavailable {
        request_id: Uuid,
        request_hash: Digest,
        reason: RecallUnavailableReason,
    },
}

impl MemoryRecallOutcome {
    pub fn available(
        request: &MemoryRecallRequest,
        adapter_version: impl Into<String>,
        response_hash: Digest,
        results: Vec<RecalledMemory>,
    ) -> Result<Self, MemoryContractError> {
        let outcome = Self::Available {
            request_id: request.request_id,
            request_hash: request.canonical_hash()?,
            adapter_version: adapter_version.into(),
            response_hash,
            results,
        };
        outcome.validate_against(request)?;
        Ok(outcome)
    }

    pub fn unavailable(
        request: &MemoryRecallRequest,
        reason: RecallUnavailableReason,
    ) -> Result<Self, MemoryContractError> {
        request.validate()?;
        Ok(Self::Unavailable {
            request_id: request.request_id,
            request_hash: request.canonical_hash()?,
            reason,
        })
    }

    #[must_use]
    pub const fn request_id(&self) -> Uuid {
        match self {
            Self::Available { request_id, .. } | Self::Unavailable { request_id, .. } => {
                *request_id
            }
        }
    }

    #[must_use]
    pub const fn request_hash(&self) -> Digest {
        match self {
            Self::Available { request_hash, .. } | Self::Unavailable { request_hash, .. } => {
                *request_hash
            }
        }
    }

    pub fn validate_against(
        &self,
        request: &MemoryRecallRequest,
    ) -> Result<(), MemoryContractError> {
        request.validate()?;
        if self.request_id() != request.request_id
            || self.request_hash() != request.canonical_hash()?
        {
            return Err(MemoryContractError::InvalidRecallOutcome);
        }
        let Self::Available {
            adapter_version,
            response_hash,
            results,
            ..
        } = self
        else {
            return Ok(());
        };
        if adapter_version.trim().is_empty()
            || adapter_version.len() > 128
            || *response_hash == Digest::ZERO
            || results.len() > MAX_RECALL_RESULTS
        {
            return Err(MemoryContractError::InvalidRecallOutcome);
        }
        let mut documents = BTreeSet::new();
        for (position, result) in results.iter().enumerate() {
            if usize::from(result.rank) != position
                || !documents.insert(result.document_id)
                || result.remote_memory_id.trim().is_empty()
                || result.remote_memory_id.len() > MAX_REMOTE_MEMORY_ID_BYTES
                || result.source_sequence == EventSequence::ZERO
                || result.sim_tick > request.selected_at_tick
                || result.text.trim().is_empty()
                || result.text.len() > MAX_MEMORY_CONTENT_BYTES
                || result.kind != MemoryFactKind::Experience
                || result.context.trim().is_empty()
                || result.context.len() > MAX_MEMORY_CONTEXT_BYTES
            {
                return Err(MemoryContractError::InvalidRecalledMemory);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryOutboxEntry {
    pub retain: MemoryRetain,
    pub attempt_count: u32,
}

#[derive(Debug, Error)]
pub enum MemoryContractError {
    #[error("unsupported memory payload version {0}")]
    UnsupportedPayloadVersion(u16),
    #[error("memory source sequence must be greater than zero")]
    ZeroSourceSequence,
    #[error("memory identity does not match its deterministic provenance")]
    InvalidDeterministicIdentity,
    #[error("memory bank identifier does not match world and agent")]
    InvalidBankId,
    #[error("memory content must contain 1 to 65536 bytes")]
    InvalidContent,
    #[error("memory context must contain 1 to 512 bytes")]
    InvalidContext,
    #[error("memory does not belong to its committed transition")]
    TransitionMismatch,
    #[error("transition memory operations and documents must have unique identities")]
    DuplicateIdentity,
    #[error("memory recall deadline precedes its deterministic selection tick")]
    DeadlineBeforeSelection,
    #[error("memory recall query must contain 1 to 4096 bytes")]
    InvalidQuery,
    #[error("memory recall token budget must be between 1 and 4096")]
    InvalidTokenBudget,
    #[error("memory recall outcome does not match its request or bounded adapter contract")]
    InvalidRecallOutcome,
    #[error("recalled memory lacks canonical life-local source provenance")]
    InvalidRecalledMemory,
    #[error("memory contract hashing failed: {0}")]
    Hash(String),
}

#[derive(Debug, Error)]
pub enum MemoryAdapterError {
    #[error("memory adapter is unavailable: {0}")]
    Unavailable(String),
    #[error("memory adapter rejected the request: {0}")]
    Rejected(String),
    #[error("memory adapter returned an invalid response: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait AgentMemory: Send + Sync {
    async fn retain(
        &self,
        memory: &MemoryRetain,
    ) -> Result<MemoryRetainReceipt, MemoryAdapterError>;

    async fn recall(&self, request: &MemoryRecallRequest) -> MemoryRecallOutcome;
}

#[async_trait]
pub trait MemoryOutboxStore: Send + Sync {
    async fn claim_next_memory(
        &self,
        worker_id: &str,
        claim_lease_seconds: u32,
    ) -> Result<Option<MemoryOutboxEntry>, super::StoreError>;

    async fn mark_memory_accepted(
        &self,
        worker_id: &str,
        entry: &MemoryOutboxEntry,
        receipt: &MemoryRetainReceipt,
    ) -> Result<(), super::StoreError>;

    async fn reschedule_memory(
        &self,
        worker_id: &str,
        entry: &MemoryOutboxEntry,
        error: &str,
        retry_after_seconds: u32,
    ) -> Result<(), super::StoreError>;

    /// Admits only life-local Hindsight results whose caller-supplied document
    /// provenance and exact content still match an accepted local delivery.
    async fn admit_recall_for_cognition(
        &self,
        request: &MemoryRecallRequest,
        outcome: &MemoryRecallOutcome,
    ) -> Result<Vec<CognitionMemoryInput>, super::StoreError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopMemory;

#[async_trait]
impl AgentMemory for NoopMemory {
    async fn retain(
        &self,
        _memory: &MemoryRetain,
    ) -> Result<MemoryRetainReceipt, MemoryAdapterError> {
        Err(MemoryAdapterError::Unavailable(
            "subjective memory is disabled".to_owned(),
        ))
    }

    async fn recall(&self, request: &MemoryRecallRequest) -> MemoryRecallOutcome {
        unavailable_or_invalid(request, RecallUnavailableReason::Disabled)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RecordedMemory {
    outcomes: BTreeMap<Uuid, MemoryRecallOutcome>,
}

impl RecordedMemory {
    #[must_use]
    pub fn new(outcomes: impl IntoIterator<Item = MemoryRecallOutcome>) -> Self {
        Self {
            outcomes: outcomes
                .into_iter()
                .map(|outcome| (outcome.request_id(), outcome))
                .collect(),
        }
    }
}

#[async_trait]
impl AgentMemory for RecordedMemory {
    async fn retain(
        &self,
        _memory: &MemoryRetain,
    ) -> Result<MemoryRetainReceipt, MemoryAdapterError> {
        Err(MemoryAdapterError::Rejected(
            "replay never delivers memories to a remote service".to_owned(),
        ))
    }

    async fn recall(&self, request: &MemoryRecallRequest) -> MemoryRecallOutcome {
        let Ok(request_hash) = request.canonical_hash() else {
            return unavailable_or_invalid(request, RecallUnavailableReason::InvalidResponse);
        };
        match self.outcomes.get(&request.request_id) {
            Some(outcome)
                if outcome.request_hash() == request_hash
                    && outcome.validate_against(request).is_ok() =>
            {
                outcome.clone()
            }
            Some(_) => {
                unavailable_or_invalid(request, RecallUnavailableReason::RecordedRequestMismatch)
            }
            None => unavailable_or_invalid(request, RecallUnavailableReason::MissingRecordedInput),
        }
    }
}

fn unavailable_or_invalid(
    request: &MemoryRecallRequest,
    reason: RecallUnavailableReason,
) -> MemoryRecallOutcome {
    match MemoryRecallOutcome::unavailable(request, reason) {
        Ok(outcome) => outcome,
        Err(_) => MemoryRecallOutcome::Unavailable {
            request_id: request.request_id,
            request_hash: Digest::ZERO,
            reason: RecallUnavailableReason::InvalidResponse,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identities() -> (WorldId, EntityId) {
        let world_id = WorldId::from_uuid(Uuid::from_u128(17));
        (
            world_id,
            EntityId::deterministic(world_id, b"memory-test-agent"),
        )
    }

    #[test]
    fn retain_and_recall_identities_are_deterministic_and_life_scoped() {
        let (world_id, agent_id) = identities();
        let first = MemoryRetain::new(
            world_id,
            agent_id,
            EventSequence::new(7),
            SimTick::new(5),
            0,
            "A sharp chill followed the wind.",
            "direct perception",
        )
        .expect("valid retain");
        let second = MemoryRetain::new(
            world_id,
            agent_id,
            EventSequence::new(7),
            SimTick::new(5),
            0,
            "A sharp chill followed the wind.",
            "direct perception",
        )
        .expect("valid retain");
        assert_eq!(first, second);
        assert!(first.bank_id.contains(&agent_id.to_string()));

        let mut forged = first;
        forged.ordinal = 1;
        assert!(matches!(
            forged.validate(),
            Err(MemoryContractError::InvalidDeterministicIdentity)
        ));
    }

    #[tokio::test]
    async fn recorded_memory_refuses_a_changed_request() {
        let (world_id, agent_id) = identities();
        let request = MemoryRecallRequest::new(
            world_id,
            agent_id,
            SimTick::new(10),
            SimTick::new(12),
            0,
            "cold wind",
            256,
        )
        .expect("valid request");
        let recorded =
            MemoryRecallOutcome::unavailable(&request, RecallUnavailableReason::AdapterUnavailable)
                .expect("valid outcome");
        let adapter = RecordedMemory::new([recorded]);

        assert_eq!(
            adapter.recall(&request).await.request_id(),
            request.request_id
        );
        let mut changed = request.clone();
        changed.query = "warm shelter".to_owned();
        assert!(matches!(
            adapter.recall(&changed).await,
            MemoryRecallOutcome::Unavailable {
                reason: RecallUnavailableReason::RecordedRequestMismatch,
                ..
            }
        ));

        let mut forged = request;
        forged.ordinal = 1;
        assert!(matches!(
            forged.validate(),
            Err(MemoryContractError::InvalidDeterministicIdentity)
        ));
    }
}
