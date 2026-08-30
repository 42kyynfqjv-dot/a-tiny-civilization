use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    CancerResearchTarget, CanonicalHashError, Digest, EntityId, SimTick, WorldId, WorldSeed,
};

pub const CANCER_RESEARCH_PROFILE_SCHEMA_VERSION: u16 = 1;
pub const CANCER_RESEARCH_TURN_SCHEMA_VERSION: u16 = 1;
pub const LEGACY_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION: u16 = 1;
/// Schema v2 introduced optional executable plans and required them for machine
/// designs. It remains readable because artifacts written before virtual-lab
/// execution became mandatory may be experiment proposals without a plan.
pub const VIRTUAL_PLAN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION: u16 = 2;
/// Schema v3 requires every newly proposed experiment or machine design to carry
/// a closed plan that the deterministic observer-side virtual lab can execute.
pub const REQUIRED_PLAN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION: u16 = 3;
/// Schema v4 permits independent-replication protocols to remain proposals until
/// the observer-side lab has executed their required, frozen plan.
pub const CAMPAIGN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION: u16 = 4;
/// Schema v5 adds an optional structured prediction for a held-out NCI-60 CNS
/// response challenge. New contributions must answer exactly one supplied
/// challenge and may never manufacture a prediction without that evidence.
pub const RESPONSE_CHALLENGE_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION: u16 = 5;
/// Schema v6 adds explicit canonical gene-symbol targets. This lets an
/// observer-side evidence worker perform exact, provenance-bound molecular
/// lookups without interpreting or fuzzy-matching research prose.
pub const CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION: u16 = 6;
pub const LEGACY_CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION: u16 = 1;
/// Schema v2 requires sensing plans to measure detection sensitivity and keeps
/// treatment plans on intervention endpoints. Historical v1 plans stay
/// replayable, including early structurally inert experiment proposals.
pub const CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION: u16 = 2;
pub const LEGACY_CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION: u16 = 1;
/// Schema v2 adds a compact multiscale readout. Historical v1 results remain
/// readable and immutable; the current method writes v2 rows beside them.
pub const CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION: u16 = 2;
pub const CANCER_VIRTUAL_MECHANISTIC_READOUT_SCHEMA_VERSION: u16 = 1;
pub const CANCER_VIRTUAL_LAB_METHOD_VERSION: u16 = 2;
pub const CANCER_RESEARCH_NOVELTY_AUDIT_SCHEMA_VERSION: u16 = 1;
/// Method v2 makes malformed-source filtering and duplicate-source selection
/// total and deterministic. Historical method-v1 audits remain valid records.
pub const CANCER_RESEARCH_NOVELTY_METHOD_VERSION: u16 = 2;
pub const CANCER_NCI60_RESPONSE_PREDICTION_SCHEMA_VERSION: u16 = 1;
pub const CANCER_NCI60_RESPONSE_QUALIFICATION_SCHEMA_VERSION: u16 = 1;
pub const CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION: u16 = 1;
pub const CANCER_PATIENT_DERIVED_MOLECULAR_QUALIFICATION_SCHEMA_VERSION: u16 = 1;
pub const CANCER_PATIENT_DERIVED_MOLECULAR_QUALIFICATION_METHOD_VERSION: u16 = 1;
pub const MAX_CANCER_RESEARCH_NOVELTY_MATCHES: usize = 5;
pub const MAX_CANCER_RESEARCH_NOVELTY_QUERY_TERMS: usize = 8;
pub const MAX_CANCER_RESEARCH_NOVELTY_WARNINGS: usize = 4;
pub const MAX_RESEARCH_EVIDENCE_REFERENCES: usize = 64;
pub const MAX_RESEARCH_CLAIMS: usize = 8;
pub const MAX_RESEARCH_MOLECULAR_TARGETS: usize = 4;
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

/// The two independent Cancer World research programs. The program is derived
/// from the immutable turn ordinal instead of being added to the persisted turn
/// schema, so historical requests retain their original canonical hashes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchProgram {
    Devices,
    Treatments,
}

impl CancerResearchProgram {
    #[must_use]
    pub const fn for_ordinal(ordinal: u32) -> Self {
        if ordinal.is_multiple_of(2) {
            Self::Devices
        } else {
            Self::Treatments
        }
    }

    #[must_use]
    pub const fn ordinal_remainder(self) -> u32 {
        match self {
            Self::Devices => 0,
            Self::Treatments => 1,
        }
    }
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
    DesignDiagnosticInstrument,
    DesignTreatmentMachine,
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
                Self::GenerateMechanisticHypothesis
                    | Self::ProposeDiscriminatingExperiment
                    | Self::DesignDiagnosticInstrument
                    | Self::DesignTreatmentMachine
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
    ResponseChallenge,
    FrozenHypothesis,
    PriorResearchArtifact,
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
        let response_challenge_count = self
            .evidence
            .iter()
            .filter(|reference| reference.kind == CancerResearchEvidenceKind::ResponseChallenge)
            .count();
        if response_challenge_count > 1
            || (response_challenge_count == 1
                && (self.stage != CancerResearchStage::BlindDiscovery
                    || CancerResearchProgram::for_ordinal(self.ordinal)
                        != CancerResearchProgram::Treatments))
        {
            return Err(CancerResearchContractError::EvidenceFirewallViolation);
        }
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
    DiagnosticInstrumentDesign,
    TreatmentMachineDesign,
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
            Self::DiagnosticInstrumentDesign => "diagnostic_instrument_design",
            Self::TreatmentMachineDesign => "treatment_machine_design",
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

/// An exact HGNC-style gene symbol deliberately kept separate from prose.
/// Symbols are identities only: naming one does not assert expression,
/// causality, druggability, safety, or therapeutic effect.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerMolecularTarget {
    pub gene_symbol: String,
}

impl CancerMolecularTarget {
    pub fn validate(&self) -> Result<(), CancerResearchContractError> {
        let bytes = self.gene_symbol.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 32
            || !bytes[0].is_ascii_uppercase()
            || !bytes[bytes.len() - 1].is_ascii_alphanumeric()
            || bytes
                .iter()
                .any(|byte| !(byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'-'))
        {
            return Err(CancerResearchContractError::InvalidContribution);
        }
        Ok(())
    }
}

/// A concrete intervention identity from the public NCI compound catalogues.
/// NSC identifiers are opaque references, not treatment recommendations or a
/// claim that a compound is safe or effective in people.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CancerNciInterventionIdentity {
    SingleAgent { nsc: u64 },
    Combination { nsc_1: u64, nsc_2: u64 },
}

impl CancerNciInterventionIdentity {
    fn validate(&self) -> Result<(), CancerResearchContractError> {
        match self {
            Self::SingleAgent { nsc } if *nsc > 0 => Ok(()),
            Self::Combination { nsc_1, nsc_2 } if *nsc_1 > 0 && nsc_1 < nsc_2 => Ok(()),
            _ => Err(CancerResearchContractError::InvalidResponsePrediction),
        }
    }
}

/// The six CNS lines in the pinned NCI-60 response export. These names describe
/// immortalized in-vitro models and must never be presented as patient cohorts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CancerNci60CnsLine {
    #[serde(alias = "sf-268", alias = "SF-268", alias = "CNS:SF-268")]
    Sf268,
    #[serde(alias = "sf-295", alias = "SF-295", alias = "CNS:SF-295")]
    Sf295,
    #[serde(alias = "sf-539", alias = "SF-539", alias = "CNS:SF-539")]
    Sf539,
    #[serde(alias = "snb-19", alias = "SNB-19", alias = "CNS:SNB-19")]
    Snb19,
    #[serde(alias = "snb-75", alias = "SNB-75", alias = "CNS:SNB-75")]
    Snb75,
    #[serde(alias = "u-251", alias = "U-251", alias = "CNS:U251")]
    U251,
}

/// A falsifiable, label-free ordering submitted before the runtime-isolated
/// labels are opened. Single-agent predictions order activity/sensitivity;
/// combination predictions order greater-than-additive interaction strength.
/// The distinction matters: ALMANAC ComboScore is not total treatment response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerNci60ResponsePrediction {
    pub schema_version: u16,
    pub challenge_id: Uuid,
    pub intervention: CancerNciInterventionIdentity,
    pub predicted_response_order: Vec<CancerNci60CnsLine>,
}

/// One line's observed competition rank. Equal ranks are biological ties and
/// must not be converted into arbitrary alphabetic wins during scoring.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerNci60ObservedRank {
    pub cell_line: CancerNci60CnsLine,
    pub rank: u8,
}

impl CancerNci60ResponsePrediction {
    pub fn validate(&self) -> Result<(), CancerResearchContractError> {
        self.intervention.validate()?;
        let unique = self
            .predicted_response_order
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if self.schema_version != CANCER_NCI60_RESPONSE_PREDICTION_SCHEMA_VERSION
            || self.challenge_id.is_nil()
            || self.predicted_response_order.len() != 6
            || unique.len() != 6
        {
            return Err(CancerResearchContractError::InvalidResponsePrediction);
        }
        Ok(())
    }

    #[must_use]
    pub fn challenge_source_id(&self) -> String {
        match self.intervention {
            CancerNciInterventionIdentity::SingleAgent { nsc } => format!(
                "cancer-world://nci60-response-challenge/{}/single-agent/{nsc}",
                self.challenge_id
            ),
            CancerNciInterventionIdentity::Combination { nsc_1, nsc_2 } => format!(
                "cancer-world://nci60-response-challenge/{}/combination/{nsc_1}-{nsc_2}",
                self.challenge_id
            ),
        }
    }
}

/// Observer-side opening of one preregistered held-out NCI-60 challenge. This
/// measures rank agreement with an in-vitro panel only; it is not evidence of
/// efficacy, safety, mechanism, animal benefit, or clinical benefit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerNci60ResponseQualification {
    pub schema_version: u16,
    pub method_version: u16,
    pub qualification_id: Uuid,
    pub world_id: WorldId,
    pub request_id: Uuid,
    pub artifact_hash: Digest,
    pub prediction_hash: Digest,
    pub challenge_id: Uuid,
    pub intervention: CancerNciInterventionIdentity,
    pub observed_response_ranks: Vec<CancerNci60ObservedRank>,
    pub pairwise_comparison_count: u8,
    pub concordant_pair_count: u8,
    /// None means every observed line tied, leaving no informative pair.
    pub pairwise_concordance_per_mille: Option<u16>,
    pub most_responsive_line_correct: Option<bool>,
    pub least_responsive_line_correct: Option<bool>,
    pub answer_key: CancerResearchEvidenceReference,
    pub limitations: Vec<String>,
}

impl CancerNci60ResponseQualification {
    #[must_use]
    pub fn deterministic_id(request_id: Uuid, method_version: u16) -> Uuid {
        Uuid::new_v5(
            &request_id,
            format!("observer-nci60-response-qualification:v{method_version}").as_bytes(),
        )
    }

    pub fn validate_against(
        &self,
        contribution: &CancerResearchContribution,
    ) -> Result<(), CancerResearchContractError> {
        let prediction = contribution
            .nci60_response_prediction
            .as_ref()
            .ok_or(CancerResearchContractError::InvalidResponseQualification)?;
        prediction.validate()?;
        self.intervention.validate()?;
        let observed_unique = self
            .observed_response_ranks
            .iter()
            .map(|observation| observation.cell_line)
            .collect::<std::collections::BTreeSet<_>>();
        let (comparisons, concordant, per_mille, top_correct, bottom_correct) =
            response_rank_concordance(
                &prediction.predicted_response_order,
                &self.observed_response_ranks,
            )?;
        if self.schema_version != CANCER_NCI60_RESPONSE_QUALIFICATION_SCHEMA_VERSION
            || self.method_version != CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION
            || self.qualification_id != Self::deterministic_id(self.request_id, self.method_version)
            || self.request_id != contribution.request_id
            || self.artifact_hash != contribution.canonical_hash()?
            || self.prediction_hash != Digest::canonical(prediction)?
            || self.challenge_id != prediction.challenge_id
            || self.intervention != prediction.intervention
            || self.observed_response_ranks.len() != 6
            || observed_unique.len() != 6
            || self.pairwise_comparison_count != comparisons
            || self.concordant_pair_count != concordant
            || self.pairwise_concordance_per_mille != per_mille
            || self.most_responsive_line_correct != top_correct
            || self.least_responsive_line_correct != bottom_correct
            || self.answer_key.kind != CancerResearchEvidenceKind::AssayObservation
            || self.answer_key.validate().is_err()
            || !(3..=6).contains(&self.limitations.len())
            || self
                .limitations
                .iter()
                .any(|limitation| !bounded_text(limitation, 512))
        {
            return Err(CancerResearchContractError::InvalidResponseQualification);
        }
        Ok(())
    }

    pub fn canonical_hash(
        &self,
        contribution: &CancerResearchContribution,
    ) -> Result<Digest, CancerResearchContractError> {
        self.validate_against(contribution)?;
        Ok(Digest::canonical(self)?)
    }
}

/// Exact target-level result of looking up a structured gene symbol in a
/// patient-derived tumor-model proteome. It is a molecular-presence check only;
/// no treatment, response, causality, or efficacy label exists in this type.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerPatientDerivedTargetStatus {
    Observed,
    NotDetected,
    Unresolved,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerPatientDerivedTargetObservation {
    pub target: CancerMolecularTarget,
    pub protein_ids: Vec<String>,
    /// Models for which this exact target had an assay row. Zero means the
    /// submitted symbol could not be resolved without fuzzy inference.
    pub assayed_model_count: u16,
    /// Models with a non-missing reported value for an exact matching row.
    pub observed_model_count: u16,
    pub status: CancerPatientDerivedTargetStatus,
}

impl CancerPatientDerivedTargetObservation {
    fn validate(&self, cohort_model_count: u16) -> Result<(), CancerResearchContractError> {
        self.target.validate()?;
        let valid_protein_ids = self.protein_ids.len() <= 32
            && self.protein_ids.windows(2).all(|pair| pair[0] < pair[1])
            && self
                .protein_ids
                .iter()
                .all(|value| value.trim() == value && !value.is_empty() && value.len() <= 128);
        let counts_valid = self.observed_model_count <= self.assayed_model_count
            && self.assayed_model_count <= cohort_model_count;
        let status_valid = match self.status {
            CancerPatientDerivedTargetStatus::Observed => {
                self.assayed_model_count == cohort_model_count
                    && self.observed_model_count > 0
                    && !self.protein_ids.is_empty()
            }
            CancerPatientDerivedTargetStatus::NotDetected => {
                self.assayed_model_count == cohort_model_count
                    && self.observed_model_count == 0
                    && !self.protein_ids.is_empty()
            }
            CancerPatientDerivedTargetStatus::Unresolved => {
                self.assayed_model_count == 0
                    && self.observed_model_count == 0
                    && self.protein_ids.is_empty()
            }
        };
        if !valid_protein_ids || !counts_valid || !status_valid {
            return Err(CancerResearchContractError::InvalidPatientDerivedQualification);
        }
        Ok(())
    }
}

/// Observer-side molecular corroboration against an immutable PDC-derived
/// cohort artifact. Patient-linked source rows remain outside the public event
/// log and model memory; only exact target coverage is retained here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerPatientDerivedMolecularQualification {
    pub schema_version: u16,
    pub method_version: u16,
    pub qualification_id: Uuid,
    pub world_id: WorldId,
    pub request_id: Uuid,
    pub artifact_hash: Digest,
    pub source: CancerResearchEvidenceReference,
    pub pdc_study_id: String,
    pub study_version_id: Uuid,
    pub source_file_id: Uuid,
    pub source_file_md5: String,
    pub cohort_model_count: u16,
    pub target_observations: Vec<CancerPatientDerivedTargetObservation>,
    pub limitations: Vec<String>,
}

impl CancerPatientDerivedMolecularQualification {
    #[must_use]
    pub fn deterministic_id(request_id: Uuid, method_version: u16) -> Uuid {
        Uuid::new_v5(
            &request_id,
            format!("observer-patient-derived-molecular-qualification:v{method_version}")
                .as_bytes(),
        )
    }

    pub fn validate_against(
        &self,
        contribution: &CancerResearchContribution,
    ) -> Result<(), CancerResearchContractError> {
        let expected_targets = contribution.molecular_targets.to_vec();
        let observed_targets = self
            .target_observations
            .iter()
            .map(|observation| observation.target.clone())
            .collect::<Vec<_>>();
        if self.schema_version != CANCER_PATIENT_DERIVED_MOLECULAR_QUALIFICATION_SCHEMA_VERSION
            || self.method_version != CANCER_PATIENT_DERIVED_MOLECULAR_QUALIFICATION_METHOD_VERSION
            || self.qualification_id != Self::deterministic_id(self.request_id, self.method_version)
            || self.request_id != contribution.request_id
            || self.artifact_hash != contribution.canonical_hash()?
            || self.source.kind != CancerResearchEvidenceKind::RawDataset
            || self.source.validate().is_err()
            || self.pdc_study_id != "PDC000711"
            || self.study_version_id.is_nil()
            || self.source_file_id.is_nil()
            || self.source_file_md5.len() != 32
            || !self
                .source_file_md5
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || self.cohort_model_count == 0
            || self.cohort_model_count > 10_000
            || expected_targets.is_empty()
            || observed_targets != expected_targets
            || self
                .target_observations
                .iter()
                .any(|observation| observation.validate(self.cohort_model_count).is_err())
            || !(4..=6).contains(&self.limitations.len())
            || self
                .limitations
                .iter()
                .any(|limitation| !bounded_text(limitation, 512))
        {
            return Err(CancerResearchContractError::InvalidPatientDerivedQualification);
        }
        Ok(())
    }

    pub fn canonical_hash(
        &self,
        contribution: &CancerResearchContribution,
    ) -> Result<Digest, CancerResearchContractError> {
        self.validate_against(contribution)?;
        Ok(Digest::canonical(self)?)
    }
}

type CancerNci60RankScore = (u8, u8, Option<u16>, Option<bool>, Option<bool>);

fn response_rank_concordance(
    predicted: &[CancerNci60CnsLine],
    observed: &[CancerNci60ObservedRank],
) -> Result<CancerNci60RankScore, CancerResearchContractError> {
    if predicted.len() != 6 || observed.len() != 6 {
        return Err(CancerResearchContractError::InvalidResponseQualification);
    }
    let predicted_positions = predicted
        .iter()
        .enumerate()
        .map(|(index, line)| (*line, index))
        .collect::<std::collections::BTreeMap<_, _>>();
    if predicted_positions.len() != 6
        || observed
            .iter()
            .map(|observation| observation.cell_line)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != 6
        || observed.iter().enumerate().any(|(index, observation)| {
            observation.rank == 0
                || usize::from(observation.rank) > index + 1
                || (index > 0
                    && (observed[index - 1].rank > observation.rank
                        || (observed[index - 1].rank == observation.rank
                            && observed[index - 1].cell_line >= observation.cell_line)
                        || (observed[index - 1].rank < observation.rank
                            && usize::from(observation.rank) != index + 1)))
        })
    {
        return Err(CancerResearchContractError::InvalidResponseQualification);
    }
    let mut concordant = 0_u8;
    let mut comparisons = 0_u8;
    for left in 0..observed.len() {
        for right in (left + 1)..observed.len() {
            if observed[left].rank == observed[right].rank {
                continue;
            }
            comparisons = comparisons.saturating_add(1);
            if predicted_positions[&observed[left].cell_line]
                < predicted_positions[&observed[right].cell_line]
            {
                concordant = concordant.saturating_add(1);
            }
        }
    }
    let per_mille = (comparisons > 0)
        .then(|| {
            u16::try_from(u32::from(concordant) * 1_000 / u32::from(comparisons))
                .map_err(|_| CancerResearchContractError::InvalidResponseQualification)
        })
        .transpose()?;
    let top_rank = observed
        .first()
        .ok_or(CancerResearchContractError::InvalidResponseQualification)?
        .rank;
    let bottom_rank = observed
        .last()
        .ok_or(CancerResearchContractError::InvalidResponseQualification)?
        .rank;
    let predicted_top = predicted
        .first()
        .ok_or(CancerResearchContractError::InvalidResponseQualification)?;
    let predicted_bottom = predicted
        .last()
        .ok_or(CancerResearchContractError::InvalidResponseQualification)?;
    let informative = top_rank != bottom_rank;
    let top_correct = informative.then(|| {
        observed.iter().any(|observation| {
            observation.rank == top_rank && &observation.cell_line == predicted_top
        })
    });
    let bottom_correct = informative.then(|| {
        observed.iter().any(|observation| {
            observation.rank == bottom_rank && &observation.cell_line == predicted_bottom
        })
    });
    Ok((
        comparisons,
        concordant,
        per_mille,
        top_correct,
        bottom_correct,
    ))
}

/// Closed subject models that a research contribution may ask the observer-side
/// virtual lab to execute. These are explicitly computational abstractions, not
/// exact replicas of a person, organoid, or animal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerVirtualSubjectModel {
    CellCulture,
    TumorOrganoid,
    OrthotopicMouse,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerVirtualInterventionModality {
    MolecularInhibition,
    Radiation,
    Thermal,
    ElectricField,
    TargetedDelivery,
    SurgicalResection,
    DiagnosticSensing,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerVirtualMechanismTarget {
    CellDivision,
    DnaRepair,
    ApoptosisResistance,
    HypoxiaAdaptation,
    Angiogenesis,
    ImmuneEvasion,
    Invasion,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerVirtualEndpoint {
    RelativeTumorBurden,
    ViableTumorFraction,
    InvasiveCellFraction,
    HypoxicCellFraction,
    OffTargetHealthyCellLoss,
    DetectionSensitivity,
}

/// Machine-readable projection of a proposed experiment. The closed vocabulary
/// keeps execution deterministic and prevents prose from being mistaken for a
/// completed assay. Intensities and model outputs use millionths, never floats.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerVirtualExperimentPlan {
    pub schema_version: u16,
    pub subject_model: CancerVirtualSubjectModel,
    pub intervention_modality: CancerVirtualInterventionModality,
    pub primary_target: CancerVirtualMechanismTarget,
    pub secondary_target: Option<CancerVirtualMechanismTarget>,
    pub primary_endpoint: CancerVirtualEndpoint,
    pub intensity_parts_per_million: u32,
    pub exposure_hours: u16,
    pub cohort_size: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerVirtualExperimentInterpretation {
    ModelSupportsPrediction,
    ModelShowsNoMaterialEffect,
    ModelShowsConcerningTradeoff,
    ModelInconclusive,
}

/// The fidelity name is intentionally narrow. A structural screen can reject
/// internally weak ideas, but it is neither a calibrated tissue model nor a
/// substitute for biological validation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerVirtualLabFidelity {
    StructuralMultiscaleScreen,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerVirtualCalibrationGrade {
    StructuralUncalibrated,
}

/// Three coarse phenotypic compartments expose treatment selection instead of
/// allowing an apparently smaller tumor to hide enrichment of resistant cells.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerVirtualCloneFractions {
    pub treatment_sensitive_parts_per_million: u32,
    pub drug_tolerant_parts_per_million: u32,
    pub resistant_parts_per_million: u32,
}

impl CancerVirtualCloneFractions {
    fn validate(&self) -> Result<(), CancerResearchContractError> {
        let total = u64::from(self.treatment_sensitive_parts_per_million)
            + u64::from(self.drug_tolerant_parts_per_million)
            + u64::from(self.resistant_parts_per_million);
        if total != 1_000_000 {
            return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
        }
        Ok(())
    }
}

/// Dimensionless, bounded exposure summary for drug-like interventions. It is
/// present only when an orthotopic subject receives molecular inhibition or
/// targeted delivery. Source-calibrated compound profiles will replace these
/// structural values in a later fidelity tier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerVirtualPkReadout {
    pub systemic_exposure_parts_per_million: u32,
    pub bbb_penetration_parts_per_million: u32,
    pub unbound_brain_exposure_parts_per_million: u32,
    pub effective_exposure_hours: u16,
}

impl CancerVirtualPkReadout {
    fn validate(&self) -> Result<(), CancerResearchContractError> {
        if self.systemic_exposure_parts_per_million > 1_000_000
            || self.bbb_penetration_parts_per_million > 1_000_000
            || self.unbound_brain_exposure_parts_per_million > 1_000_000
            || self.effective_exposure_hours == 0
            || self.effective_exposure_hours > 2_160
        {
            return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
        }
        let maximum_brain_exposure = u64::from(self.systemic_exposure_parts_per_million)
            * u64::from(self.bbb_penetration_parts_per_million)
            / 1_000_000;
        if u64::from(self.unbound_brain_exposure_parts_per_million) > maximum_brain_exposure {
            return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
        }
        Ok(())
    }
}

/// Compact mechanistic trace emitted by every current structural screen. It is
/// deliberately small enough to retain at high research volume while exposing
/// the two major failure modes hidden by the old scalar model: inadequate brain
/// exposure and treatment-driven resistant-clone enrichment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerVirtualMechanisticReadout {
    pub schema_version: u16,
    pub fidelity: CancerVirtualLabFidelity,
    pub calibration_grade: CancerVirtualCalibrationGrade,
    pub baseline_clones: CancerVirtualCloneFractions,
    pub post_exposure_clones: CancerVirtualCloneFractions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pharmacokinetics: Option<CancerVirtualPkReadout>,
    pub delivered_exposure_parts_per_million: u32,
    pub target_engagement_parts_per_million: u32,
    pub resistant_selection_parts_per_million: i32,
}

impl CancerVirtualMechanisticReadout {
    fn validate(&self) -> Result<(), CancerResearchContractError> {
        self.baseline_clones.validate()?;
        self.post_exposure_clones.validate()?;
        if self.schema_version != CANCER_VIRTUAL_MECHANISTIC_READOUT_SCHEMA_VERSION
            || self.delivered_exposure_parts_per_million > 1_000_000
            || self.target_engagement_parts_per_million > 1_000_000
            || !(-1_000_000..=1_000_000).contains(&self.resistant_selection_parts_per_million)
            || self.resistant_selection_parts_per_million
                != i32::try_from(self.post_exposure_clones.resistant_parts_per_million)
                    .unwrap_or(i32::MAX)
                    - i32::try_from(self.baseline_clones.resistant_parts_per_million)
                        .unwrap_or(i32::MAX)
        {
            return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
        }
        if let Some(pharmacokinetics) = &self.pharmacokinetics {
            pharmacokinetics.validate()?;
            if self.delivered_exposure_parts_per_million
                != pharmacokinetics.unbound_brain_exposure_parts_per_million
            {
                return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
            }
        }
        Ok(())
    }
}

/// A deterministic result from the deliberately simplified observer-side
/// virtual lab. It is a model projection—not wet-lab evidence, an animal study,
/// a clinical result, or a causal fact inside Cancer World.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerVirtualExperimentResult {
    pub schema_version: u16,
    pub method_version: u16,
    pub experiment_id: Uuid,
    pub world_id: WorldId,
    pub request_id: Uuid,
    pub artifact_hash: Digest,
    pub plan_hash: Digest,
    pub subject_model: CancerVirtualSubjectModel,
    pub primary_endpoint: CancerVirtualEndpoint,
    pub cohort_size: u16,
    pub control_value_parts_per_million: u32,
    pub intervention_value_parts_per_million: u32,
    pub estimated_change_parts_per_million: i32,
    pub uncertainty_low_parts_per_million: i32,
    pub uncertainty_high_parts_per_million: i32,
    pub interpretation: CancerVirtualExperimentInterpretation,
    pub model_calibration: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mechanistic_readout: Option<CancerVirtualMechanisticReadout>,
    pub caveats: Vec<String>,
}

impl CancerVirtualExperimentResult {
    #[must_use]
    pub fn deterministic_id(request_id: Uuid, method_version: u16) -> Uuid {
        Uuid::new_v5(
            &request_id,
            format!("observer-virtual-lab:v{method_version}").as_bytes(),
        )
    }

    pub fn validate_against(
        &self,
        contribution: &CancerResearchContribution,
    ) -> Result<(), CancerResearchContractError> {
        let Some(plan) = contribution.virtual_experiment_plan.as_ref() else {
            return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
        };
        plan.validate()?;
        let legacy = self.schema_version == LEGACY_CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION
            && self.method_version == 1
            && self.model_calibration == "uncalibrated_mechanistic_projection_v1"
            && self.mechanistic_readout.is_none()
            && self.caveats.len() == 2;
        let current = self.schema_version == CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION
            && self.method_version == CANCER_VIRTUAL_LAB_METHOD_VERSION
            && self.model_calibration == "structural_multiscale_projection_v2"
            && self.mechanistic_readout.is_some()
            && (3..=6).contains(&self.caveats.len());
        if (!legacy && !current)
            || self.experiment_id != Self::deterministic_id(self.request_id, self.method_version)
            || self.request_id != contribution.request_id
            || self.artifact_hash != contribution.canonical_hash()?
            || self.plan_hash != Digest::canonical(plan)?
            || self.subject_model != plan.subject_model
            || self.primary_endpoint != plan.primary_endpoint
            || self.cohort_size != plan.cohort_size
            || self.control_value_parts_per_million > 1_000_000
            || self.intervention_value_parts_per_million > 1_000_000
            || self.estimated_change_parts_per_million
                != i32::try_from(self.intervention_value_parts_per_million).unwrap_or(i32::MAX)
                    - i32::try_from(self.control_value_parts_per_million).unwrap_or(i32::MAX)
            || self.uncertainty_low_parts_per_million > self.estimated_change_parts_per_million
            || self.uncertainty_high_parts_per_million < self.estimated_change_parts_per_million
            || self.caveats.iter().any(|caveat| !bounded_text(caveat, 512))
        {
            return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
        }
        if let Some(readout) = &self.mechanistic_readout {
            readout.validate()?;
            let pharmacokinetics_expected = self.subject_model
                == CancerVirtualSubjectModel::OrthotopicMouse
                && matches!(
                    plan.intervention_modality,
                    CancerVirtualInterventionModality::MolecularInhibition
                        | CancerVirtualInterventionModality::TargetedDelivery
                );
            if readout.pharmacokinetics.is_some() != pharmacokinetics_expected {
                return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
            }
        }
        Ok(())
    }

    pub fn canonical_hash(
        &self,
        contribution: &CancerResearchContribution,
    ) -> Result<Digest, CancerResearchContractError> {
        self.validate_against(contribution)?;
        Ok(Digest::canonical(self)?)
    }
}

impl CancerVirtualExperimentPlan {
    pub fn validate(&self) -> Result<(), CancerResearchContractError> {
        if !matches!(
            self.schema_version,
            LEGACY_CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION
                | CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION
        ) || self.intensity_parts_per_million == 0
            || self.intensity_parts_per_million > 1_000_000
            || self.exposure_hours == 0
            || self.exposure_hours > 2_160
            || !(8..=4_096).contains(&self.cohort_size)
            || self.secondary_target == Some(self.primary_target)
        {
            return Err(CancerResearchContractError::InvalidVirtualExperimentPlan);
        }
        Ok(())
    }

    fn validate_for_artifact(
        &self,
        artifact_kind: CancerResearchArtifactKind,
    ) -> Result<(), CancerResearchContractError> {
        self.validate()?;
        let diagnostic =
            self.intervention_modality == CancerVirtualInterventionModality::DiagnosticSensing;
        let diagnostic_endpoint =
            self.primary_endpoint == CancerVirtualEndpoint::DetectionSensitivity;
        if self.schema_version == CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION
            && diagnostic != diagnostic_endpoint
        {
            return Err(CancerResearchContractError::InvalidVirtualExperimentPlan);
        }
        match artifact_kind {
            CancerResearchArtifactKind::DiagnosticInstrumentDesign
                if diagnostic && diagnostic_endpoint => {}
            CancerResearchArtifactKind::TreatmentMachineDesign
                if !diagnostic && !diagnostic_endpoint => {}
            CancerResearchArtifactKind::ExperimentProposal => {}
            _ => return Err(CancerResearchContractError::InvalidVirtualExperimentPlan),
        }
        Ok(())
    }
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub molecular_targets: Vec<CancerMolecularTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub virtual_experiment_plan: Option<CancerVirtualExperimentPlan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nci60_response_prediction: Option<CancerNci60ResponsePrediction>,
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
        Self::new_with_virtual_experiment(
            selection,
            artifact_kind,
            title,
            abstract_text,
            claims,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_virtual_experiment(
        selection: &CancerResearchTurnSelection,
        artifact_kind: CancerResearchArtifactKind,
        title: impl Into<String>,
        abstract_text: impl Into<String>,
        claims: Vec<CancerResearchClaim>,
        virtual_experiment_plan: Option<CancerVirtualExperimentPlan>,
    ) -> Result<Self, CancerResearchContractError> {
        Self::new_with_virtual_experiment_and_response_prediction(
            selection,
            artifact_kind,
            title,
            abstract_text,
            claims,
            virtual_experiment_plan,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_virtual_experiment_and_response_prediction(
        selection: &CancerResearchTurnSelection,
        artifact_kind: CancerResearchArtifactKind,
        title: impl Into<String>,
        abstract_text: impl Into<String>,
        claims: Vec<CancerResearchClaim>,
        virtual_experiment_plan: Option<CancerVirtualExperimentPlan>,
        nci60_response_prediction: Option<CancerNci60ResponsePrediction>,
    ) -> Result<Self, CancerResearchContractError> {
        Self::new_with_structured_evidence_targets(
            selection,
            artifact_kind,
            title,
            abstract_text,
            claims,
            Vec::new(),
            virtual_experiment_plan,
            nci60_response_prediction,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_structured_evidence_targets(
        selection: &CancerResearchTurnSelection,
        artifact_kind: CancerResearchArtifactKind,
        title: impl Into<String>,
        abstract_text: impl Into<String>,
        claims: Vec<CancerResearchClaim>,
        molecular_targets: Vec<CancerMolecularTarget>,
        virtual_experiment_plan: Option<CancerVirtualExperimentPlan>,
        nci60_response_prediction: Option<CancerNci60ResponsePrediction>,
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
            molecular_targets,
            virtual_experiment_plan,
            nci60_response_prediction,
        };
        contribution.validate_against(selection)?;
        Ok(contribution)
    }

    pub fn validate_against(
        &self,
        selection: &CancerResearchTurnSelection,
    ) -> Result<(), CancerResearchContractError> {
        selection.validate()?;
        if !matches!(
            self.schema_version,
            LEGACY_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                | VIRTUAL_PLAN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                | REQUIRED_PLAN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                | CAMPAIGN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                | RESPONSE_CHALLENGE_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                | CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
        ) {
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
            || (self.schema_version < CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                && !self.molecular_targets.is_empty())
            || self.molecular_targets.len() > MAX_RESEARCH_MOLECULAR_TARGETS
            || self
                .molecular_targets
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self
                .molecular_targets
                .iter()
                .any(|target| target.validate().is_err())
            || !artifact_kind_valid_for_stage(self.artifact_kind, self.stage)
            || (self.schema_version == LEGACY_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                && self.virtual_experiment_plan.is_some())
            || self
                .virtual_experiment_plan
                .as_ref()
                .is_some_and(|plan| plan.validate_for_artifact(self.artifact_kind).is_err())
            || (matches!(
                self.artifact_kind,
                CancerResearchArtifactKind::DiagnosticInstrumentDesign
                    | CancerResearchArtifactKind::TreatmentMachineDesign
            ) && matches!(
                self.schema_version,
                VIRTUAL_PLAN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                    | REQUIRED_PLAN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                    | CAMPAIGN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                    | RESPONSE_CHALLENGE_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                    | CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
            ) && self.virtual_experiment_plan.is_none())
            || (self.artifact_kind == CancerResearchArtifactKind::ExperimentProposal
                && matches!(
                    self.schema_version,
                    REQUIRED_PLAN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                        | CAMPAIGN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                        | RESPONSE_CHALLENGE_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                        | CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                )
                && self.virtual_experiment_plan.is_none())
            || self.response_prediction_invalid_for(selection)
        {
            return Err(CancerResearchContractError::InvalidContribution);
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<Digest, CancerResearchContractError> {
        Ok(Digest::canonical(self)?)
    }

    fn response_prediction_invalid_for(&self, selection: &CancerResearchTurnSelection) -> bool {
        let challenges = selection
            .evidence
            .iter()
            .filter(|reference| reference.kind == CancerResearchEvidenceKind::ResponseChallenge)
            .collect::<Vec<_>>();
        if self.schema_version < RESPONSE_CHALLENGE_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION {
            return self.nci60_response_prediction.is_some();
        }
        match (
            challenges.as_slice(),
            self.nci60_response_prediction.as_ref(),
        ) {
            ([], None) => false,
            ([challenge], Some(prediction)) => {
                prediction.validate().is_err()
                    || challenge.source_id != prediction.challenge_source_id()
            }
            _ => true,
        }
    }
}

/// Observer-side assessment of how closely a newly produced artifact resembles
/// earlier work. This is deliberately not called a novelty proof: literature
/// indexes are incomplete and lexical overlap is only a triage signal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchNoveltyStatus {
    KnownOverlap,
    NewCombination,
    NoCloseMatchFound,
    PossibleError,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchNoveltyMatch {
    pub source_id: String,
    pub title: String,
    /// YYYY-MM-DD when supplied by the external index.
    pub published_on: Option<String>,
    pub overlap_per_mille: u16,
}

impl CancerResearchNoveltyMatch {
    pub fn validate(&self) -> Result<(), CancerResearchContractError> {
        if !bounded_text(&self.source_id, MAX_RESEARCH_SOURCE_ID_BYTES)
            || !bounded_text(&self.title, MAX_RESEARCH_TITLE_BYTES)
            || self.overlap_per_mille > 1_000
            || self
                .published_on
                .as_ref()
                .is_some_and(|date| date.trim() != date || date.len() != 10)
        {
            return Err(CancerResearchContractError::InvalidNoveltyAudit);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchNoveltyAudit {
    pub schema_version: u16,
    pub method_version: u16,
    pub audit_id: Uuid,
    pub world_id: WorldId,
    pub request_id: Uuid,
    pub artifact_hash: Digest,
    pub query_terms: Vec<String>,
    pub status: CancerResearchNoveltyStatus,
    pub literature_overlap_per_mille: u16,
    pub prior_world_overlap_per_mille: u16,
    pub matches: Vec<CancerResearchNoveltyMatch>,
    pub warnings: Vec<String>,
}

impl CancerResearchNoveltyAudit {
    #[must_use]
    pub fn deterministic_id(request_id: Uuid, method_version: u16) -> Uuid {
        Uuid::new_v5(
            &request_id,
            format!("observer-novelty-audit:v{method_version}").as_bytes(),
        )
    }

    pub fn validate(&self) -> Result<(), CancerResearchContractError> {
        if self.schema_version != CANCER_RESEARCH_NOVELTY_AUDIT_SCHEMA_VERSION
            || self.method_version == 0
            || self.audit_id != Self::deterministic_id(self.request_id, self.method_version)
            || self.request_id.is_nil()
            || self.artifact_hash == Digest::ZERO
            || self.query_terms.is_empty()
            || self.query_terms.len() > MAX_CANCER_RESEARCH_NOVELTY_QUERY_TERMS
            || self
                .query_terms
                .iter()
                .any(|term| term.trim() != term || term.is_empty() || term.len() > 64)
            || self.query_terms.windows(2).any(|pair| pair[0] >= pair[1])
            || self.literature_overlap_per_mille > 1_000
            || self.prior_world_overlap_per_mille > 1_000
            || self.matches.len() > MAX_CANCER_RESEARCH_NOVELTY_MATCHES
            || self.matches.iter().any(|source| source.validate().is_err())
            || self
                .matches
                .windows(2)
                .any(|pair| pair[0].overlap_per_mille < pair[1].overlap_per_mille)
            || self
                .matches
                .iter()
                .map(|source| source.source_id.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.matches.len()
            || self.warnings.len() > MAX_CANCER_RESEARCH_NOVELTY_WARNINGS
            || self
                .warnings
                .iter()
                .any(|warning| !bounded_text(warning, 512))
        {
            return Err(CancerResearchContractError::InvalidNoveltyAudit);
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<Digest, CancerResearchContractError> {
        self.validate()?;
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
                | CancerResearchArtifactKind::DiagnosticInstrumentDesign
                | CancerResearchArtifactKind::TreatmentMachineDesign
                | CancerResearchArtifactKind::Critique
        ) | (
            CancerResearchStage::LiteratureAudit,
            CancerResearchArtifactKind::LiteratureAudit
                | CancerResearchArtifactKind::Critique
                | CancerResearchArtifactKind::Retraction
        ) | (
            CancerResearchStage::IndependentReplication,
            CancerResearchArtifactKind::ExperimentProposal
                | CancerResearchArtifactKind::ReplicationResult
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
    #[error("cancer virtual experiment plan is invalid or incompatible with its artifact")]
    InvalidVirtualExperimentPlan,
    #[error("cancer virtual experiment result is invalid or mismatched")]
    InvalidVirtualExperimentResult,
    #[error("NCI-60 response prediction is invalid or mismatched to its held-out challenge")]
    InvalidResponsePrediction,
    #[error("NCI-60 response qualification is invalid or crossed its held-out provenance")]
    InvalidResponseQualification,
    #[error("patient-derived molecular qualification is invalid or crossed its provenance")]
    InvalidPatientDerivedQualification,
    #[error("cancer-research novelty audit is invalid or overstates its evidence")]
    InvalidNoveltyAudit,
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

    #[test]
    fn new_experiment_proposals_require_a_plan_without_invalidating_schema_two_history() {
        let (world_id, resident_id, profile) = fixture();
        let selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(30),
            SimTick::new(50),
            2,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::ProposeDiscriminatingExperiment,
            CancerResearchInferenceTier::Exploration,
            profile,
            Vec::new(),
            None,
            2_048,
        )
        .expect("experiment selection");
        let claims = vec![CancerResearchClaim {
            statement: "The intervention changes the selected endpoint.".to_owned(),
            testable_prediction: "The intervention cohort differs from control.".to_owned(),
            falsification_test: "The bounded interval crosses zero.".to_owned(),
            citation_hashes: Vec::new(),
        }];
        let planned = CancerResearchContribution::new_with_virtual_experiment(
            &selection,
            CancerResearchArtifactKind::ExperimentProposal,
            "A closed experiment proposal",
            "A computational plan compares one bounded intervention with its control.",
            claims,
            Some(CancerVirtualExperimentPlan {
                schema_version: CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION,
                subject_model: CancerVirtualSubjectModel::TumorOrganoid,
                intervention_modality: CancerVirtualInterventionModality::MolecularInhibition,
                primary_target: CancerVirtualMechanismTarget::CellDivision,
                secondary_target: None,
                primary_endpoint: CancerVirtualEndpoint::ViableTumorFraction,
                intensity_parts_per_million: 500_000,
                exposure_hours: 168,
                cohort_size: 128,
            }),
        )
        .expect("planned current contribution");
        assert_eq!(
            planned.schema_version,
            CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
        );

        let mut current_without_plan = planned.clone();
        current_without_plan.virtual_experiment_plan = None;
        assert!(matches!(
            current_without_plan.validate_against(&selection),
            Err(CancerResearchContractError::InvalidContribution)
        ));

        let mut historical_schema_two = current_without_plan;
        historical_schema_two.schema_version =
            VIRTUAL_PLAN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION;
        assert!(historical_schema_two.validate_against(&selection).is_ok());

        let mut incoherent_current = planned.clone();
        let plan = incoherent_current
            .virtual_experiment_plan
            .as_mut()
            .expect("current plan");
        plan.intervention_modality = CancerVirtualInterventionModality::DiagnosticSensing;
        assert!(matches!(
            incoherent_current.validate_against(&selection),
            Err(CancerResearchContractError::InvalidContribution)
        ));
        incoherent_current
            .virtual_experiment_plan
            .as_mut()
            .expect("legacy plan")
            .schema_version = LEGACY_CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION;
        assert!(
            incoherent_current.validate_against(&selection).is_ok(),
            "historical plan bytes remain replayable"
        );

        let mut corrupt_schema_four = historical_schema_two;
        corrupt_schema_four.schema_version = CAMPAIGN_CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION;
        assert!(matches!(
            corrupt_schema_four.validate_against(&selection),
            Err(CancerResearchContractError::InvalidContribution)
        ));
    }

    #[test]
    fn held_out_response_prediction_is_bound_to_one_exact_challenge() {
        let (world_id, resident_id, profile) = fixture();
        let challenge_id = Uuid::from_u128(0x600d);
        let prediction = CancerNci60ResponsePrediction {
            schema_version: CANCER_NCI60_RESPONSE_PREDICTION_SCHEMA_VERSION,
            challenge_id,
            intervention: CancerNciInterventionIdentity::Combination {
                nsc_1: 12,
                nsc_2: 34,
            },
            predicted_response_order: vec![
                CancerNci60CnsLine::Sf268,
                CancerNci60CnsLine::Sf295,
                CancerNci60CnsLine::Sf539,
                CancerNci60CnsLine::Snb19,
                CancerNci60CnsLine::Snb75,
                CancerNci60CnsLine::U251,
            ],
        };
        let challenge = CancerResearchEvidenceReference {
            kind: CancerResearchEvidenceKind::ResponseChallenge,
            source_id: prediction.challenge_source_id(),
            content_hash: Digest::sha256(b"prompt-safe challenge without labels"),
        };
        let device_turn = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(39),
            SimTick::new(59),
            2,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::GenerateMechanisticHypothesis,
            CancerResearchInferenceTier::Exploration,
            profile.clone(),
            vec![challenge.clone()],
            None,
            2_048,
        );
        assert!(matches!(
            device_turn,
            Err(CancerResearchContractError::EvidenceFirewallViolation)
        ));
        let selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(40),
            SimTick::new(60),
            3,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::GenerateMechanisticHypothesis,
            CancerResearchInferenceTier::Exploration,
            profile,
            vec![challenge],
            None,
            2_048,
        )
        .expect("challenge selection");
        let contribution =
            CancerResearchContribution::new_with_virtual_experiment_and_response_prediction(
                &selection,
                CancerResearchArtifactKind::Hypothesis,
                "A preregistered response ranking",
                "The proposed mechanism makes a falsifiable ordering prediction before labels open.",
                vec![CancerResearchClaim {
                    statement: "The exact NSC pair may produce heterogeneous CNS-line response."
                        .to_owned(),
                    testable_prediction: "The committed line ordering will agree with held-out assay ranks."
                        .to_owned(),
                    falsification_test: "Held-out pairwise concordance is no better than reversal."
                        .to_owned(),
                    citation_hashes: Vec::new(),
                }],
                None,
                Some(prediction.clone()),
            )
            .expect("bound response prediction");
        assert!(contribution.validate_against(&selection).is_ok());

        let mut mismatched = contribution.clone();
        mismatched
            .nci60_response_prediction
            .as_mut()
            .expect("prediction")
            .intervention = CancerNciInterventionIdentity::SingleAgent { nsc: 12 };
        assert!(matches!(
            mismatched.validate_against(&selection),
            Err(CancerResearchContractError::InvalidContribution)
        ));
    }

    #[test]
    fn nci60_line_labels_accept_exact_source_punctuation_but_serialize_canonically() {
        let cases = [
            ("sf-268", CancerNci60CnsLine::Sf268, "sf268"),
            ("SF-295", CancerNci60CnsLine::Sf295, "sf295"),
            ("CNS:SF-539", CancerNci60CnsLine::Sf539, "sf539"),
            ("snb-19", CancerNci60CnsLine::Snb19, "snb19"),
            ("SNB-75", CancerNci60CnsLine::Snb75, "snb75"),
            ("CNS:U251", CancerNci60CnsLine::U251, "u251"),
        ];
        for (source_label, expected, canonical) in cases {
            let decoded: CancerNci60CnsLine = serde_json::from_str(&format!("\"{source_label}\""))
                .expect("known NCI-60 source label");
            assert_eq!(decoded, expected);
            assert_eq!(
                serde_json::to_string(&decoded).expect("canonical line label"),
                format!("\"{canonical}\"")
            );
        }
        assert!(serde_json::from_str::<CancerNci60CnsLine>("\"unknown-line\"").is_err());
    }

    #[test]
    fn response_qualification_recomputes_rank_concordance_and_provenance() {
        let (world_id, resident_id, profile) = fixture();
        let challenge_id = Uuid::from_u128(0x5151);
        let predicted = vec![
            CancerNci60CnsLine::Sf268,
            CancerNci60CnsLine::Sf295,
            CancerNci60CnsLine::Sf539,
            CancerNci60CnsLine::Snb19,
            CancerNci60CnsLine::Snb75,
            CancerNci60CnsLine::U251,
        ];
        let prediction = CancerNci60ResponsePrediction {
            schema_version: CANCER_NCI60_RESPONSE_PREDICTION_SCHEMA_VERSION,
            challenge_id,
            intervention: CancerNciInterventionIdentity::SingleAgent { nsc: 101 },
            predicted_response_order: predicted.clone(),
        };
        let selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(50),
            SimTick::new(70),
            5,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::GenerateMechanisticHypothesis,
            CancerResearchInferenceTier::Exploration,
            profile,
            vec![CancerResearchEvidenceReference {
                kind: CancerResearchEvidenceKind::ResponseChallenge,
                source_id: prediction.challenge_source_id(),
                content_hash: Digest::sha256(b"challenge"),
            }],
            None,
            2_048,
        )
        .expect("selection");
        let contribution =
            CancerResearchContribution::new_with_virtual_experiment_and_response_prediction(
                &selection,
                CancerResearchArtifactKind::Hypothesis,
                "Response rank",
                "A committed ranking is opened against one held-out assay profile.",
                vec![CancerResearchClaim {
                    statement: "Responses vary by line.".to_owned(),
                    testable_prediction: "The rank is concordant.".to_owned(),
                    falsification_test: "The rank is reversed.".to_owned(),
                    citation_hashes: Vec::new(),
                }],
                None,
                Some(prediction.clone()),
            )
            .expect("contribution");
        let result = CancerNci60ResponseQualification {
            schema_version: CANCER_NCI60_RESPONSE_QUALIFICATION_SCHEMA_VERSION,
            method_version: CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION,
            qualification_id: CancerNci60ResponseQualification::deterministic_id(
                contribution.request_id,
                CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION,
            ),
            world_id,
            request_id: contribution.request_id,
            artifact_hash: contribution.canonical_hash().expect("artifact hash"),
            prediction_hash: Digest::canonical(&prediction).expect("prediction hash"),
            challenge_id,
            intervention: prediction.intervention,
            observed_response_ranks: predicted
                .into_iter()
                .enumerate()
                .map(|(index, cell_line)| CancerNci60ObservedRank {
                    cell_line,
                    rank: u8::try_from(index + 1).expect("six ranks fit u8"),
                })
                .collect(),
            pairwise_comparison_count: 15,
            concordant_pair_count: 15,
            pairwise_concordance_per_mille: Some(1_000),
            most_responsive_line_correct: Some(true),
            least_responsive_line_correct: Some(true),
            answer_key: CancerResearchEvidenceReference {
                kind: CancerResearchEvidenceKind::AssayObservation,
                source_id: "nci-cellminer-nci60:held-out-answer-key-v1".to_owned(),
                content_hash: Digest::sha256(b"answer-key"),
            },
            limitations: vec![
                "Immortalized cell-line rank agreement is not patient efficacy.".to_owned(),
                "The panel contains six CNS lines and no immune microenvironment.".to_owned(),
                "This result does not establish safety, dose, or mechanism.".to_owned(),
            ],
        };
        assert!(result.validate_against(&contribution).is_ok());

        let mut forged = result;
        forged.concordant_pair_count = 14;
        assert!(matches!(
            forged.validate_against(&contribution),
            Err(CancerResearchContractError::InvalidResponseQualification)
        ));
    }

    #[test]
    fn response_rank_scoring_preserves_ties_and_marks_all_ties_uninformative() {
        let predicted = vec![
            CancerNci60CnsLine::Sf268,
            CancerNci60CnsLine::Sf295,
            CancerNci60CnsLine::Sf539,
            CancerNci60CnsLine::Snb19,
            CancerNci60CnsLine::Snb75,
            CancerNci60CnsLine::U251,
        ];
        let partial_ties = predicted
            .iter()
            .copied()
            .zip([1, 1, 3, 4, 5, 6])
            .map(|(cell_line, rank)| CancerNci60ObservedRank { cell_line, rank })
            .collect::<Vec<_>>();
        assert_eq!(
            response_rank_concordance(&predicted, &partial_ties).expect("partial ties"),
            (14, 14, Some(1_000), Some(true), Some(true))
        );

        let all_ties = predicted
            .iter()
            .copied()
            .map(|cell_line| CancerNci60ObservedRank { cell_line, rank: 1 })
            .collect::<Vec<_>>();
        assert_eq!(
            response_rank_concordance(&predicted, &all_ties).expect("all ties"),
            (0, 0, None, None, None)
        );
    }
}
