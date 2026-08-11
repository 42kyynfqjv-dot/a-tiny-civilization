use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CancerResearchContractError, CancerResearchContribution, CancerResearchEvidenceKind,
    CancerResearchEvidenceReference, CancerResearchInferenceTier, CancerResearchStage,
    CancerResearchTurnSelection, Digest,
};

use crate::{
    CognitionBillingClass, CognitionContractError, CognitionModelRoute, CognitionProviderId,
    CognitionRoutePurpose, ModelTokenUsage,
};

pub const CANCER_RESEARCH_MODEL_CONTRACT_VERSION: u16 = 1;
pub const MAX_CANCER_RESEARCH_EVIDENCE_DOCUMENT_BYTES: usize = 128 * 1024;
pub const MAX_CANCER_RESEARCH_TOTAL_EVIDENCE_BYTES: usize = 512 * 1024;
pub const MAX_CANCER_RESEARCH_MEMORY_INPUTS: usize = 16;
pub const MAX_CANCER_RESEARCH_MEMORY_BYTES: usize = 16 * 1024;
const MAX_PROVIDER_RESPONSE_ID_BYTES: usize = 256;
const MAX_MODEL_ID_BYTES: usize = 256;
const MAX_ADAPTER_VERSION_BYTES: usize = 128;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchEvidenceDocument {
    pub reference: CancerResearchEvidenceReference,
    pub content: String,
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
    #[error("a free cancer-research route reported non-zero cost")]
    FreeRouteReportedCost,
    #[error("a paid cancer-research response used a different model")]
    PaidModelMismatch,
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
