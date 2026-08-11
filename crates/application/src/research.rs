use async_trait::async_trait;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CancerResearchContractError, CancerResearchContribution, CancerResearchEvidenceKind,
    CancerResearchEvidenceReference, CancerResearchInferenceTier, CancerResearchStage,
    CancerResearchTurnSelection, Digest,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchPriorResult {
    pub request: CancerResearchModelRequest,
    pub result: CancerResearchLadderResult,
}

impl CancerResearchPriorResult {
    pub fn validate(&self) -> Result<(), CancerResearchModelContractError> {
        self.request.validate()?;
        let registry = match self.request.selection.inference_tier {
            CancerResearchInferenceTier::Exploration => {
                CognitionRouteRegistry::cancer_research_exploration()
            }
            CancerResearchInferenceTier::Escalation => {
                CognitionRouteRegistry::cancer_research_escalation()
            }
        };
        self.result.validate_against(&registry, &self.request)?;
        if self.result.receipt.is_none() {
            return Err(CancerResearchModelContractError::InvalidPriorResult);
        }
        Ok(())
    }

    pub fn contribution(&self) -> &CancerResearchContribution {
        &self
            .result
            .receipt
            .as_ref()
            .expect("validated prior result has a receipt")
            .contribution
    }
}

use crate::{
    CognitionBillingClass, CognitionContractError, CognitionModelRoute, CognitionProviderId,
    CognitionRouteAttempt, CognitionRouteAttemptStatus, CognitionRoutePurpose,
    CognitionRouteRegistry, ModelTokenUsage, StoreError,
};

pub const CANCER_RESEARCH_MODEL_CONTRACT_VERSION: u16 = 1;
pub const MAX_CANCER_RESEARCH_EVIDENCE_DOCUMENT_BYTES: usize = 128 * 1024;
pub const MAX_CANCER_RESEARCH_TOTAL_EVIDENCE_BYTES: usize = 512 * 1024;
pub const MAX_CANCER_RESEARCH_MEMORY_INPUTS: usize = 16;
pub const MAX_CANCER_RESEARCH_MEMORY_BYTES: usize = 16 * 1024;
pub const MAX_CANCER_RESEARCH_NETWORK_ATTEMPTS: u16 = 4;
pub const MAX_CANCER_RESEARCH_LITERATURE_DOCUMENTS: usize = 8;
pub const MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD: u64 = 250_000;
const MAX_PROVIDER_RESPONSE_ID_BYTES: usize = 256;
const MAX_MODEL_ID_BYTES: usize = 256;
const MAX_ADAPTER_VERSION_BYTES: usize = 128;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchEvidenceDocument {
    pub reference: CancerResearchEvidenceReference,
    pub content: String,
}

/// One immutable, content-addressed snapshot obtained from an external research
/// index. The source payload is stored separately by the adapter; only the
/// bounded evidence document crosses the model boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchLiteratureSnapshot {
    pub evidence_id: Uuid,
    pub world_id: world_domain::WorldId,
    pub source_id: String,
    pub title: String,
    pub license: String,
    pub published_at: Option<NaiveDate>,
    pub document: CancerResearchEvidenceDocument,
    pub source_payload: serde_json::Value,
    pub retrieved_at: chrono::DateTime<chrono::Utc>,
}

impl CancerResearchLiteratureSnapshot {
    pub fn validate(&self) -> Result<(), CancerResearchModelContractError> {
        if self.evidence_id.is_nil()
            || self.source_id.trim() != self.source_id
            || self.source_id.is_empty()
            || self.title.trim() != self.title
            || self.title.is_empty()
            || !matches!(self.license.as_str(), "cc by" | "cc0")
            || self.document.reference.kind != CancerResearchEvidenceKind::Literature
            || self.document.reference.source_id != self.source_id
            || !self.source_payload.is_object()
        {
            return Err(CancerResearchModelContractError::InvalidEvidenceDocuments);
        }
        self.document.validate()
    }
}

impl CancerResearchEvidenceDocument {
    fn validate(&self) -> Result<(), CancerResearchModelContractError> {
        if self.content.trim() != self.content
            || self.content.is_empty()
            || self.content.len() > MAX_CANCER_RESEARCH_EVIDENCE_DOCUMENT_BYTES
            || Digest::sha256(self.content.as_bytes()) != self.reference.content_hash
        {
            return Err(CancerResearchModelContractError::InvalidEvidenceDocuments);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchMemoryInput {
    pub document_id: Uuid,
    pub source_artifact_hash: Digest,
    pub evidence_kind: CancerResearchEvidenceKind,
    pub text: String,
}

impl CancerResearchMemoryInput {
    fn validate(&self, stage: CancerResearchStage) -> Result<(), CancerResearchModelContractError> {
        if self.document_id.is_nil()
            || self.source_artifact_hash == Digest::ZERO
            || self.text.trim() != self.text
            || self.text.is_empty()
            || self.text.len() > MAX_CANCER_RESEARCH_MEMORY_BYTES
            || (stage == CancerResearchStage::BlindDiscovery
                && self.evidence_kind == CancerResearchEvidenceKind::Literature)
        {
            return Err(CancerResearchModelContractError::InvalidMemoryInputs);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchModelRequest {
    pub contract_version: u16,
    pub request_id: Uuid,
    pub selection_hash: Digest,
    pub selection: CancerResearchTurnSelection,
    pub evidence_documents: Vec<CancerResearchEvidenceDocument>,
    pub recalled_memories: Vec<CancerResearchMemoryInput>,
}

impl CancerResearchModelRequest {
    pub fn new(
        selection: CancerResearchTurnSelection,
        evidence_documents: Vec<CancerResearchEvidenceDocument>,
        recalled_memories: Vec<CancerResearchMemoryInput>,
    ) -> Result<Self, CancerResearchModelContractError> {
        let request = Self {
            contract_version: CANCER_RESEARCH_MODEL_CONTRACT_VERSION,
            request_id: selection.request_id,
            selection_hash: selection.canonical_hash()?,
            selection,
            evidence_documents,
            recalled_memories,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), CancerResearchModelContractError> {
        if self.contract_version != CANCER_RESEARCH_MODEL_CONTRACT_VERSION {
            return Err(
                CancerResearchModelContractError::UnsupportedContractVersion(self.contract_version),
            );
        }
        self.selection.validate()?;
        if self.request_id != self.selection.request_id
            || self.selection_hash != self.selection.canonical_hash()?
            || self.evidence_documents.len() != self.selection.evidence.len()
            || self
                .evidence_documents
                .iter()
                .zip(&self.selection.evidence)
                .any(|(document, reference)| {
                    &document.reference != reference || document.validate().is_err()
                })
        {
            return Err(CancerResearchModelContractError::InvalidEvidenceDocuments);
        }
        let total_evidence_bytes = self
            .evidence_documents
            .iter()
            .try_fold(0_usize, |total, document| {
                total.checked_add(document.content.len())
            })
            .ok_or(CancerResearchModelContractError::InvalidEvidenceDocuments)?;
        if total_evidence_bytes > MAX_CANCER_RESEARCH_TOTAL_EVIDENCE_BYTES {
            return Err(CancerResearchModelContractError::InvalidEvidenceDocuments);
        }
        if self.recalled_memories.len() > MAX_CANCER_RESEARCH_MEMORY_INPUTS
            || self
                .recalled_memories
                .windows(2)
                .any(|pair| pair[0].document_id >= pair[1].document_id)
            || self
                .recalled_memories
                .iter()
                .any(|memory| memory.validate(self.selection.stage).is_err())
        {
            return Err(CancerResearchModelContractError::InvalidMemoryInputs);
        }
        Ok(())
    }

    #[must_use]
    pub const fn route_purpose(&self) -> CognitionRoutePurpose {
        match self.selection.inference_tier {
            CancerResearchInferenceTier::Exploration => {
                CognitionRoutePurpose::CancerResearchExploration
            }
            CancerResearchInferenceTier::Escalation => {
                CognitionRoutePurpose::CancerResearchEscalation
            }
        }
    }

    pub fn validate_route(
        &self,
        route: &CognitionModelRoute,
    ) -> Result<(), CancerResearchModelContractError> {
        self.validate()?;
        route.validate()?;
        let approved = match self.selection.inference_tier {
            CancerResearchInferenceTier::Exploration => {
                route == &CognitionModelRoute::openrouter_cancer_gpt_oss_20b_free()
            }
            CancerResearchInferenceTier::Escalation => {
                route == &CognitionModelRoute::openrouter_cancer_deepseek_v4_pro()
                    || route == &CognitionModelRoute::openrouter_cancer_deepseek_v4_flash()
            }
        };
        if approved {
            Ok(())
        } else {
            Err(CancerResearchModelContractError::UnapprovedInferenceTierRoute)
        }
    }

    pub fn canonical_hash(&self) -> Result<Digest, CancerResearchModelContractError> {
        self.validate()?;
        Ok(Digest::canonical(self)?)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchModelReceipt {
    pub contract_version: u16,
    pub request_id: Uuid,
    pub request_hash: Digest,
    pub provider: CognitionProviderId,
    pub requested_model: String,
    pub resolved_model: String,
    pub provider_response_id: String,
    pub usage: ModelTokenUsage,
    pub billed_micro_usd: u64,
    pub contribution: CancerResearchContribution,
    pub provider_response_hash: Digest,
    pub adapter_version: String,
}

impl CancerResearchModelReceipt {
    pub fn validate_against(
        &self,
        route: &CognitionModelRoute,
        request: &CancerResearchModelRequest,
    ) -> Result<(), CancerResearchModelContractError> {
        route.validate()?;
        request.validate()?;
        if self.contract_version != CANCER_RESEARCH_MODEL_CONTRACT_VERSION
            || self.request_id != request.request_id
            || self.request_hash != request.canonical_hash()?
            || self.provider != route.provider
            || self.requested_model != route.requested_model
            || self.resolved_model.trim().is_empty()
            || self.resolved_model.len() > MAX_MODEL_ID_BYTES
            || self.provider_response_id.trim().is_empty()
            || self.provider_response_id.len() > MAX_PROVIDER_RESPONSE_ID_BYTES
            || self.provider_response_hash == Digest::ZERO
            || self.adapter_version.trim().is_empty()
            || self.adapter_version.len() > MAX_ADAPTER_VERSION_BYTES
            || self.usage.completion_tokens > u32::from(request.selection.model_max_output_tokens)
        {
            return Err(CancerResearchModelContractError::InvalidReceipt);
        }
        self.contribution.validate_against(&request.selection)?;
        match route.billing_class {
            CognitionBillingClass::FreeAllocation if self.billed_micro_usd != 0 => {
                return Err(CancerResearchModelContractError::FreeRouteReportedCost);
            }
            CognitionBillingClass::PaidApproved if self.resolved_model != route.requested_model => {
                return Err(CancerResearchModelContractError::PaidModelMismatch);
            }
            CognitionBillingClass::TrialCredit | CognitionBillingClass::DevelopmentOnly => {
                return Err(CancerResearchModelContractError::InvalidReceipt);
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CancerResearchModelError {
    #[error("cancer-research model service is unavailable: {0}")]
    Unavailable(String),
    #[error("cancer-research model service rejected the request: {0}")]
    Rejected(String),
    #[error("cancer-research model service returned an invalid response: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait CancerResearchModel: Send + Sync {
    async fn infer_research(
        &self,
        route: &CognitionModelRoute,
        request: &CancerResearchModelRequest,
    ) -> Result<CancerResearchModelReceipt, CancerResearchModelError>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchJobEntry {
    pub request: CancerResearchModelRequest,
    pub claim_count: u32,
}

impl CancerResearchJobEntry {
    pub fn validate(&self) -> Result<(), CancerResearchModelContractError> {
        self.request.validate()?;
        if self.claim_count == 0 {
            return Err(CancerResearchModelContractError::InvalidJob);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchLadderResult {
    pub contract_version: u16,
    pub request_id: Uuid,
    pub route_policy_version: u16,
    pub route_registry_hash: Digest,
    pub attempts: Vec<CognitionRouteAttempt>,
    pub receipt: Option<CancerResearchModelReceipt>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchAttemptPersistenceState {
    Dispatched,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchRouteAttemptRecord {
    pub route_index: u16,
    pub route: CognitionModelRoute,
    pub persistence_state: CancerResearchAttemptPersistenceState,
    pub attempt: Option<CognitionRouteAttempt>,
    pub receipt: Option<CancerResearchModelReceipt>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchPaidAuthorization {
    pub request_id: Uuid,
    pub billing_month: NaiveDate,
    pub reserved_micro_usd: u64,
}

impl CancerResearchPaidAuthorization {
    pub fn validate_against(
        &self,
        entry: &CancerResearchJobEntry,
    ) -> Result<(), CancerResearchModelContractError> {
        entry.validate()?;
        if self.request_id != entry.request.request_id
            || self.billing_month.day() != 1
            || self.reserved_micro_usd == 0
            || self.reserved_micro_usd > MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD
        {
            return Err(CancerResearchModelContractError::InvalidPaidAuthorization);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchPaidReservationDecision {
    Authorized(CancerResearchPaidAuthorization),
    DeniedHardStop,
}

impl CancerResearchRouteAttemptRecord {
    pub fn validate(&self) -> Result<(), CancerResearchModelContractError> {
        self.route.validate()?;
        let valid = match self.persistence_state {
            CancerResearchAttemptPersistenceState::Dispatched => {
                self.attempt.is_none() && self.receipt.is_none()
            }
            CancerResearchAttemptPersistenceState::Completed => {
                self.attempt.as_ref().is_some_and(|attempt| {
                    attempt.route_index == self.route_index
                        && attempt.provider == self.route.provider
                        && attempt.requested_model == self.route.requested_model
                        && attempt.billing_class == self.route.billing_class
                        && matches!(
                            attempt.status,
                            CognitionRouteAttemptStatus::Succeeded
                                | CognitionRouteAttemptStatus::Unavailable
                                | CognitionRouteAttemptStatus::Rejected
                                | CognitionRouteAttemptStatus::InvalidResponse
                        )
                        && ((attempt.status == CognitionRouteAttemptStatus::Succeeded)
                            == self.receipt.is_some())
                })
            }
        };
        if valid {
            Ok(())
        } else {
            Err(CancerResearchModelContractError::InvalidAttemptRecord)
        }
    }
}

impl CancerResearchLadderResult {
    pub fn validate_against(
        &self,
        registry: &CognitionRouteRegistry,
        request: &CancerResearchModelRequest,
    ) -> Result<(), CancerResearchModelContractError> {
        let purpose = request.route_purpose();
        registry.validate(purpose)?;
        request.validate()?;
        if self.contract_version != CANCER_RESEARCH_MODEL_CONTRACT_VERSION
            || self.request_id != request.request_id
            || self.route_policy_version != registry.policy_version
            || self.route_registry_hash != registry.canonical_hash(purpose)?
            || self.attempts.is_empty()
            || self.attempts.len() > registry.routes.len()
        {
            return Err(CancerResearchModelContractError::InvalidLadderResult);
        }
        let mut succeeded_route = None;
        for (position, attempt) in self.attempts.iter().enumerate() {
            let route = &registry.routes[position];
            if usize::from(attempt.route_index) != position
                || attempt.provider != route.provider
                || attempt.requested_model != route.requested_model
                || attempt.billing_class != route.billing_class
            {
                return Err(CancerResearchModelContractError::InvalidLadderResult);
            }
            if matches!(
                attempt.status,
                CognitionRouteAttemptStatus::Succeeded
                    | CognitionRouteAttemptStatus::StoppedAttemptLimit
            ) && position + 1 != self.attempts.len()
            {
                return Err(CancerResearchModelContractError::InvalidLadderResult);
            }
            if attempt.status == CognitionRouteAttemptStatus::Succeeded {
                succeeded_route = Some(route);
            }
        }
        match (succeeded_route, &self.receipt) {
            (Some(route), Some(receipt)) => receipt.validate_against(route, request),
            (None, None) => Ok(()),
            _ => Err(CancerResearchModelContractError::InvalidLadderResult),
        }
    }
}

#[async_trait]
pub trait CancerResearchJobStore: Send + Sync {
    /// Persists an immutable source snapshot. Repeating the exact snapshot is
    /// idempotent; source revisions remain independently auditable.
    async fn store_cancer_research_literature(
        &self,
        _snapshot: &CancerResearchLiteratureSnapshot,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Loads a stable, newest-first evidence set for literature-audit turns.
    async fn load_cancer_research_literature(
        &self,
        _world_id: world_domain::WorldId,
        _limit: usize,
    ) -> Result<Vec<CancerResearchLiteratureSnapshot>, StoreError> {
        Ok(Vec::new())
    }

    /// Inserts one exact content-addressed request. Repeating the identical
    /// request is idempotent; the same ID with different bytes is corruption.
    async fn enqueue_cancer_research_request(
        &self,
        request: &CancerResearchModelRequest,
    ) -> Result<(), StoreError>;

    async fn claim_next_cancer_research_request(
        &self,
        worker_id: &str,
        claim_lease_seconds: u32,
    ) -> Result<Option<CancerResearchJobEntry>, StoreError>;

    async fn reschedule_cancer_research_request(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        error: &str,
        retry_after_seconds: u32,
    ) -> Result<(), StoreError>;

    /// Commits immutable evidence that a network call is about to occur before
    /// any request leaves the process.
    async fn begin_cancer_research_route_attempt(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        route_index: u16,
        route: &CognitionModelRoute,
    ) -> Result<(), StoreError>;

    /// Appends the terminal outcome. A missing receipt is valid only for a
    /// non-success terminal status.
    async fn finish_cancer_research_route_attempt(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        attempt: &CognitionRouteAttempt,
        receipt: Option<&CancerResearchModelReceipt>,
    ) -> Result<(), StoreError>;

    async fn list_cancer_research_route_attempts(
        &self,
        entry: &CancerResearchJobEntry,
    ) -> Result<Vec<CancerResearchRouteAttemptRecord>, StoreError>;

    async fn complete_cancer_research_request(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        registry: &CognitionRouteRegistry,
        result: &CancerResearchLadderResult,
    ) -> Result<(), StoreError>;

    async fn load_cancer_research_result(
        &self,
        request_id: Uuid,
    ) -> Result<Option<CancerResearchLadderResult>, StoreError>;

    /// Returns the newest successful blind-discovery hypothesis before the given
    /// simulation ordinal. Stores that do not support promotion may safely return
    /// none; they must never fabricate a prior result.
    async fn load_latest_cancer_research_hypothesis(
        &self,
        _world_id: world_domain::WorldId,
        _before_ordinal: u32,
    ) -> Result<Option<CancerResearchPriorResult>, StoreError> {
        Ok(None)
    }

    async fn reserve_paid_cancer_research(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        route: &CognitionModelRoute,
        reserved_micro_usd: u64,
    ) -> Result<CancerResearchPaidReservationDecision, StoreError>;

    async fn load_paid_cancer_research_authorization(
        &self,
        entry: &CancerResearchJobEntry,
    ) -> Result<Option<CancerResearchPaidAuthorization>, StoreError>;

    async fn settle_paid_cancer_research(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        authorization: &CancerResearchPaidAuthorization,
        receipt: &CancerResearchModelReceipt,
    ) -> Result<(), StoreError>;

    async fn release_paid_cancer_research(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        authorization: &CancerResearchPaidAuthorization,
    ) -> Result<(), StoreError>;

    async fn mark_paid_cancer_research_indeterminate(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        authorization: &CancerResearchPaidAuthorization,
    ) -> Result<(), StoreError>;
}

#[derive(Debug, Error)]
pub enum CancerResearchModelContractError {
    #[error("unsupported cancer-research model contract {0}")]
    UnsupportedContractVersion(u16),
    #[error("cancer-research evidence documents are missing, mismatched, or oversized")]
    InvalidEvidenceDocuments,
    #[error("cancer-research recalled memories are invalid or violate the evidence firewall")]
    InvalidMemoryInputs,
    #[error("cancer-research model receipt is incomplete or mismatched")]
    InvalidReceipt,
    #[error("cancer-research job is invalid")]
    InvalidJob,
    #[error("cancer-research route-ladder result is invalid")]
    InvalidLadderResult,
    #[error("persisted cancer-research route attempt is invalid")]
    InvalidAttemptRecord,
    #[error("the model route is not approved for the selected cancer-research inference tier")]
    UnapprovedInferenceTierRoute,
    #[error("the paid cancer-research authorization is invalid")]
    InvalidPaidAuthorization,
    #[error("a free cancer-research route reported non-zero cost")]
    FreeRouteReportedCost,
    #[error("a paid cancer-research response used a different model")]
    PaidModelMismatch,
    #[error("a prior cancer-research result is incomplete or invalid")]
    InvalidPriorResult,
    #[error(transparent)]
    Research(#[from] CancerResearchContractError),
    #[error(transparent)]
    Cognition(#[from] CognitionContractError),
    #[error("cancer-research canonical hashing failed: {0}")]
    Hash(#[from] world_domain::CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{
        CancerResearchProfile, CancerResearchTarget, CancerResearchTask, EntityId, SimTick,
        WorldId, WorldSeed,
    };

    fn selection(evidence: Vec<CancerResearchEvidenceReference>) -> CancerResearchTurnSelection {
        let world_id = WorldId::from_uuid(Uuid::from_u128(71));
        let resident_id = EntityId::deterministic(world_id, b"research-model-test");
        CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(10),
            SimTick::new(30),
            0,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::GenerateMechanisticHypothesis,
            CancerResearchInferenceTier::Exploration,
            CancerResearchProfile::seeded(WorldSeed::new(71), resident_id).expect("profile"),
            evidence,
            None,
            2_048,
        )
        .expect("selection")
    }

    #[test]
    fn request_requires_exact_content_addressed_evidence() {
        let content = "bounded raw assay values";
        let reference = CancerResearchEvidenceReference {
            kind: CancerResearchEvidenceKind::RawDataset,
            source_id: "dataset:test-v1".to_owned(),
            content_hash: Digest::sha256(content.as_bytes()),
        };
        let selection = selection(vec![reference.clone()]);
        let request = CancerResearchModelRequest::new(
            selection.clone(),
            vec![CancerResearchEvidenceDocument {
                reference,
                content: content.to_owned(),
            }],
            Vec::new(),
        )
        .expect("content-addressed request");
        assert_eq!(
            request.route_purpose(),
            CognitionRoutePurpose::CancerResearchExploration
        );

        let mut changed = request;
        changed.evidence_documents[0].content.push_str(" changed");
        assert!(changed.validate().is_err());
    }

    #[test]
    fn blind_request_rejects_recalled_literature() {
        let selection = selection(Vec::new());
        let request = CancerResearchModelRequest::new(
            selection,
            Vec::new(),
            vec![CancerResearchMemoryInput {
                document_id: Uuid::from_u128(1),
                source_artifact_hash: Digest::sha256(b"paper"),
                evidence_kind: CancerResearchEvidenceKind::Literature,
                text: "A paper summary".to_owned(),
            }],
        );
        assert!(matches!(
            request,
            Err(CancerResearchModelContractError::InvalidMemoryInputs)
        ));
    }
}
