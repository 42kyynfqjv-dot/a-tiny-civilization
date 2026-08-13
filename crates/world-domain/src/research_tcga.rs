use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{Digest, WorldId};

use crate::research::{
    CancerMolecularTarget, CancerResearchContribution, CancerResearchEvidenceKind,
    CancerResearchEvidenceReference,
};

pub const CANCER_TCGA_GBM_TARGET_CONTEXT_SCHEMA_VERSION: u16 = 1;
pub const CANCER_TCGA_GBM_TARGET_CONTEXT_METHOD_VERSION: u16 = 1;

/// Whether an exact artifact target was part of the feature set selected using
/// calibration patients only. Absence from that set is deliberately not
/// interpreted as absence of a variant in GBM.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerTcgaGbmTargetContextStatus {
    Evaluated,
    OutsideCalibrationFeatureSet,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerTcgaGbmTargetContextObservation {
    pub target: CancerMolecularTarget,
    pub status: CancerTcgaGbmTargetContextStatus,
    pub calibration_prevalence_parts_per_million: Option<u32>,
    pub held_out_prevalence_parts_per_million: Option<u32>,
    pub absolute_error_parts_per_million: Option<u32>,
}

impl CancerTcgaGbmTargetContextObservation {
    fn validate(&self) -> Result<(), CancerTcgaGbmTargetContextError> {
        self.target
            .validate()
            .map_err(|_| CancerTcgaGbmTargetContextError::InvalidQualification)?;
        match self.status {
            CancerTcgaGbmTargetContextStatus::Evaluated => {
                let calibration = self
                    .calibration_prevalence_parts_per_million
                    .ok_or(CancerTcgaGbmTargetContextError::InvalidQualification)?;
                let held_out = self
                    .held_out_prevalence_parts_per_million
                    .ok_or(CancerTcgaGbmTargetContextError::InvalidQualification)?;
                if calibration > 1_000_000
                    || held_out > 1_000_000
                    || self.absolute_error_parts_per_million != Some(calibration.abs_diff(held_out))
                {
                    return Err(CancerTcgaGbmTargetContextError::InvalidQualification);
                }
            }
            CancerTcgaGbmTargetContextStatus::OutsideCalibrationFeatureSet => {
                if self.calibration_prevalence_parts_per_million.is_some()
                    || self.held_out_prevalence_parts_per_million.is_some()
                    || self.absolute_error_parts_per_million.is_some()
                {
                    return Err(CancerTcgaGbmTargetContextError::InvalidQualification);
                }
            }
        }
        Ok(())
    }
}

/// Observer-side prevalence context from the frozen open TCGA-GBM aggregate.
/// This records protein-altering somatic-variant prevalence only; it is not an
/// expression assay, causal mechanism, intervention response, or clinical
/// qualification.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerTcgaGbmTargetContextQualification {
    pub schema_version: u16,
    pub method_version: u16,
    pub qualification_id: Uuid,
    pub world_id: WorldId,
    pub request_id: Uuid,
    pub artifact_hash: Digest,
    pub source: CancerResearchEvidenceReference,
    pub baseline_id: String,
    pub data_release: String,
    pub calibration_profiled_patient_count: u16,
    pub held_out_profiled_patient_count: u16,
    pub feature_selection: String,
    pub target_observations: Vec<CancerTcgaGbmTargetContextObservation>,
    pub limitations: Vec<String>,
}

impl CancerTcgaGbmTargetContextQualification {
    #[must_use]
    pub fn deterministic_id(request_id: Uuid, method_version: u16) -> Uuid {
        Uuid::new_v5(
            &request_id,
            format!("observer-tcga-gbm-target-context:v{method_version}").as_bytes(),
        )
    }

    pub fn validate_against(
        &self,
        contribution: &CancerResearchContribution,
    ) -> Result<(), CancerTcgaGbmTargetContextError> {
        let artifact_hash = contribution
            .canonical_hash()
            .map_err(|_| CancerTcgaGbmTargetContextError::InvalidQualification)?;
        let expected_targets = contribution.molecular_targets.as_slice();
        let observed_targets = self
            .target_observations
            .iter()
            .map(|observation| &observation.target)
            .collect::<Vec<_>>();
        if self.schema_version != CANCER_TCGA_GBM_TARGET_CONTEXT_SCHEMA_VERSION
            || self.method_version != CANCER_TCGA_GBM_TARGET_CONTEXT_METHOD_VERSION
            || self.qualification_id != Self::deterministic_id(self.request_id, self.method_version)
            || self.request_id != contribution.request_id
            || self.artifact_hash != artifact_hash
            || self.source.kind != CancerResearchEvidenceKind::RawDataset
            || self.source.source_id != "gdc://TCGA-GBM/DR46/open-aggregate"
            || self.source.content_hash == Digest::ZERO
            || self.baseline_id != "tcga-gbm-dr46-patient-baseline-v1"
            || self.data_release != "Data Release 46.0 - August 10, 2026"
            || self.calibration_profiled_patient_count != 303
            || self.held_out_profiled_patient_count != 71
            || self.feature_selection
                != "top 25 protein-altering genes selected using calibration patients only"
            || expected_targets.is_empty()
            || observed_targets.len() != expected_targets.len()
            || observed_targets
                .iter()
                .zip(expected_targets)
                .any(|(observed, expected)| *observed != expected)
            || self
                .target_observations
                .iter()
                .any(|observation| observation.validate().is_err())
            || self.limitations.len() != 5
            || self.limitations.iter().any(|limitation| {
                limitation.trim() != limitation || limitation.is_empty() || limitation.len() > 512
            })
        {
            return Err(CancerTcgaGbmTargetContextError::InvalidQualification);
        }
        Ok(())
    }

    pub fn canonical_hash(
        &self,
        contribution: &CancerResearchContribution,
    ) -> Result<Digest, CancerTcgaGbmTargetContextError> {
        self.validate_against(contribution)?;
        Digest::canonical(self).map_err(|_| CancerTcgaGbmTargetContextError::InvalidQualification)
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CancerTcgaGbmTargetContextError {
    #[error("TCGA-GBM target context is invalid or crosses immutable provenance")]
    InvalidQualification,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> CancerMolecularTarget {
        CancerMolecularTarget {
            gene_symbol: "EGFR".to_owned(),
        }
    }

    #[test]
    fn evaluated_context_requires_a_consistent_held_out_difference() {
        let valid = CancerTcgaGbmTargetContextObservation {
            target: target(),
            status: CancerTcgaGbmTargetContextStatus::Evaluated,
            calibration_prevalence_parts_per_million: Some(194_719),
            held_out_prevalence_parts_per_million: Some(197_183),
            absolute_error_parts_per_million: Some(2_464),
        };
        assert!(valid.validate().is_ok());
        let mut inconsistent = valid;
        inconsistent.absolute_error_parts_per_million = Some(2_463);
        assert!(inconsistent.validate().is_err());
    }

    #[test]
    fn outside_feature_set_cannot_smuggle_a_zero_prevalence() {
        let mut observation = CancerTcgaGbmTargetContextObservation {
            target: target(),
            status: CancerTcgaGbmTargetContextStatus::OutsideCalibrationFeatureSet,
            calibration_prevalence_parts_per_million: None,
            held_out_prevalence_parts_per_million: None,
            absolute_error_parts_per_million: None,
        };
        assert!(observation.validate().is_ok());
        observation.held_out_prevalence_parts_per_million = Some(0);
        assert!(observation.validate().is_err());
    }

    #[test]
    fn qualification_identity_is_request_and_method_bound() {
        let request_id =
            Uuid::parse_str("d75ecbde-a72d-4b22-975f-89ce3eb3e163").expect("request UUID");
        assert_eq!(
            CancerTcgaGbmTargetContextQualification::deterministic_id(request_id, 1),
            CancerTcgaGbmTargetContextQualification::deterministic_id(request_id, 1)
        );
        assert_ne!(
            CancerTcgaGbmTargetContextQualification::deterministic_id(request_id, 1),
            CancerTcgaGbmTargetContextQualification::deterministic_id(request_id, 2)
        );
    }
}
