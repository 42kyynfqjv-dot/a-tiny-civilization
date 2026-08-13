use std::collections::BTreeMap;

use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CANCER_TCGA_GBM_TARGET_CONTEXT_METHOD_VERSION, CANCER_TCGA_GBM_TARGET_CONTEXT_SCHEMA_VERSION,
    CancerMolecularTarget, CancerResearchContribution, CancerResearchEvidenceKind,
    CancerResearchEvidenceReference, CancerTcgaGbmTargetContextObservation,
    CancerTcgaGbmTargetContextQualification, CancerTcgaGbmTargetContextStatus, Digest, WorldId,
};

const BASELINE_SHA256: &str = "f523989c2bec5ee14c0ff2c6dc30d193fb324e1dd234aba524bef179553294da";
const BASELINE_ID: &str = "tcga-gbm-dr46-patient-baseline-v1";
const DATA_RELEASE: &str = "Data Release 46.0 - August 10, 2026";
const FEATURE_SELECTION: &str =
    "top 25 protein-altering genes selected using calibration patients only";
const CALIBRATION_PROFILED_PATIENTS: u16 = 303;
const HELD_OUT_PROFILED_PATIENTS: u16 = 71;

#[derive(Clone, Debug)]
pub struct CancerTcgaGbmTargetContextCandidate {
    pub world_id: WorldId,
    pub request_id: Uuid,
    pub artifact_hash: Digest,
    pub contribution: CancerResearchContribution,
}

#[derive(Debug, Deserialize)]
struct Baseline {
    schema_version: u16,
    baseline_id: String,
    evidence_class: String,
    intended_use: String,
    source: Source,
    split: Split,
    calibration_cohort: Cohort,
    held_out_validation_cohort: Cohort,
    held_out_assessment: HeldOutAssessment,
    limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Source {
    source_id: String,
    data_release: String,
    data_release_date: String,
    api_commit: String,
    api_version: String,
    mutation_file_set_hash: Digest,
    mutation_file_count: usize,
}

#[derive(Debug, Deserialize)]
struct Split {
    derivation_domain: String,
    calibration_patient_count: usize,
    held_out_validation_patient_count: usize,
    calibration_patient_set_commitment: Digest,
    held_out_validation_patient_set_commitment: Digest,
}

#[derive(Debug, Deserialize)]
struct Cohort {
    molecularly_profiled_patient_count: usize,
}

#[derive(Debug, Deserialize)]
struct HeldOutAssessment {
    predictor: String,
    feature_selection: String,
    evaluated_gene_count: usize,
    predictions: Vec<Prediction>,
    interpretation: String,
}

#[derive(Clone, Debug, Deserialize)]
struct Prediction {
    gene: String,
    calibration_prevalence_parts_per_million: u32,
    held_out_prevalence_parts_per_million: u32,
    absolute_error_parts_per_million: u32,
}

/// Immutable exact-target lookup over a public, patient-disjoint TCGA-GBM
/// aggregate. The underlying patient rows never enter this object or research
/// memory.
pub struct CancerTcgaGbmTargetContextQualifier {
    source: CancerResearchEvidenceReference,
    predictions: BTreeMap<String, Prediction>,
}

impl CancerTcgaGbmTargetContextQualifier {
    pub fn new(baseline_bytes: &[u8]) -> Result<Self, CancerTcgaGbmTargetContextBuildError> {
        let content_hash = Digest::sha256(baseline_bytes);
        if content_hash.to_string() != BASELINE_SHA256 {
            return Err(invalid("TCGA-GBM aggregate content hash changed"));
        }
        let baseline: Baseline = serde_json::from_slice(baseline_bytes)?;
        validate_baseline(&baseline)?;
        let mut predictions = BTreeMap::new();
        for prediction in baseline.held_out_assessment.predictions {
            validate_prediction(&prediction)?;
            if predictions
                .insert(prediction.gene.clone(), prediction)
                .is_some()
            {
                return Err(invalid("TCGA-GBM aggregate repeats a prediction gene"));
            }
        }
        if predictions.len() != 25 {
            return Err(invalid(
                "TCGA-GBM aggregate does not retain its 25 calibration-selected features",
            ));
        }
        Ok(Self {
            source: CancerResearchEvidenceReference {
                kind: CancerResearchEvidenceKind::RawDataset,
                source_id: "gdc://TCGA-GBM/DR46/open-aggregate".to_owned(),
                content_hash,
            },
            predictions,
        })
    }

    pub fn qualify(
        &self,
        candidate: &CancerTcgaGbmTargetContextCandidate,
    ) -> Result<CancerTcgaGbmTargetContextQualification, CancerTcgaGbmTargetContextBuildError> {
        if candidate.request_id != candidate.contribution.request_id
            || candidate.artifact_hash != candidate.contribution.canonical_hash()?
            || candidate.contribution.molecular_targets.is_empty()
        {
            return Err(invalid(
                "TCGA-GBM target-context candidate crossed artifact provenance",
            ));
        }
        let target_observations = candidate
            .contribution
            .molecular_targets
            .iter()
            .map(|target| self.lookup(target))
            .collect();
        let qualification = CancerTcgaGbmTargetContextQualification {
            schema_version: CANCER_TCGA_GBM_TARGET_CONTEXT_SCHEMA_VERSION,
            method_version: CANCER_TCGA_GBM_TARGET_CONTEXT_METHOD_VERSION,
            qualification_id: CancerTcgaGbmTargetContextQualification::deterministic_id(
                candidate.request_id,
                CANCER_TCGA_GBM_TARGET_CONTEXT_METHOD_VERSION,
            ),
            world_id: candidate.world_id,
            request_id: candidate.request_id,
            artifact_hash: candidate.artifact_hash,
            source: self.source.clone(),
            baseline_id: BASELINE_ID.to_owned(),
            data_release: DATA_RELEASE.to_owned(),
            calibration_profiled_patient_count: CALIBRATION_PROFILED_PATIENTS,
            held_out_profiled_patient_count: HELD_OUT_PROFILED_PATIENTS,
            feature_selection: FEATURE_SELECTION.to_owned(),
            target_observations,
            limitations: vec![
                "This is retrospective aggregate somatic-variant prevalence, not a prospective or interventional result.".to_owned(),
                "A protein-altering variant is not expression, target dependence, druggability, treatment response, or causal evidence.".to_owned(),
                "Only the top 25 genes selected in calibration patients are evaluated; an unresolved target is not evidence of absence.".to_owned(),
                "The held-out cohort checks prevalence stability only and was not exposed to Cancer World research prompts or memory.".to_owned(),
                "This result cannot establish efficacy, safety, clinical benefit, or a cure and requires independent experimental validation.".to_owned(),
            ],
        };
        qualification.validate_against(&candidate.contribution)?;
        Ok(qualification)
    }

    fn lookup(&self, target: &CancerMolecularTarget) -> CancerTcgaGbmTargetContextObservation {
        self.predictions.get(&target.gene_symbol).map_or_else(
            || CancerTcgaGbmTargetContextObservation {
                target: target.clone(),
                status: CancerTcgaGbmTargetContextStatus::OutsideCalibrationFeatureSet,
                calibration_prevalence_parts_per_million: None,
                held_out_prevalence_parts_per_million: None,
                absolute_error_parts_per_million: None,
            },
            |prediction| CancerTcgaGbmTargetContextObservation {
                target: target.clone(),
                status: CancerTcgaGbmTargetContextStatus::Evaluated,
                calibration_prevalence_parts_per_million: Some(
                    prediction.calibration_prevalence_parts_per_million,
                ),
                held_out_prevalence_parts_per_million: Some(
                    prediction.held_out_prevalence_parts_per_million,
                ),
                absolute_error_parts_per_million: Some(prediction.absolute_error_parts_per_million),
            },
        )
    }
}

fn validate_baseline(baseline: &Baseline) -> Result<(), CancerTcgaGbmTargetContextBuildError> {
    if baseline.schema_version != 1
        || baseline.baseline_id != BASELINE_ID
        || baseline.evidence_class != "retrospective_observational_aggregate"
        || baseline.intended_use
            != "Population and somatic-variant baseline checks for Cancer World; not intervention-response calibration."
        || baseline.source.source_id != "tcga-gbm-2013"
        || baseline.source.data_release != DATA_RELEASE
        || baseline.source.data_release_date != "2026-08-10"
        || baseline.source.api_commit != "8f7c2a51ab0084b216ad1b62a3fae8b945439c53"
        || baseline.source.api_version != "8.5.0"
        || baseline.source.mutation_file_set_hash == Digest::ZERO
        || baseline.source.mutation_file_count != 464
        || baseline.split.derivation_domain != "a-tiny-civilization/tcga-gbm/patient-split/v1"
        || baseline.split.calibration_patient_count != 492
        || baseline.split.held_out_validation_patient_count != 125
        || baseline.split.calibration_patient_set_commitment == Digest::ZERO
        || baseline.split.held_out_validation_patient_set_commitment == Digest::ZERO
        || baseline
            .calibration_cohort
            .molecularly_profiled_patient_count
            != usize::from(CALIBRATION_PROFILED_PATIENTS)
        || baseline
            .held_out_validation_cohort
            .molecularly_profiled_patient_count
            != usize::from(HELD_OUT_PROFILED_PATIENTS)
        || baseline.held_out_assessment.predictor != "calibration_cohort_empirical_gene_prevalence"
        || baseline.held_out_assessment.feature_selection != FEATURE_SELECTION
        || baseline.held_out_assessment.evaluated_gene_count != 25
        || baseline.held_out_assessment.predictions.len() != 25
        || baseline.held_out_assessment.interpretation
            != "This is the simple out-of-sample molecular baseline a future Cancer World genomic model must beat; it is not treatment-response validation."
        || baseline.limitations.len() != 5
        || baseline
            .limitations
            .iter()
            .any(|limitation| limitation.trim().is_empty())
    {
        return Err(invalid(
            "TCGA-GBM aggregate disagrees with the frozen intended-use contract",
        ));
    }
    Ok(())
}

fn validate_prediction(
    prediction: &Prediction,
) -> Result<(), CancerTcgaGbmTargetContextBuildError> {
    CancerMolecularTarget {
        gene_symbol: prediction.gene.clone(),
    }
    .validate()?;
    if prediction.calibration_prevalence_parts_per_million > 1_000_000
        || prediction.held_out_prevalence_parts_per_million > 1_000_000
        || prediction.absolute_error_parts_per_million
            != prediction
                .calibration_prevalence_parts_per_million
                .abs_diff(prediction.held_out_prevalence_parts_per_million)
    {
        return Err(invalid(
            "TCGA-GBM target prevalence is internally inconsistent",
        ));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> CancerTcgaGbmTargetContextBuildError {
    CancerTcgaGbmTargetContextBuildError::InvalidArtifact(message.into())
}

#[derive(Debug, Error)]
pub enum CancerTcgaGbmTargetContextBuildError {
    #[error("decode TCGA-GBM aggregate: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid TCGA-GBM target-context artifact: {0}")]
    InvalidArtifact(String),
    #[error(transparent)]
    Contract(#[from] world_domain::CancerResearchContractError),
    #[error(transparent)]
    Qualification(#[from] world_domain::CancerTcgaGbmTargetContextError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{
        CancerResearchArtifactKind, CancerResearchClaim, CancerResearchInferenceTier,
        CancerResearchProfile, CancerResearchStage, CancerResearchTarget, CancerResearchTask,
        CancerResearchTurnSelection, EntityId, SimTick, WorldSeed,
    };

    const BASELINE: &[u8] =
        include_bytes!("../../../data/cancer-research/tcga-gbm-dr46-patient-baseline-v1.json");

    #[test]
    fn frozen_baseline_is_exact_and_patient_disjoint() {
        let qualifier =
            CancerTcgaGbmTargetContextQualifier::new(BASELINE).expect("frozen aggregate");
        assert_eq!(qualifier.predictions.len(), 25);
        let egfr = qualifier.lookup(&CancerMolecularTarget {
            gene_symbol: "EGFR".to_owned(),
        });
        assert_eq!(egfr.status, CancerTcgaGbmTargetContextStatus::Evaluated);
        assert_eq!(egfr.calibration_prevalence_parts_per_million, Some(194_719));
        assert_eq!(egfr.held_out_prevalence_parts_per_million, Some(197_183));
        assert_eq!(egfr.absolute_error_parts_per_million, Some(2_464));
    }

    #[test]
    fn outside_calibration_feature_set_never_becomes_absence() {
        let qualifier =
            CancerTcgaGbmTargetContextQualifier::new(BASELINE).expect("frozen aggregate");
        let observation = qualifier.lookup(&CancerMolecularTarget {
            gene_symbol: "VEGFA".to_owned(),
        });
        assert_eq!(
            observation.status,
            CancerTcgaGbmTargetContextStatus::OutsideCalibrationFeatureSet
        );
        assert_eq!(observation.calibration_prevalence_parts_per_million, None);
        assert_eq!(observation.held_out_prevalence_parts_per_million, None);
    }

    #[test]
    fn changed_aggregate_bytes_fail_closed() {
        let mut changed = BASELINE.to_vec();
        changed.push(b'\n');
        assert!(CancerTcgaGbmTargetContextQualifier::new(&changed).is_err());
    }

    #[test]
    fn exact_targets_receive_context_without_turning_uncovered_targets_into_zeroes() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(71));
        let resident_id = EntityId::deterministic(world_id, b"tcga-target-context-test");
        let selection = CancerResearchTurnSelection::new(
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
            Vec::new(),
            None,
            2_048,
        )
        .expect("selection");
        let contribution = CancerResearchContribution::new_with_structured_evidence_targets(
            &selection,
            CancerResearchArtifactKind::Hypothesis,
            "Exact target context",
            "EGFR and VEGFA are explicit subjects of a falsifiable bounded hypothesis.",
            vec![CancerResearchClaim {
                statement: "The two targets can be distinguished experimentally.".to_owned(),
                testable_prediction: "A preregistered perturbation produces different readouts."
                    .to_owned(),
                falsification_test: "Equivalent readouts reject the proposed distinction."
                    .to_owned(),
                citation_hashes: Vec::new(),
            }],
            vec![
                CancerMolecularTarget {
                    gene_symbol: "EGFR".to_owned(),
                },
                CancerMolecularTarget {
                    gene_symbol: "VEGFA".to_owned(),
                },
            ],
            None,
            None,
        )
        .expect("contribution");
        let candidate = CancerTcgaGbmTargetContextCandidate {
            world_id,
            request_id: contribution.request_id,
            artifact_hash: contribution.canonical_hash().expect("artifact hash"),
            contribution,
        };
        let result = CancerTcgaGbmTargetContextQualifier::new(BASELINE)
            .expect("qualifier")
            .qualify(&candidate)
            .expect("qualification");
        assert_eq!(
            result.target_observations[0].status,
            CancerTcgaGbmTargetContextStatus::Evaluated
        );
        assert_eq!(
            result.target_observations[1].status,
            CancerTcgaGbmTargetContextStatus::OutsideCalibrationFeatureSet
        );
        assert!(result.validate_against(&candidate.contribution).is_ok());
    }
}
