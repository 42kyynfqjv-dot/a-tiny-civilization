use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    CancerResearchTarget, CanonicalHashError, Digest, EntityId, SimTick, WorldId, WorldSeed,
};

pub const CANCER_RESEARCH_PROFILE_SCHEMA_VERSION: u16 = 1;
pub const CANCER_RESEARCH_TURN_SCHEMA_VERSION: u16 = 1;
pub const CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION: u16 = 1;
pub const MAX_RESEARCH_EVIDENCE_REFERENCES: usize = 64;
pub const MAX_RESEARCH_CLAIMS: usize = 8;
pub const MAX_RESEARCH_CITATIONS: usize = 32;
pub const MAX_RESEARCH_MODEL_OUTPUT_TOKENS: u16 = 4_096;
const MAX_RESEARCH_SOURCE_ID_BYTES: usize = 256;
const MAX_RESEARCH_TITLE_BYTES: usize = 256;
const MAX_RESEARCH_ABSTRACT_BYTES: usize = 8 * 1024;
const MAX_RESEARCH_CLAIM_BYTES: usize = 2 * 1024;
const MAX_RESEARCH_TEST_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchSpecialty {
    CellBiology,
    CancerGenetics,
    Immunology,
    Pharmacology,
    SystemsBiology,
    Bioinformatics,
    Pathology,
    Biostatistics,
    ExperimentalDesign,
    Replication,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchProfile {
    pub schema_version: u16,
    pub specialty: CancerResearchSpecialty,
    pub hypothesis_prior: u8,
    pub exploration_tolerance: u8,
    pub evidentiary_threshold: u8,
    pub replication_preference: u8,
    pub consensus_challenge: u8,
}

impl CancerResearchProfile {
    pub fn seeded(
        seed: WorldSeed,
        resident_id: EntityId,
    ) -> Result<Self, CancerResearchContractError> {
        let digest = Digest::canonical(&(
            "a-tiny-civilization:cancer-research-profile:v1",
            seed,
            resident_id,
        ))?;
        let bytes = digest.as_bytes();
        let specialties = [
            CancerResearchSpecialty::CellBiology,
            CancerResearchSpecialty::CancerGenetics,
            CancerResearchSpecialty::Immunology,
            CancerResearchSpecialty::Pharmacology,
            CancerResearchSpecialty::SystemsBiology,
            CancerResearchSpecialty::Bioinformatics,
            CancerResearchSpecialty::Pathology,
            CancerResearchSpecialty::Biostatistics,
            CancerResearchSpecialty::ExperimentalDesign,
            CancerResearchSpecialty::Replication,
        ];
        let score = |offset: usize| {
            let raw = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
            u8::try_from(raw % 101).expect("modulo 101 fits u8")
        };
        let profile = Self {
            schema_version: CANCER_RESEARCH_PROFILE_SCHEMA_VERSION,
            specialty: specialties[usize::from(bytes[0]) % specialties.len()],
            hypothesis_prior: score(1),
            exploration_tolerance: score(3),
            evidentiary_threshold: score(5),
            replication_preference: score(7),
            consensus_challenge: score(9),
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), CancerResearchContractError> {
        if self.schema_version != CANCER_RESEARCH_PROFILE_SCHEMA_VERSION {
            return Err(CancerResearchContractError::UnsupportedProfileSchema(
                self.schema_version,
            ));
        }
        if [
            self.hypothesis_prior,
            self.exploration_tolerance,
            self.evidentiary_threshold,
            self.replication_preference,
            self.consensus_challenge,
        ]
        .iter()
        .any(|score| *score > 100)
        {
            return Err(CancerResearchContractError::InvalidProfile);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchStage {
    BlindDiscovery,
    LiteratureAudit,
    IndependentReplication,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchInferenceTier {
    Exploration,
    Escalation,
}

impl CancerResearchStage {
    const fn identity_code(self) -> &'static str {
        match self {
            Self::BlindDiscovery => "blind_discovery",
            Self::LiteratureAudit => "literature_audit",
            Self::IndependentReplication => "independent_replication",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchTask {
    GenerateMechanisticHypothesis,
    ProposeDiscriminatingExperiment,
    ChallengeFrozenHypothesis,
    AuditAgainstLiterature,
    DesignIndependentReplication,
    InterpretReplicationResult,
}

impl CancerResearchTask {
    const fn valid_for(self, stage: CancerResearchStage) -> bool {
        matches!(
            (stage, self),
            (
                CancerResearchStage::BlindDiscovery,
                Self::GenerateMechanisticHypothesis | Self::ProposeDiscriminatingExperiment
            ) | (
                CancerResearchStage::LiteratureAudit,
                Self::ChallengeFrozenHypothesis | Self::AuditAgainstLiterature
            ) | (
                CancerResearchStage::IndependentReplication,
                Self::DesignIndependentReplication | Self::InterpretReplicationResult
            )
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchEvidenceKind {
    BiologicalPrimitive,
    RawDataset,
    AssayObservation,
    FrozenHypothesis,
    Literature,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchEvidenceReference {
    pub kind: CancerResearchEvidenceKind,
    pub source_id: String,
    pub content_hash: Digest,
}

impl CancerResearchEvidenceReference {
    fn validate(&self) -> Result<(), CancerResearchContractError> {
        if self.source_id.trim() != self.source_id
            || self.source_id.is_empty()
            || self.source_id.len() > MAX_RESEARCH_SOURCE_ID_BYTES
            || self.content_hash == Digest::ZERO
        {
            return Err(CancerResearchContractError::InvalidEvidenceReferences);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchTurnSelection {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub world_id: WorldId,
    pub resident_id: EntityId,
    pub selected_at_tick: SimTick,
    pub deadline_tick: SimTick,
    pub ordinal: u32,
    pub target: CancerResearchTarget,
    pub stage: CancerResearchStage,
    pub task: CancerResearchTask,
    pub inference_tier: CancerResearchInferenceTier,
    pub profile: CancerResearchProfile,
    pub evidence: Vec<CancerResearchEvidenceReference>,
    pub frozen_candidate_hash: Option<Digest>,
    pub model_max_output_tokens: u16,
}

impl CancerResearchTurnSelection {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world_id: WorldId,
        resident_id: EntityId,
        selected_at_tick: SimTick,
        deadline_tick: SimTick,
        ordinal: u32,
        target: CancerResearchTarget,
        stage: CancerResearchStage,
        task: CancerResearchTask,
        inference_tier: CancerResearchInferenceTier,
        profile: CancerResearchProfile,
        evidence: Vec<CancerResearchEvidenceReference>,
        frozen_candidate_hash: Option<Digest>,
        model_max_output_tokens: u16,
    ) -> Result<Self, CancerResearchContractError> {
        let selection = Self {
            schema_version: CANCER_RESEARCH_TURN_SCHEMA_VERSION,
            request_id: cancer_research_request_id(
                world_id,
                resident_id,
                selected_at_tick,
                ordinal,
                stage,
                inference_tier,
            ),
            world_id,
            resident_id,
            selected_at_tick,
            deadline_tick,
            ordinal,
            target,
            stage,
            task,
            inference_tier,
            profile,
            evidence,
            frozen_candidate_hash,
            model_max_output_tokens,
        };
        selection.validate()?;
        Ok(selection)
    }

    pub fn validate(&self) -> Result<(), CancerResearchContractError> {
        if self.schema_version != CANCER_RESEARCH_TURN_SCHEMA_VERSION {
            return Err(CancerResearchContractError::UnsupportedTurnSchema(
                self.schema_version,
            ));
        }
        if self.request_id
            != cancer_research_request_id(
                self.world_id,
                self.resident_id,
                self.selected_at_tick,
                self.ordinal,
                self.stage,
                self.inference_tier,
            )
        {
            return Err(CancerResearchContractError::InvalidRequestIdentity);
        }
        self.profile.validate()?;
        if self.deadline_tick <= self.selected_at_tick
            || !self.task.valid_for(self.stage)
            || self.model_max_output_tokens == 0
            || self.model_max_output_tokens > MAX_RESEARCH_MODEL_OUTPUT_TOKENS
        {
            return Err(CancerResearchContractError::InvalidTurn);
        }
        if self.evidence.len() > MAX_RESEARCH_EVIDENCE_REFERENCES
            || self.evidence.windows(2).any(|pair| pair[0] >= pair[1])
            || self
                .evidence
                .iter()
                .any(|reference| reference.validate().is_err())
        {
            return Err(CancerResearchContractError::InvalidEvidenceReferences);
        }
        let has_literature = self
            .evidence
            .iter()
            .any(|reference| reference.kind == CancerResearchEvidenceKind::Literature);
        match self.stage {
            CancerResearchStage::BlindDiscovery
                if has_literature
                    || self.frozen_candidate_hash.is_some()
                    || self.inference_tier != CancerResearchInferenceTier::Exploration =>
            {
                return Err(CancerResearchContractError::EvidenceFirewallViolation);
            }
            CancerResearchStage::LiteratureAudit | CancerResearchStage::IndependentReplication
                if self
                    .frozen_candidate_hash
                    .is_none_or(|digest| digest == Digest::ZERO) =>
            {
                return Err(CancerResearchContractError::EvidenceFirewallViolation);
            }
            _ => {}
        }
        if self.inference_tier == CancerResearchInferenceTier::Escalation
            && self
                .frozen_candidate_hash
                .is_none_or(|digest| digest == Digest::ZERO)
        {
            return Err(CancerResearchContractError::EvidenceFirewallViolation);
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<Digest, CancerResearchContractError> {
        self.validate()?;
        Ok(Digest::canonical(self)?)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchArtifactKind {
    Hypothesis,
    ExperimentProposal,
    LiteratureAudit,
    ReplicationResult,
    Critique,
    Retraction,
    Paper,
}

impl CancerResearchArtifactKind {
    const fn identity_code(self) -> &'static str {
        match self {
            Self::Hypothesis => "hypothesis",
            Self::ExperimentProposal => "experiment_proposal",
            Self::LiteratureAudit => "literature_audit",
            Self::ReplicationResult => "replication_result",
            Self::Critique => "critique",
            Self::Retraction => "retraction",
            Self::Paper => "paper",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchClaim {
    pub statement: String,
    pub testable_prediction: String,
    pub falsification_test: String,
    pub citation_hashes: Vec<Digest>,
}

impl CancerResearchClaim {
    fn validate(&self, stage: CancerResearchStage) -> Result<(), CancerResearchContractError> {
        if !bounded_text(&self.statement, MAX_RESEARCH_CLAIM_BYTES)
            || !bounded_text(&self.testable_prediction, MAX_RESEARCH_TEST_BYTES)
            || !bounded_text(&self.falsification_test, MAX_RESEARCH_TEST_BYTES)
            || self.citation_hashes.len() > MAX_RESEARCH_CITATIONS
            || self
                .citation_hashes
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self.citation_hashes.contains(&Digest::ZERO)
            || (stage == CancerResearchStage::BlindDiscovery && !self.citation_hashes.is_empty())
        {
            return Err(CancerResearchContractError::InvalidContribution);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchContribution {
    pub schema_version: u16,
    pub contribution_id: Uuid,
    pub request_id: Uuid,
    pub selection_hash: Digest,
    pub resident_id: EntityId,
    pub stage: CancerResearchStage,
    pub artifact_kind: CancerResearchArtifactKind,
    pub title: String,
    pub abstract_text: String,
    pub claims: Vec<CancerResearchClaim>,
}

impl CancerResearchContribution {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        selection: &CancerResearchTurnSelection,
        artifact_kind: CancerResearchArtifactKind,
        title: impl Into<String>,
        abstract_text: impl Into<String>,
        claims: Vec<CancerResearchClaim>,
    ) -> Result<Self, CancerResearchContractError> {
        let contribution = Self {
            schema_version: CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION,
            contribution_id: Uuid::new_v5(
                &selection.request_id,
                format!("research-contribution:{}", artifact_kind.identity_code()).as_bytes(),
            ),
            request_id: selection.request_id,
            selection_hash: selection.canonical_hash()?,
            resident_id: selection.resident_id,
            stage: selection.stage,
            artifact_kind,
            title: title.into(),
            abstract_text: abstract_text.into(),
            claims,
        };
        contribution.validate_against(selection)?;
        Ok(contribution)
    }

    pub fn validate_against(
        &self,
        selection: &CancerResearchTurnSelection,
    ) -> Result<(), CancerResearchContractError> {
        selection.validate()?;
        if self.schema_version != CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION {
            return Err(CancerResearchContractError::UnsupportedContributionSchema(
                self.schema_version,
            ));
        }
        let expected_id = Uuid::new_v5(
            &selection.request_id,
            format!(
                "research-contribution:{}",
                self.artifact_kind.identity_code()
            )
            .as_bytes(),
        );
        if self.contribution_id != expected_id
            || self.request_id != selection.request_id
            || self.selection_hash != selection.canonical_hash()?
            || self.resident_id != selection.resident_id
            || self.stage != selection.stage
            || !bounded_text(&self.title, MAX_RESEARCH_TITLE_BYTES)
            || !bounded_text(&self.abstract_text, MAX_RESEARCH_ABSTRACT_BYTES)
            || self.claims.is_empty()
            || self.claims.len() > MAX_RESEARCH_CLAIMS
            || self
                .claims
                .iter()
                .any(|claim| claim.validate(self.stage).is_err())
            || !artifact_kind_valid_for_stage(self.artifact_kind, self.stage)
        {
            return Err(CancerResearchContractError::InvalidContribution);
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<Digest, CancerResearchContractError> {
        Ok(Digest::canonical(self)?)
    }
}

fn artifact_kind_valid_for_stage(
    kind: CancerResearchArtifactKind,
    stage: CancerResearchStage,
) -> bool {
    matches!(
        (stage, kind),
        (
            CancerResearchStage::BlindDiscovery,
            CancerResearchArtifactKind::Hypothesis
                | CancerResearchArtifactKind::ExperimentProposal
                | CancerResearchArtifactKind::Critique
        ) | (
            CancerResearchStage::LiteratureAudit,
            CancerResearchArtifactKind::LiteratureAudit
                | CancerResearchArtifactKind::Critique
                | CancerResearchArtifactKind::Retraction
        ) | (
            CancerResearchStage::IndependentReplication,
            CancerResearchArtifactKind::ReplicationResult
                | CancerResearchArtifactKind::Critique
                | CancerResearchArtifactKind::Retraction
                | CancerResearchArtifactKind::Paper
        )
    )
}

fn bounded_text(value: &str, maximum_bytes: usize) -> bool {
    value.trim() == value && !value.is_empty() && value.len() <= maximum_bytes
}

#[must_use]
pub fn cancer_research_request_id(
    world_id: WorldId,
    resident_id: EntityId,
    selected_at_tick: SimTick,
    ordinal: u32,
    stage: CancerResearchStage,
    inference_tier: CancerResearchInferenceTier,
) -> Uuid {
    Uuid::new_v5(
        &world_id.as_uuid(),
        format!(
            "cancer-research:{resident_id}:{}:{ordinal}:{}:{}",
            selected_at_tick.get(),
            stage.identity_code(),
            match inference_tier {
                CancerResearchInferenceTier::Exploration => "exploration",
                CancerResearchInferenceTier::Escalation => "escalation",
            }
        )
        .as_bytes(),
    )
}

#[derive(Debug, Error)]
pub enum CancerResearchContractError {
    #[error("unsupported cancer-research profile schema {0}")]
    UnsupportedProfileSchema(u16),
    #[error("unsupported cancer-research turn schema {0}")]
    UnsupportedTurnSchema(u16),
    #[error("unsupported cancer-research contribution schema {0}")]
    UnsupportedContributionSchema(u16),
    #[error("cancer-research profile is outside its fixed bounds")]
    InvalidProfile,
    #[error("cancer-research request identity is not deterministic")]
    InvalidRequestIdentity,
    #[error("cancer-research turn is invalid")]
    InvalidTurn,
    #[error("cancer-research evidence is oversized, reordered, duplicated, or invalid")]
    InvalidEvidenceReferences,
    #[error("cancer-research evidence firewall was violated")]
    EvidenceFirewallViolation,
    #[error("cancer-research contribution is invalid or mismatched")]
    InvalidContribution,
    #[error("cancer-research canonical hashing failed: {0}")]
    Hash(#[from] CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (WorldId, EntityId, CancerResearchProfile) {
        let world_id = WorldId::from_uuid(Uuid::from_u128(37));
        let resident_id = EntityId::deterministic(world_id, b"cancer-resident-0000");
        let profile =
            CancerResearchProfile::seeded(WorldSeed::new(37), resident_id).expect("seeded profile");
        (world_id, resident_id, profile)
    }

    #[test]
    fn seeded_profiles_are_stable_bounded_and_individually_diverse() {
        let (world_id, resident_id, first) = fixture();
        let repeated = CancerResearchProfile::seeded(WorldSeed::new(37), resident_id)
            .expect("repeated profile");
        let other_id = EntityId::deterministic(world_id, b"cancer-resident-0001");
        let other =
            CancerResearchProfile::seeded(WorldSeed::new(37), other_id).expect("other profile");
        assert_eq!(first, repeated);
        assert_ne!(first, other);
        assert!(first.validate().is_ok());
    }

    #[test]
    fn blind_discovery_cannot_see_literature_or_a_frozen_candidate() {
        let (world_id, resident_id, profile) = fixture();
        let literature = CancerResearchEvidenceReference {
            kind: CancerResearchEvidenceKind::Literature,
            source_id: "doi:10.1000/example".to_owned(),
            content_hash: Digest::sha256(b"paper"),
        };
        let result = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(10),
            SimTick::new(20),
            0,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::GenerateMechanisticHypothesis,
            CancerResearchInferenceTier::Exploration,
            profile,
            vec![literature],
            None,
            2_048,
        );
        assert!(matches!(
            result,
            Err(CancerResearchContractError::EvidenceFirewallViolation)
        ));
    }

    #[test]
    fn frozen_hypothesis_can_advance_to_audited_replication() {
        let (world_id, resident_id, profile) = fixture();
        let frozen = Digest::sha256(b"frozen hypothesis");
        let selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(20),
            SimTick::new(40),
            1,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::LiteratureAudit,
            CancerResearchTask::AuditAgainstLiterature,
            CancerResearchInferenceTier::Exploration,
            profile,
            vec![CancerResearchEvidenceReference {
                kind: CancerResearchEvidenceKind::Literature,
                source_id: "pmid:123".to_owned(),
                content_hash: Digest::sha256(b"literature record"),
            }],
            Some(frozen),
            2_048,
        )
        .expect("audited selection");
        let contribution = CancerResearchContribution::new(
            &selection,
            CancerResearchArtifactKind::LiteratureAudit,
            "Audit of a frozen glioblastoma hypothesis",
            "The frozen hypothesis was compared with the committed literature corpus.",
            vec![CancerResearchClaim {
                statement: "The candidate overlaps one previously described mechanism.".to_owned(),
                testable_prediction: "The overlap should be detectable in the cited dataset."
                    .to_owned(),
                falsification_test: "The overlap is absent under the preregistered comparison."
                    .to_owned(),
                citation_hashes: vec![Digest::sha256(b"literature record")],
            }],
        )
        .expect("bounded contribution");
        assert!(contribution.validate_against(&selection).is_ok());
        assert_ne!(contribution.canonical_hash().expect("hash"), Digest::ZERO);
    }
}
