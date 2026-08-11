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
pub const CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION: u16 = 3;
pub const CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION: u16 = 1;
pub const CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION: u16 = 1;
pub const CANCER_VIRTUAL_LAB_METHOD_VERSION: u16 = 1;
pub const CANCER_RESEARCH_NOVELTY_AUDIT_SCHEMA_VERSION: u16 = 1;
pub const CANCER_RESEARCH_NOVELTY_METHOD_VERSION: u16 = 1;
pub const MAX_CANCER_RESEARCH_NOVELTY_MATCHES: usize = 5;
pub const MAX_CANCER_RESEARCH_NOVELTY_QUERY_TERMS: usize = 8;
pub const MAX_CANCER_RESEARCH_NOVELTY_WARNINGS: usize = 4;
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
        if self.schema_version != CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION
            || self.method_version == 0
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
            || self.model_calibration != "uncalibrated_mechanistic_projection_v1"
            || self.caveats.len() != 2
            || self.caveats.iter().any(|caveat| !bounded_text(caveat, 512))
        {
            return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
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
        if self.schema_version != CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION
            || self.intensity_parts_per_million == 0
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub virtual_experiment_plan: Option<CancerVirtualExperimentPlan>,
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
            virtual_experiment_plan,
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
                    | CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
            ) && self.virtual_experiment_plan.is_none())
            || (self.artifact_kind == CancerResearchArtifactKind::ExperimentProposal
                && self.schema_version == CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION
                && self.virtual_experiment_plan.is_none())
        {
            return Err(CancerResearchContractError::InvalidContribution);
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<Digest, CancerResearchContractError> {
        Ok(Digest::canonical(self)?)
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
            || self.matches.iter().any(|source| {
                !bounded_text(&source.source_id, MAX_RESEARCH_SOURCE_ID_BYTES)
                    || !bounded_text(&source.title, MAX_RESEARCH_TITLE_BYTES)
                    || source.overlap_per_mille > 1_000
                    || source
                        .published_on
                        .as_ref()
                        .is_some_and(|date| date.trim() != date || date.len() != 10)
            })
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
    #[error("cancer virtual experiment plan is invalid or incompatible with its artifact")]
    InvalidVirtualExperimentPlan,
    #[error("cancer virtual experiment result is invalid or mismatched")]
    InvalidVirtualExperimentResult,
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
    }
}
