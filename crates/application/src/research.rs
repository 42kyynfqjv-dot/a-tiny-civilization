use async_trait::async_trait;
use std::collections::BTreeSet;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CancerResearchContractError, CancerResearchContribution, CancerResearchEvidenceKind,
    CancerResearchEvidenceReference, CancerResearchInferenceTier, CancerResearchStage,
    CancerResearchTurnSelection, Digest, EntityId,
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
pub const MAX_CANCER_RESEARCH_CATALOG_ENTRIES: usize = 256;
pub const CANCER_RESEARCH_CATALOG_PAGE_SIZE: usize = 24;
const MAX_PROVIDER_RESPONSE_ID_BYTES: usize = 256;
const MAX_MODEL_ID_BYTES: usize = 256;
const MAX_ADAPTER_VERSION_BYTES: usize = 128;

/// Stable synthetic identity for the observer-side research collective. It is
/// never an organism and gives Cancer World one Hindsight bank isolated from
/// every resident's subjective-memory bank.
#[must_use]
pub fn cancer_research_collective_id(world_id: world_domain::WorldId) -> EntityId {
    EntityId::deterministic(world_id, b"cancer-research-collective:v1")
}

#[must_use]
pub fn cancer_research_memory_bank_id(world_id: world_domain::WorldId) -> String {
    crate::memory_bank_id(world_id, cancer_research_collective_id(world_id))
}

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchCatalogItem {
    pub ordinal: u32,
    pub contribution_id: Uuid,
    pub artifact_hash: Digest,
    pub artifact_kind: world_domain::CancerResearchArtifactKind,
    pub title: String,
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

    pub fn from_internal_catalog(
        contribution: &CancerResearchContribution,
    ) -> Result<Self, CancerResearchModelContractError> {
        #[derive(Serialize)]
        struct CatalogEntry<'a> {
            catalog_schema_version: u16,
            artifact_kind: world_domain::CancerResearchArtifactKind,
            title: &'a str,
            abstract_text: &'a str,
            claim_statements: Vec<&'a str>,
        }

        let text = serde_json::to_string(&CatalogEntry {
            catalog_schema_version: 1,
            artifact_kind: contribution.artifact_kind,
            title: &contribution.title,
            abstract_text: &contribution.abstract_text,
            claim_statements: contribution
                .claims
                .iter()
                .map(|claim| claim.statement.as_str())
                .collect(),
        })
        .map_err(|_| CancerResearchModelContractError::InvalidMemoryInputs)?;
        let memory = Self {
            document_id: contribution.contribution_id,
            source_artifact_hash: contribution.canonical_hash()?,
            evidence_kind: CancerResearchEvidenceKind::PriorResearchArtifact,
            text,
        };
        memory.validate(CancerResearchStage::BlindDiscovery)?;
        Ok(memory)
    }

    pub fn from_internal_catalog_page(
        world_id: world_domain::WorldId,
        before_ordinal: u32,
        page_index: u16,
        entries: &[CancerResearchCatalogItem],
    ) -> Result<Self, CancerResearchModelContractError> {
        #[derive(Serialize)]
        struct CatalogPage<'a> {
            catalog_schema_version: u16,
            before_ordinal: u32,
            page_index: u16,
            entries: &'a [CancerResearchCatalogItem],
        }

        if entries.is_empty() || entries.len() > CANCER_RESEARCH_CATALOG_PAGE_SIZE {
            return Err(CancerResearchModelContractError::InvalidMemoryInputs);
        }
        let text = serde_json::to_string(&CatalogPage {
            catalog_schema_version: 1,
            before_ordinal,
            page_index,
            entries,
        })
        .map_err(|_| CancerResearchModelContractError::InvalidMemoryInputs)?;
        let source_artifact_hash = Digest::sha256(text.as_bytes());
        let document_id = Uuid::new_v5(
            &world_id.as_uuid(),
            format!(
                "cancer-research-catalog:v1:{before_ordinal}:{page_index}:{source_artifact_hash}"
            )
            .as_bytes(),
        );
        let memory = Self {
            document_id,
            source_artifact_hash,
            evidence_kind: CancerResearchEvidenceKind::PriorResearchArtifact,
            text,
        };
        memory.validate(CancerResearchStage::BlindDiscovery)?;
        Ok(memory)
    }
}

/// Deterministic title-level duplicate detector retained for compact catalogue
/// entries. Whole contributions should use [`cancer_research_contributions_duplicate`].
#[must_use]
pub fn cancer_research_titles_duplicate(left: &str, right: &str) -> bool {
    let left = cancer_research_title_terms(left);
    let right = cancer_research_title_terms(right);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left == right {
        return true;
    }
    if cancer_research_terms_contradict(&left, &right) {
        return false;
    }
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    let shorter = left.len().min(right.len());
    intersection >= 4
        && (intersection.saturating_mul(100) >= union.saturating_mul(60)
            || intersection.saturating_mul(100) >= shorter.saturating_mul(75))
}

/// Collapses paraphrases by comparing both the title and the contribution's
/// mechanism-bearing prose. Numeric observations and generic research wording
/// are excluded, while opposing direction/independence qualifiers prevent a
/// scientifically distinct claim from being hidden as a duplicate.
#[must_use]
pub fn cancer_research_contributions_duplicate(
    left: &CancerResearchContribution,
    right: &CancerResearchContribution,
) -> bool {
    if left.artifact_kind != right.artifact_kind || left.stage != right.stage {
        return false;
    }
    if cancer_research_titles_duplicate(&left.title, &right.title) {
        return true;
    }

    let left_title = cancer_research_title_terms(&left.title);
    let right_title = cancer_research_title_terms(&right.title);
    if cancer_research_terms_contradict(&left_title, &right_title) {
        return false;
    }
    let title_intersection = left_title.intersection(&right_title).count();
    let title_shorter = left_title.len().min(right_title.len());
    if title_intersection < 3
        || title_intersection.saturating_mul(100) < title_shorter.saturating_mul(60)
    {
        return false;
    }

    let left_body = cancer_research_contribution_terms(left);
    let right_body = cancer_research_contribution_terms(right);
    if cancer_research_terms_contradict(&left_body, &right_body) {
        return false;
    }
    let body_intersection = left_body.intersection(&right_body).count();
    let body_shorter = left_body.len().min(right_body.len());
    body_intersection >= 10
        && body_intersection.saturating_mul(100) >= body_shorter.saturating_mul(68)
}

fn cancer_research_contribution_terms(
    contribution: &CancerResearchContribution,
) -> BTreeSet<String> {
    let mut terms = cancer_research_text_terms(&contribution.title);
    terms.extend(cancer_research_text_terms(&contribution.abstract_text));
    for claim in &contribution.claims {
        terms.extend(cancer_research_text_terms(&claim.statement));
        terms.extend(cancer_research_text_terms(&claim.testable_prediction));
        terms.extend(cancer_research_text_terms(&claim.falsification_test));
    }
    terms
}

fn cancer_research_title_terms(title: &str) -> BTreeSet<String> {
    cancer_research_text_terms(title)
}

fn cancer_research_text_terms(text: &str) -> BTreeSet<String> {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .filter_map(|term| {
            if term.chars().all(char::is_numeric) {
                return None;
            }
            if matches!(
                term,
                "a" | "adult"
                    | "an"
                    | "and"
                    | "as"
                    | "at"
                    | "by"
                    | "cell"
                    | "cells"
                    | "cohort"
                    | "data"
                    | "day"
                    | "days"
                    | "for"
                    | "from"
                    | "glioblastoma"
                    | "in"
                    | "into"
                    | "its"
                    | "of"
                    | "on"
                    | "patient"
                    | "patients"
                    | "primary"
                    | "propose"
                    | "proposed"
                    | "role"
                    | "study"
                    | "test"
                    | "the"
                    | "their"
                    | "to"
                    | "tumor"
                    | "tumors"
                    | "unit"
                    | "units"
                    | "using"
                    | "we"
            ) {
                return None;
            }
            let normalized = match term {
                "clonal" | "clones" => "clone",
                "activated" | "activates" | "activation" => "activate",
                "associated" | "associates" | "association" | "correlated" | "correlates"
                | "correlation" => "associate",
                "depleted" | "depletion" | "depleting" | "decreases" | "decreased" | "reduces"
                | "reduced" | "reduction" => "decrease",
                "driven" | "driver" | "drivers" | "drives" | "driving" | "induced" | "induces"
                | "promotes" | "promoting" | "promotion" => "drive",
                "engaged" => "engagement",
                "expanded" | "expanding" | "expansion" | "proliferation" | "proliferative" => {
                    "growth"
                }
                "heterogeneity" | "heterogeneous" => "heterogeneous",
                "hypoxic" => "hypoxia",
                "inhibited" | "inhibiting" | "inhibition" => "inhibit",
                "mechanically" => "mechanical",
                "metabolically" => "metabolic",
                "modulated" | "modulates" | "modulation" | "influences" | "influenced"
                | "affects" | "affected" => "modulate",
                "reprogrammed" | "reprogramming" => "reprogram",
                "selected" | "selecting" | "selection" | "selectively" => "select",
                "spatially" => "spatial",
                "suppressed" | "suppresses" | "suppression" => "suppress",
                "trajectories" => "trajectory",
                other => other,
            };
            Some(normalized.to_owned())
        })
        .collect()
}

fn cancer_research_terms_contradict(left: &BTreeSet<String>, right: &BTreeSet<String>) -> bool {
    const OPPOSING_TERMS: [(&str, &str); 6] = [
        ("activate", "inhibit"),
        ("dependent", "independent"),
        ("drive", "independent"),
        ("high", "low"),
        ("increase", "decrease"),
        ("drive", "suppress"),
    ];
    OPPOSING_TERMS.iter().any(|(positive, negative)| {
        (left.contains(*positive) && right.contains(*negative))
            || (left.contains(*negative) && right.contains(*positive))
    })
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

    /// Loads successful artifacts that do not yet have an audit for the current
    /// observer-side novelty method. This never changes world state or model input.
    async fn load_unaudited_cancer_research(
        &self,
        _world_id: world_domain::WorldId,
        _method_version: u16,
        _limit: usize,
    ) -> Result<Vec<crate::CancerResearchNoveltyCandidate>, StoreError> {
        Ok(Vec::new())
    }

    /// Appends one immutable observer-side overlap assessment. Repeating the
    /// exact audit is idempotent; a conflicting audit is corruption.
    async fn store_cancer_research_novelty_audit(
        &self,
        _audit: &world_domain::CancerResearchNoveltyAudit,
    ) -> Result<(), StoreError> {
        Ok(())
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

    /// Loads a bounded, distinct internal catalogue of earlier research. This
    /// is the collective's working wiki: it is derived from immutable results,
    /// never from observer edits, and is supplied to new blinded turns solely
    /// to prevent accidental repetition and support cumulative work.
    async fn load_cancer_research_catalog(
        &self,
        _world_id: world_domain::WorldId,
        _before_ordinal: u32,
        _limit: usize,
    ) -> Result<Vec<CancerResearchMemoryInput>, StoreError> {
        Ok(Vec::new())
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
        CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION, CancerResearchArtifactKind,
        CancerResearchClaim, CancerResearchProfile, CancerResearchTarget, CancerResearchTask,
        EntityId, SimTick, WorldId, WorldSeed,
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

    #[test]
    fn research_title_deduplication_collapses_rewording_without_erasing_qualifiers() {
        assert!(cancer_research_titles_duplicate(
            "Clone Diversity Drives Glioblastoma Growth Trajectory",
            "Clone Diversity as a Driver of Local Growth Trajectory in Adult Glioblastoma",
        ));
        assert!(cancer_research_titles_duplicate(
            "Immune-Cell Density Modulates Clonal Growth Trajectories in Adult Glioblastoma",
            "Spatial Immune-Cell Density Modulates Clonal Growth Trajectories in Adult Glioblastoma",
        ));
        assert!(cancer_research_titles_duplicate(
            "Spatial Immune-Engagement Modulates Clone Expansion in Adult Glioblastoma",
            "Immune-Microenvironment Modulates Clonal Expansion in Adult Glioblastoma",
        ));
        assert!(cancer_research_titles_duplicate(
            "Hypoxia-Induced Metabolic Reprogramming Drives Glioblastoma Clonal Expansion",
            "Hypoxia-Driven Metabolic Reprogramming Promotes Clonal Expansion in Adult Glioblastoma",
        ));
        assert!(!cancer_research_titles_duplicate(
            "Immune Engagement Modulates Clone Diversity in Adult Glioblastoma",
            "Immune Engagement Drives Clonal Expansion in Adult Glioblastoma",
        ));
        assert!(!cancer_research_titles_duplicate(
            "Hypoxia-Independent Immune Modulation of Glioblastoma Proliferation",
            "Hypoxia-Driven Immune Modulation of Glioblastoma Proliferation",
        ));
    }

    #[test]
    fn research_contribution_deduplication_uses_mechanism_prose_and_keeps_opposites() {
        fn contribution(
            ordinal: u128,
            title: &str,
            abstract_text: &str,
            statement: &str,
        ) -> CancerResearchContribution {
            CancerResearchContribution {
                schema_version: CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION,
                contribution_id: Uuid::from_u128(ordinal),
                request_id: Uuid::from_u128(ordinal + 100),
                selection_hash: Digest::sha256(format!("selection-{ordinal}").as_bytes()),
                resident_id: EntityId::deterministic(
                    WorldId::from_uuid(Uuid::from_u128(71)),
                    format!("resident-{ordinal}").as_bytes(),
                ),
                stage: CancerResearchStage::BlindDiscovery,
                artifact_kind: CancerResearchArtifactKind::Hypothesis,
                title: title.to_owned(),
                abstract_text: abstract_text.to_owned(),
                claims: vec![CancerResearchClaim {
                    statement: statement.to_owned(),
                    testable_prediction:
                        "A spatial perturbation changes immune engagement and clone growth."
                            .to_owned(),
                    falsification_test:
                        "The mechanism fails if immune engagement and clone growth remain unchanged."
                            .to_owned(),
                    citation_hashes: Vec::new(),
                }],
                virtual_experiment_plan: None,
            }
        }

        let original = contribution(
            1,
            "Spatial immune engagement shapes local growth",
            "Hypoxia-driven immune engagement selects a spatially heterogeneous clone population and changes its growth trajectory.",
            "Local hypoxia drives immune modulation that selects metabolically reprogrammed clones.",
        );
        let paraphrase = contribution(
            2,
            "Spatial immune response alters expansion",
            "Immune engagement driven by hypoxia selects heterogeneous clones across space and alters their growth trajectory.",
            "Hypoxia drives local immune modulation and selection of metabolically reprogrammed clones.",
        );
        let opposing_mechanism = contribution(
            3,
            "Spatial immune response alters expansion",
            "Hypoxia-independent immune engagement selects heterogeneous clones across space and alters their growth trajectory.",
            "Local immune modulation selects metabolically reprogrammed clones independently of hypoxia.",
        );

        assert!(cancer_research_contributions_duplicate(
            &original,
            &paraphrase
        ));
        assert!(!cancer_research_contributions_duplicate(
            &original,
            &opposing_mechanism
        ));
    }
}
