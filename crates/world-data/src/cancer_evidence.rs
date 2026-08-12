//! Versioned registry and admission boundary for Cancer World patient-derived evidence.
//!
//! Registry membership is not scientific admission. It records where a dataset lives,
//! what it can plausibly inform, and which legal and causal limitations must survive
//! normalization. Patient-level bytes never belong in this public repository.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use world_domain::Digest;

pub const CANCER_DATASET_REGISTRY_SCHEMA_VERSION: u16 = 1;
pub const CANCER_DATASET_REGISTRY_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.cancer-dataset-registry+json";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerDatasetAccessClass {
    PublicUnauthenticated,
    AccountAndTerms,
    MixedOpenAndControlled,
    MaterialRequestAndPublicMetadata,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerDatasetModality {
    ClinicalAnnotation,
    CopyNumber,
    DnaMethylation,
    Genomics,
    GrowthKinetics,
    Histology,
    InSituHybridization,
    Lipidomics,
    Metabolomics,
    PatientDerivedModels,
    Phosphoproteomics,
    Proteomics,
    SingleCellTranscriptomics,
    SomaticVariants,
    SpatialTranscriptomics,
    Transcriptomics,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerSpecimenContext {
    NormalBrainComparator,
    PatientDerivedCulture,
    PatientDerivedOrganoid,
    PatientDerivedXenograft,
    PrimaryResection,
    RapidAutopsy,
    RecurrentResection,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerDatasetPipelineRole {
    EvolutionCalibration,
    HeldOutValidationCandidate,
    MechanismContext,
    ModelQualification,
    PopulationPrior,
    SpatialCalibration,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerDatasetAdmissionStatus {
    RegistryOnlyTermsAndArtifactsUnverified,
    SourceSnapshotVerified,
    NormalizedCalibrationEligible,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerMissingParameterPolicy {
    RejectRatherThanInvent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerQualificationRequirement {
    BaselineReproduction,
    CalibrationValidationPatientSeparation,
    CrossDatasetGeneralization,
    ExternalFalsificationProtocol,
    ReproducibleFrozenInputs,
    StrongBaselineComparison,
    UncertaintyAndSensitivityAnalysis,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerDatasetSource {
    pub source_id: String,
    pub title: String,
    pub custodian: String,
    pub canonical_url: String,
    pub documentation_url: String,
    pub terms_url: String,
    pub version_locator: String,
    pub reviewed_on: NaiveDate,
    pub access_class: CancerDatasetAccessClass,
    pub admission_status: CancerDatasetAdmissionStatus,
    pub disease_scope: String,
    pub modalities: Vec<CancerDatasetModality>,
    pub specimen_contexts: Vec<CancerSpecimenContext>,
    pub pipeline_roles: Vec<CancerDatasetPipelineRole>,
    pub patient_level_linkage: bool,
    pub contains_intervention_outcomes: bool,
    pub supports_counterfactual_treatment_claims: bool,
    pub patient_data_redistributable_in_public_repo: bool,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerDatasetAdmissionPolicy {
    pub patient_data_may_enter_public_repository: bool,
    pub controlled_data_requires_documented_authorization: bool,
    pub calibration_and_validation_split_unit: String,
    pub missing_numeric_parameter_policy: CancerMissingParameterPolicy,
    pub minimum_independent_validation_datasets: u8,
    pub required_qualification_evidence: Vec<CancerQualificationRequirement>,
    pub candidate_claim: String,
    pub prohibited_claim: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerDatasetRegistry {
    pub registry_schema_version: u16,
    pub registry_id: String,
    pub disease_scope: String,
    pub policy: CancerDatasetAdmissionPolicy,
    pub sources: Vec<CancerDatasetSource>,
}

impl CancerDatasetRegistry {
    pub fn validate(&self) -> Result<(), CancerDatasetRegistryError> {
        if self.registry_schema_version != CANCER_DATASET_REGISTRY_SCHEMA_VERSION {
            return Err(CancerDatasetRegistryError::UnsupportedSchema(
                self.registry_schema_version,
            ));
        }
        validate_slug(&self.registry_id)?;
        require_text(&self.disease_scope)?;
        self.policy.validate()?;
        if self.sources.len() < 4 {
            return Err(CancerDatasetRegistryError::InsufficientSourceComposition);
        }
        if self
            .sources
            .windows(2)
            .any(|pair| pair[0].source_id >= pair[1].source_id)
        {
            return Err(CancerDatasetRegistryError::NonCanonicalSourceOrder);
        }
        for source in &self.sources {
            source.validate()?;
        }
        if !self.sources.iter().any(|source| {
            source
                .specimen_contexts
                .contains(&CancerSpecimenContext::RapidAutopsy)
        }) || !self.sources.iter().any(|source| {
            source
                .pipeline_roles
                .contains(&CancerDatasetPipelineRole::EvolutionCalibration)
        }) || !self.sources.iter().any(|source| {
            source
                .pipeline_roles
                .contains(&CancerDatasetPipelineRole::SpatialCalibration)
        }) {
            return Err(CancerDatasetRegistryError::MissingRequiredEvidenceAxis);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CancerDatasetRegistryError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| CancerDatasetRegistryError::Encoding(error.to_string()))
    }

    pub fn content_digest(&self) -> Result<Digest, CancerDatasetRegistryError> {
        Ok(Digest::sha256(&self.canonical_bytes()?))
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, CancerDatasetRegistryError> {
        let registry: Self = serde_json::from_slice(bytes)
            .map_err(|error| CancerDatasetRegistryError::Decode(error.to_string()))?;
        registry.validate()?;
        Ok(registry)
    }
}

impl CancerDatasetAdmissionPolicy {
    fn validate(&self) -> Result<(), CancerDatasetRegistryError> {
        if self.patient_data_may_enter_public_repository
            || !self.controlled_data_requires_documented_authorization
            || self.calibration_and_validation_split_unit != "patient"
            || self.missing_numeric_parameter_policy
                != CancerMissingParameterPolicy::RejectRatherThanInvent
            || self.minimum_independent_validation_datasets < 2
        {
            return Err(CancerDatasetRegistryError::UnsafeAdmissionPolicy);
        }
        validate_sorted_unique(&self.required_qualification_evidence)?;
        let required = BTreeSet::from([
            CancerQualificationRequirement::BaselineReproduction,
            CancerQualificationRequirement::CalibrationValidationPatientSeparation,
            CancerQualificationRequirement::CrossDatasetGeneralization,
            CancerQualificationRequirement::ExternalFalsificationProtocol,
            CancerQualificationRequirement::ReproducibleFrozenInputs,
            CancerQualificationRequirement::StrongBaselineComparison,
            CancerQualificationRequirement::UncertaintyAndSensitivityAnalysis,
        ]);
        if self
            .required_qualification_evidence
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            != required
        {
            return Err(CancerDatasetRegistryError::IncompleteQualificationGate);
        }
        require_text(&self.candidate_claim)?;
        require_text(&self.prohibited_claim)?;
        Ok(())
    }
}

impl CancerDatasetSource {
    fn validate(&self) -> Result<(), CancerDatasetRegistryError> {
        validate_slug(&self.source_id)?;
        require_text(&self.title)?;
        require_text(&self.custodian)?;
        validate_https_url(&self.canonical_url)?;
        validate_https_url(&self.documentation_url)?;
        validate_https_url(&self.terms_url)?;
        require_text(&self.version_locator)?;
        require_text(&self.disease_scope)?;
        validate_sorted_unique(&self.modalities)?;
        validate_sorted_unique(&self.specimen_contexts)?;
        validate_sorted_unique(&self.pipeline_roles)?;
        if self.modalities.is_empty()
            || self.specimen_contexts.is_empty()
            || self.pipeline_roles.is_empty()
            || self.limitations.is_empty()
        {
            return Err(CancerDatasetRegistryError::IncompleteSource(
                self.source_id.clone(),
            ));
        }
        if self.supports_counterfactual_treatment_claims
            || self.patient_data_redistributable_in_public_repo
            || self.admission_status
                != CancerDatasetAdmissionStatus::RegistryOnlyTermsAndArtifactsUnverified
        {
            return Err(CancerDatasetRegistryError::UnsafeSourceClaim(
                self.source_id.clone(),
            ));
        }
        let mut limitations = BTreeSet::new();
        for limitation in &self.limitations {
            require_text(limitation)?;
            if !limitations.insert(limitation) {
                return Err(CancerDatasetRegistryError::DuplicateLimitation(
                    self.source_id.clone(),
                ));
            }
        }
        Ok(())
    }
}

fn validate_sorted_unique<T: Ord>(values: &[T]) -> Result<(), CancerDatasetRegistryError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(CancerDatasetRegistryError::NonCanonicalValueOrder);
    }
    Ok(())
}

fn validate_slug(value: &str) -> Result<(), CancerDatasetRegistryError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(CancerDatasetRegistryError::InvalidIdentifier);
    }
    Ok(())
}

fn require_text(value: &str) -> Result<(), CancerDatasetRegistryError> {
    if value.trim().is_empty() || value.len() > 2_048 {
        return Err(CancerDatasetRegistryError::InvalidText);
    }
    Ok(())
}

fn validate_https_url(value: &str) -> Result<(), CancerDatasetRegistryError> {
    let url = Url::parse(value).map_err(|_| CancerDatasetRegistryError::InvalidUrl)?;
    if url.scheme() != "https" || url.host_str().is_none() || url.username() != "" {
        return Err(CancerDatasetRegistryError::InvalidUrl);
    }
    Ok(())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CancerDatasetRegistryError {
    #[error("unsupported Cancer World dataset-registry schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid Cancer World dataset-registry identifier")]
    InvalidIdentifier,
    #[error("invalid or oversized Cancer World dataset-registry text")]
    InvalidText,
    #[error("Cancer World dataset-registry URL must be absolute HTTPS")]
    InvalidUrl,
    #[error("Cancer World dataset registry needs at least four independent sources")]
    InsufficientSourceComposition,
    #[error("Cancer World dataset sources are not strictly ordered by source_id")]
    NonCanonicalSourceOrder,
    #[error("Cancer World dataset values are not strictly ordered and unique")]
    NonCanonicalValueOrder,
    #[error("Cancer World dataset registry is missing spatial, longitudinal, or autopsy evidence")]
    MissingRequiredEvidenceAxis,
    #[error("Cancer World dataset admission policy permits leakage or invented parameters")]
    UnsafeAdmissionPolicy,
    #[error("Cancer World dataset qualification gate is incomplete")]
    IncompleteQualificationGate,
    #[error("Cancer World dataset source {0:?} is incomplete")]
    IncompleteSource(String),
    #[error("Cancer World dataset source {0:?} makes an unverified causal or redistribution claim")]
    UnsafeSourceClaim(String),
    #[error("Cancer World dataset source {0:?} repeats a limitation")]
    DuplicateLimitation(String),
    #[error("failed to decode Cancer World dataset registry: {0}")]
    Decode(String),
    #[error("failed to encode Cancer World dataset registry: {0}")]
    Encoding(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &[u8] =
        include_bytes!("../../../data/cancer-research/gbm-dataset-registry-v1.json");

    #[test]
    fn committed_registry_is_valid_and_content_addressable() {
        let registry = CancerDatasetRegistry::from_slice(REGISTRY).expect("valid registry");
        assert_eq!(registry.sources.len(), 6);
        assert_ne!(registry.content_digest().expect("digest"), Digest::ZERO);
    }

    #[test]
    fn observational_sources_cannot_claim_counterfactual_treatments() {
        let mut registry = CancerDatasetRegistry::from_slice(REGISTRY).expect("valid registry");
        registry.sources[0].supports_counterfactual_treatment_claims = true;
        assert!(matches!(
            registry.validate(),
            Err(CancerDatasetRegistryError::UnsafeSourceClaim(_))
        ));
    }

    #[test]
    fn qualification_cannot_leak_a_patient_across_the_split() {
        let mut registry = CancerDatasetRegistry::from_slice(REGISTRY).expect("valid registry");
        registry.policy.calibration_and_validation_split_unit = "sample".to_owned();
        assert_eq!(
            registry.validate(),
            Err(CancerDatasetRegistryError::UnsafeAdmissionPolicy)
        );
    }
}
