use world_domain::{
    CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION, CANCER_VIRTUAL_LAB_METHOD_VERSION,
    CANCER_VIRTUAL_MECHANISTIC_READOUT_SCHEMA_VERSION, CancerResearchContractError,
    CancerResearchContribution, CancerVirtualCalibrationGrade, CancerVirtualCloneFractions,
    CancerVirtualEndpoint, CancerVirtualExperimentInterpretation, CancerVirtualExperimentResult,
    CancerVirtualInterventionModality, CancerVirtualLabFidelity, CancerVirtualMechanismTarget,
    CancerVirtualMechanisticReadout, CancerVirtualPkReadout, CancerVirtualSubjectModel, Digest,
    WorldId,
};

const PARTS_PER_MILLION: u32 = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancerVirtualExperimentCandidate {
    pub world_id: WorldId,
    pub request_id: uuid::Uuid,
    pub ordinal: u32,
    pub artifact_hash: Digest,
    pub contribution: CancerResearchContribution,
}

/// Executes the closed plan in a deterministic structural multiscale screen.
/// It models delivery, coarse PK/BBB exposure where applicable, differential
/// clone survival, and resistance selection without pretending those structural
/// parameters are calibrated biological or clinical evidence.
pub fn execute_cancer_virtual_experiment(
    candidate: &CancerVirtualExperimentCandidate,
) -> Result<CancerVirtualExperimentResult, CancerResearchContractError> {
    if candidate.request_id != candidate.contribution.request_id
        || candidate.artifact_hash != candidate.contribution.canonical_hash()?
    {
        return Err(CancerResearchContractError::InvalidVirtualExperimentResult);
    }
    let plan = candidate
        .contribution
        .virtual_experiment_plan
        .as_ref()
        .ok_or(CancerResearchContractError::InvalidVirtualExperimentPlan)?;
    plan.validate()?;
    let plan_hash = Digest::canonical(plan)?;
    let noise_hash = Digest::canonical(&(
        "a-tiny-civilization:virtual-lab:v2",
        candidate.world_id,
        candidate.request_id,
        candidate.artifact_hash,
        plan_hash,
    ))?;
    let noise_raw = u16::from_be_bytes([noise_hash.as_bytes()[0], noise_hash.as_bytes()[1]]);
    let noise = i64::from(noise_raw % 40_001) - 20_000;

    let baseline_clones = baseline_clone_fractions(plan.subject_model, noise_hash.as_bytes());
    let (delivered_exposure, pharmacokinetics) = delivered_exposure(plan);
    let target_engagement = multiply_parts_per_million(&[
        delivered_exposure,
        modality_factor(plan.intervention_modality),
        target_endpoint_affinity(plan.primary_target, plan.primary_endpoint),
    ]);
    let (post_exposure_clones, viable_tumor_fraction) = post_exposure_clone_fractions(
        &baseline_clones,
        target_engagement,
        plan.primary_target,
        plan.secondary_target,
        plan.intervention_modality,
    );
    let resistant_selection = i32::try_from(post_exposure_clones.resistant_parts_per_million)
        .unwrap_or(i32::MAX)
        - i32::try_from(baseline_clones.resistant_parts_per_million).unwrap_or(i32::MAX);
    let mechanistic_readout = CancerVirtualMechanisticReadout {
        schema_version: CANCER_VIRTUAL_MECHANISTIC_READOUT_SCHEMA_VERSION,
        fidelity: CancerVirtualLabFidelity::StructuralMultiscaleScreen,
        calibration_grade: CancerVirtualCalibrationGrade::StructuralUncalibrated,
        baseline_clones: baseline_clones.clone(),
        post_exposure_clones: post_exposure_clones.clone(),
        pharmacokinetics: pharmacokinetics.clone(),
        delivered_exposure_parts_per_million: delivered_exposure,
        target_engagement_parts_per_million: target_engagement,
        resistant_selection_parts_per_million: resistant_selection,
    };

    let (control, intervention) = match plan.primary_endpoint {
        CancerVirtualEndpoint::DetectionSensitivity => {
            let signal = (250_000_i64 + i64::from(target_engagement) * 700 / 1_000 + noise)
                .clamp(0, 1_000_000);
            (0_u32, u32::try_from(signal).unwrap_or_default())
        }
        CancerVirtualEndpoint::OffTargetHealthyCellLoss => {
            let toxicity = toxicity_factor(plan.intervention_modality);
            let relevant_exposure = pharmacokinetics
                .as_ref()
                .map_or(delivered_exposure, |readout| {
                    readout.systemic_exposure_parts_per_million
                });
            let loss = (i64::from(toxicity) * i64::from(relevant_exposure) / 1_000_000 + noise / 2)
                .clamp(0, 1_000_000);
            (0_u32, u32::try_from(loss).unwrap_or_default())
        }
        CancerVirtualEndpoint::InvasiveCellFraction => fraction_endpoint(
            invasive_fraction(&baseline_clones),
            invasive_fraction(&post_exposure_clones),
            noise,
        ),
        CancerVirtualEndpoint::HypoxicCellFraction => fraction_endpoint(
            hypoxic_fraction(&baseline_clones),
            hypoxic_fraction(&post_exposure_clones),
            noise,
        ),
        _ => {
            let intervention = (i64::from(viable_tumor_fraction) + noise).clamp(0, 1_000_000);
            (
                PARTS_PER_MILLION,
                u32::try_from(intervention).unwrap_or_default(),
            )
        }
    };
    let change = i32::try_from(intervention).unwrap_or(i32::MAX)
        - i32::try_from(control).unwrap_or(i32::MAX);
    let sampling_uncertainty = 500_000_i32
        / i32::try_from(integer_sqrt(u32::from(plan.cohort_size)).max(1)).unwrap_or(i32::MAX);
    let extrapolation_uncertainty = match plan.subject_model {
        CancerVirtualSubjectModel::CellCulture => 20_000,
        CancerVirtualSubjectModel::TumorOrganoid => 45_000,
        CancerVirtualSubjectModel::OrthotopicMouse => 80_000,
    };
    let pk_uncertainty = if pharmacokinetics.is_some() {
        75_000
    } else {
        0
    };
    let selection_uncertainty = resistant_selection.unsigned_abs().min(300_000) / 2;
    let uncertainty = 25_000_i32
        .saturating_add(sampling_uncertainty)
        .saturating_add(extrapolation_uncertainty)
        .saturating_add(pk_uncertainty)
        .saturating_add(i32::try_from(selection_uncertainty).unwrap_or(i32::MAX))
        .min(600_000);
    let low = change.saturating_sub(uncertainty);
    let high = change.saturating_add(uncertainty);
    let interpretation = interpret(plan.primary_endpoint, change, low, high);
    let result = CancerVirtualExperimentResult {
        schema_version: CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION,
        method_version: CANCER_VIRTUAL_LAB_METHOD_VERSION,
        experiment_id: CancerVirtualExperimentResult::deterministic_id(
            candidate.request_id,
            CANCER_VIRTUAL_LAB_METHOD_VERSION,
        ),
        world_id: candidate.world_id,
        request_id: candidate.request_id,
        artifact_hash: candidate.artifact_hash,
        plan_hash,
        subject_model: plan.subject_model,
        primary_endpoint: plan.primary_endpoint,
        cohort_size: plan.cohort_size,
        control_value_parts_per_million: control,
        intervention_value_parts_per_million: intervention,
        estimated_change_parts_per_million: change,
        uncertainty_low_parts_per_million: low,
        uncertainty_high_parts_per_million: high,
        interpretation,
        model_calibration: "structural_multiscale_projection_v2".to_owned(),
        mechanistic_readout: Some(mechanistic_readout),
        caveats: vec![
            "This is a deterministic computational projection, not wet-lab, animal, or clinical evidence."
                .to_owned(),
            "Clone, exposure, and response coefficients remain structural assumptions pending source-backed calibration."
                .to_owned(),
            "BBB exposure is dimensionless and is not specific to a real compound, patient, dose, or formulation."
                .to_owned(),
            "Only campaign survivors are eligible for later resource-capped tissue-scale refinement."
                .to_owned(),
        ],
    };
    result.validate_against(&candidate.contribution)?;
    Ok(result)
}

fn modality_factor(modality: CancerVirtualInterventionModality) -> u32 {
    match modality {
        CancerVirtualInterventionModality::MolecularInhibition => 620_000,
        CancerVirtualInterventionModality::Radiation => 760_000,
        CancerVirtualInterventionModality::Thermal => 580_000,
        CancerVirtualInterventionModality::ElectricField => 470_000,
        CancerVirtualInterventionModality::TargetedDelivery => 680_000,
        CancerVirtualInterventionModality::SurgicalResection => 820_000,
        CancerVirtualInterventionModality::DiagnosticSensing => 780_000,
    }
}

fn toxicity_factor(modality: CancerVirtualInterventionModality) -> u32 {
    match modality {
        CancerVirtualInterventionModality::MolecularInhibition => 170_000,
        CancerVirtualInterventionModality::Radiation => 320_000,
        CancerVirtualInterventionModality::Thermal => 260_000,
        CancerVirtualInterventionModality::ElectricField => 120_000,
        CancerVirtualInterventionModality::TargetedDelivery => 80_000,
        CancerVirtualInterventionModality::SurgicalResection => 360_000,
        CancerVirtualInterventionModality::DiagnosticSensing => 20_000,
    }
}

fn target_endpoint_affinity(
    target: CancerVirtualMechanismTarget,
    endpoint: CancerVirtualEndpoint,
) -> u32 {
    match (target, endpoint) {
        (
            CancerVirtualMechanismTarget::CellDivision,
            CancerVirtualEndpoint::ViableTumorFraction,
        )
        | (CancerVirtualMechanismTarget::DnaRepair, CancerVirtualEndpoint::ViableTumorFraction)
        | (
            CancerVirtualMechanismTarget::ApoptosisResistance,
            CancerVirtualEndpoint::ViableTumorFraction,
        )
        | (CancerVirtualMechanismTarget::Invasion, CancerVirtualEndpoint::InvasiveCellFraction)
        | (
            CancerVirtualMechanismTarget::HypoxiaAdaptation,
            CancerVirtualEndpoint::HypoxicCellFraction,
        )
        | (
            CancerVirtualMechanismTarget::Angiogenesis,
            CancerVirtualEndpoint::HypoxicCellFraction,
        )
        | (
            CancerVirtualMechanismTarget::ImmuneEvasion,
            CancerVirtualEndpoint::RelativeTumorBurden,
        ) => 1_000_000,
        (_, CancerVirtualEndpoint::DetectionSensitivity) => 900_000,
        (_, CancerVirtualEndpoint::OffTargetHealthyCellLoss) => 700_000,
        (_, CancerVirtualEndpoint::RelativeTumorBurden) => 850_000,
        _ => 620_000,
    }
}

fn multiply_parts_per_million(factors: &[u32]) -> u32 {
    let mut value = u64::from(PARTS_PER_MILLION);
    for factor in factors {
        value = value.saturating_mul(u64::from(*factor)) / u64::from(PARTS_PER_MILLION);
    }
    u32::try_from(value.min(u64::from(PARTS_PER_MILLION))).unwrap_or(PARTS_PER_MILLION)
}

fn baseline_clone_fractions(
    subject: CancerVirtualSubjectModel,
    entropy: &[u8; 32],
) -> CancerVirtualCloneFractions {
    let (base_sensitive, base_resistant) = match subject {
        CancerVirtualSubjectModel::CellCulture => (760_000_i64, 70_000_i64),
        CancerVirtualSubjectModel::TumorOrganoid => (650_000_i64, 120_000_i64),
        CancerVirtualSubjectModel::OrthotopicMouse => (560_000_i64, 180_000_i64),
    };
    let sensitive_jitter =
        i64::from(u32::from_be_bytes([0, entropy[2], entropy[3], entropy[6]]) % 80_001) - 40_000;
    let resistant_jitter =
        i64::from(u32::from_be_bytes([0, entropy[4], entropy[5], entropy[7]]) % 50_001) - 25_000;
    let sensitive = u32::try_from((base_sensitive + sensitive_jitter).clamp(450_000, 820_000))
        .unwrap_or(650_000);
    let resistant = u32::try_from((base_resistant + resistant_jitter).clamp(40_000, 240_000))
        .unwrap_or(120_000);
    CancerVirtualCloneFractions {
        treatment_sensitive_parts_per_million: sensitive,
        drug_tolerant_parts_per_million: PARTS_PER_MILLION - sensitive - resistant,
        resistant_parts_per_million: resistant,
    }
}

fn delivered_exposure(
    plan: &world_domain::CancerVirtualExperimentPlan,
) -> (u32, Option<CancerVirtualPkReadout>) {
    let duration_saturation = u32::try_from(
        (150_000_u64 + u64::from(plan.exposure_hours) * 850_000 / 168).min(1_000_000),
    )
    .unwrap_or(PARTS_PER_MILLION);
    let systemic_exposure =
        multiply_parts_per_million(&[plan.intensity_parts_per_million, duration_saturation]);
    let drug_like = matches!(
        plan.intervention_modality,
        CancerVirtualInterventionModality::MolecularInhibition
            | CancerVirtualInterventionModality::TargetedDelivery
    );
    if plan.subject_model == CancerVirtualSubjectModel::OrthotopicMouse && drug_like {
        let bbb_penetration = match plan.intervention_modality {
            CancerVirtualInterventionModality::TargetedDelivery => 420_000,
            CancerVirtualInterventionModality::MolecularInhibition => 160_000,
            _ => unreachable!("drug-like modality was checked above"),
        };
        let brain_exposure = multiply_parts_per_million(&[systemic_exposure, bbb_penetration]);
        return (
            brain_exposure,
            Some(CancerVirtualPkReadout {
                systemic_exposure_parts_per_million: systemic_exposure,
                bbb_penetration_parts_per_million: bbb_penetration,
                unbound_brain_exposure_parts_per_million: brain_exposure,
                effective_exposure_hours: plan.exposure_hours,
            }),
        );
    }
    let delivery_fraction = match plan.subject_model {
        CancerVirtualSubjectModel::CellCulture => 1_000_000,
        CancerVirtualSubjectModel::TumorOrganoid => 720_000,
        CancerVirtualSubjectModel::OrthotopicMouse => 900_000,
    };
    (
        multiply_parts_per_million(&[systemic_exposure, delivery_fraction]),
        None,
    )
}

fn post_exposure_clone_fractions(
    baseline: &CancerVirtualCloneFractions,
    target_engagement: u32,
    primary_target: CancerVirtualMechanismTarget,
    secondary_target: Option<CancerVirtualMechanismTarget>,
    modality: CancerVirtualInterventionModality,
) -> (CancerVirtualCloneFractions, u32) {
    if modality == CancerVirtualInterventionModality::DiagnosticSensing {
        return (baseline.clone(), PARTS_PER_MILLION);
    }
    let uniform_effect = matches!(
        modality,
        CancerVirtualInterventionModality::SurgicalResection
            | CancerVirtualInterventionModality::Thermal
    );
    let secondary_resistance_pressure = if secondary_target.is_some() {
        120_000
    } else {
        0
    };
    let (sensitive_susceptibility, tolerant_susceptibility, resistant_susceptibility) =
        if uniform_effect {
            (720_000, 720_000, 720_000)
        } else {
            let target_resistance_pressure = match primary_target {
                CancerVirtualMechanismTarget::DnaRepair
                | CancerVirtualMechanismTarget::ApoptosisResistance
                | CancerVirtualMechanismTarget::Invasion => 180_000,
                _ => 0,
            };
            (
                850_000,
                450_000,
                140_000_u32
                    .saturating_add(target_resistance_pressure)
                    .saturating_add(secondary_resistance_pressure)
                    .min(700_000),
            )
        };
    let sensitive = surviving_compartment(
        baseline.treatment_sensitive_parts_per_million,
        target_engagement,
        sensitive_susceptibility,
    );
    let tolerant = surviving_compartment(
        baseline.drug_tolerant_parts_per_million,
        target_engagement,
        tolerant_susceptibility,
    );
    let resistant = surviving_compartment(
        baseline.resistant_parts_per_million,
        target_engagement,
        resistant_susceptibility,
    );
    let viable_total = sensitive
        .saturating_add(tolerant)
        .saturating_add(resistant)
        .max(1);
    let normalized_sensitive = u32::try_from(
        u64::from(sensitive) * u64::from(PARTS_PER_MILLION) / u64::from(viable_total),
    )
    .unwrap_or_default();
    let normalized_tolerant =
        u32::try_from(u64::from(tolerant) * u64::from(PARTS_PER_MILLION) / u64::from(viable_total))
            .unwrap_or_default();
    let normalized_resistant = PARTS_PER_MILLION
        .saturating_sub(normalized_sensitive)
        .saturating_sub(normalized_tolerant);
    (
        CancerVirtualCloneFractions {
            treatment_sensitive_parts_per_million: normalized_sensitive,
            drug_tolerant_parts_per_million: normalized_tolerant,
            resistant_parts_per_million: normalized_resistant,
        },
        viable_total.min(PARTS_PER_MILLION),
    )
}

fn surviving_compartment(population: u32, pressure: u32, susceptibility: u32) -> u32 {
    let removed_fraction = multiply_parts_per_million(&[pressure, susceptibility]);
    u32::try_from(
        u64::from(population) * u64::from(PARTS_PER_MILLION - removed_fraction)
            / u64::from(PARTS_PER_MILLION),
    )
    .unwrap_or_default()
}

fn invasive_fraction(clones: &CancerVirtualCloneFractions) -> u32 {
    clones
        .resistant_parts_per_million
        .saturating_add(clones.drug_tolerant_parts_per_million / 3)
        .min(PARTS_PER_MILLION)
}

fn hypoxic_fraction(clones: &CancerVirtualCloneFractions) -> u32 {
    clones
        .drug_tolerant_parts_per_million
        .saturating_add(clones.resistant_parts_per_million / 2)
        .min(PARTS_PER_MILLION)
}

fn fraction_endpoint(control: u32, intervention: u32, noise: i64) -> (u32, u32) {
    let intervention = (i64::from(intervention) + noise / 4).clamp(0, 1_000_000);
    (control, u32::try_from(intervention).unwrap_or_default())
}

fn interpret(
    endpoint: CancerVirtualEndpoint,
    change: i32,
    low: i32,
    high: i32,
) -> CancerVirtualExperimentInterpretation {
    if low <= 0 && high >= 0 {
        return CancerVirtualExperimentInterpretation::ModelInconclusive;
    }
    match endpoint {
        CancerVirtualEndpoint::DetectionSensitivity if change >= 500_000 => {
            CancerVirtualExperimentInterpretation::ModelSupportsPrediction
        }
        CancerVirtualEndpoint::OffTargetHealthyCellLoss if change >= 200_000 => {
            CancerVirtualExperimentInterpretation::ModelShowsConcerningTradeoff
        }
        CancerVirtualEndpoint::OffTargetHealthyCellLoss => {
            CancerVirtualExperimentInterpretation::ModelShowsNoMaterialEffect
        }
        _ if change <= -150_000 => CancerVirtualExperimentInterpretation::ModelSupportsPrediction,
        _ if change.abs() < 50_000 => {
            CancerVirtualExperimentInterpretation::ModelShowsNoMaterialEffect
        }
        _ => CancerVirtualExperimentInterpretation::ModelInconclusive,
    }
}

fn integer_sqrt(value: u32) -> u32 {
    if value < 2 {
        return value;
    }
    let mut estimate = value;
    let mut next = (estimate + value / estimate) / 2;
    while next < estimate {
        estimate = next;
        next = (estimate + value / estimate) / 2;
    }
    estimate
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{
        CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION, CancerResearchArtifactKind,
        CancerResearchClaim, CancerResearchInferenceTier, CancerResearchProfile,
        CancerResearchStage, CancerResearchTarget, CancerResearchTask, CancerResearchTurnSelection,
        CancerVirtualExperimentPlan, EntityId, SimTick, WorldSeed,
    };

    fn candidate(endpoint: CancerVirtualEndpoint) -> CancerVirtualExperimentCandidate {
        let world_id = WorldId::from_uuid(uuid::Uuid::from_u128(901));
        let resident_id = EntityId::deterministic(world_id, b"virtual-lab-test");
        let selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(1),
            SimTick::new(2),
            1,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::ProposeDiscriminatingExperiment,
            CancerResearchInferenceTier::Exploration,
            CancerResearchProfile::seeded(WorldSeed::new(1), resident_id).expect("profile"),
            Vec::new(),
            None,
            512,
        )
        .expect("selection");
        let contribution = CancerResearchContribution::new_with_virtual_experiment(
            &selection,
            CancerResearchArtifactKind::ExperimentProposal,
            "A closed virtual experiment",
            "The bounded experiment compares one intervention with its control.",
            vec![CancerResearchClaim {
                statement: "The intervention changes the selected endpoint.".to_owned(),
                testable_prediction: "The intervention cohort differs from control.".to_owned(),
                falsification_test: "The bounded interval crosses zero.".to_owned(),
                citation_hashes: Vec::new(),
            }],
            Some(CancerVirtualExperimentPlan {
                schema_version: CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION,
                subject_model: CancerVirtualSubjectModel::TumorOrganoid,
                intervention_modality: CancerVirtualInterventionModality::MolecularInhibition,
                primary_target: CancerVirtualMechanismTarget::CellDivision,
                secondary_target: None,
                primary_endpoint: endpoint,
                intensity_parts_per_million: 800_000,
                exposure_hours: 168,
                cohort_size: 128,
            }),
        )
        .expect("contribution");
        CancerVirtualExperimentCandidate {
            world_id,
            request_id: contribution.request_id,
            ordinal: 1,
            artifact_hash: contribution.canonical_hash().expect("hash"),
            contribution,
        }
    }

    #[test]
    fn execution_is_repeatable_and_bound_to_the_plan() {
        let candidate = candidate(CancerVirtualEndpoint::ViableTumorFraction);
        let first = execute_cancer_virtual_experiment(&candidate).expect("first result");
        let second = execute_cancer_virtual_experiment(&candidate).expect("second result");
        assert_eq!(first, second);
        assert!(first.validate_against(&candidate.contribution).is_ok());
        assert_eq!(
            first.estimated_change_parts_per_million,
            i32::try_from(first.intervention_value_parts_per_million).expect("intervention")
                - i32::try_from(first.control_value_parts_per_million).expect("control")
        );
        let readout = first
            .mechanistic_readout
            .as_ref()
            .expect("current lab emits a mechanistic readout");
        assert_eq!(
            readout.fidelity,
            CancerVirtualLabFidelity::StructuralMultiscaleScreen
        );
        assert!(readout.pharmacokinetics.is_none());
        assert!(readout.resistant_selection_parts_per_million > 0);
    }

    #[test]
    fn uncertainty_shrinks_with_larger_cohorts() {
        let small = candidate(CancerVirtualEndpoint::ViableTumorFraction);
        let mut large = small.clone();
        large
            .contribution
            .virtual_experiment_plan
            .as_mut()
            .expect("plan")
            .cohort_size = 2_048;
        large.artifact_hash = large.contribution.canonical_hash().expect("large hash");
        let small_result = execute_cancer_virtual_experiment(&small).expect("small result");
        let large_result = execute_cancer_virtual_experiment(&large).expect("large result");
        let small_width = small_result.uncertainty_high_parts_per_million
            - small_result.uncertainty_low_parts_per_million;
        let large_width = large_result.uncertainty_high_parts_per_million
            - large_result.uncertainty_low_parts_per_million;
        assert!(large_width < small_width);
    }

    #[test]
    fn orthotopic_drug_screen_exposes_bounded_bbb_delivery() {
        let mut candidate = candidate(CancerVirtualEndpoint::ViableTumorFraction);
        candidate
            .contribution
            .virtual_experiment_plan
            .as_mut()
            .expect("plan")
            .subject_model = CancerVirtualSubjectModel::OrthotopicMouse;
        candidate.artifact_hash = candidate
            .contribution
            .canonical_hash()
            .expect("orthotopic hash");
        let result = execute_cancer_virtual_experiment(&candidate).expect("result");
        let readout = result.mechanistic_readout.as_ref().expect("readout");
        let pharmacokinetics = readout.pharmacokinetics.as_ref().expect("pk readout");
        assert!(
            pharmacokinetics.unbound_brain_exposure_parts_per_million
                < pharmacokinetics.systemic_exposure_parts_per_million
        );
        assert_eq!(
            readout.delivered_exposure_parts_per_million,
            pharmacokinetics.unbound_brain_exposure_parts_per_million
        );
        assert!(result.validate_against(&candidate.contribution).is_ok());
    }

    #[test]
    fn non_drug_orthotopic_screen_does_not_invent_pharmacokinetics() {
        let mut candidate = candidate(CancerVirtualEndpoint::ViableTumorFraction);
        let plan = candidate
            .contribution
            .virtual_experiment_plan
            .as_mut()
            .expect("plan");
        plan.subject_model = CancerVirtualSubjectModel::OrthotopicMouse;
        plan.intervention_modality = CancerVirtualInterventionModality::Radiation;
        candidate.artifact_hash = candidate
            .contribution
            .canonical_hash()
            .expect("radiation hash");
        let result = execute_cancer_virtual_experiment(&candidate).expect("result");
        assert!(
            result
                .mechanistic_readout
                .expect("readout")
                .pharmacokinetics
                .is_none()
        );
    }

    #[test]
    fn historical_method_one_results_remain_readable() {
        let candidate = candidate(CancerVirtualEndpoint::ViableTumorFraction);
        let mut legacy = execute_cancer_virtual_experiment(&candidate).expect("current result");
        legacy.schema_version =
            world_domain::LEGACY_CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION;
        legacy.method_version = 1;
        legacy.experiment_id = CancerVirtualExperimentResult::deterministic_id(
            legacy.request_id,
            legacy.method_version,
        );
        legacy.model_calibration = "uncalibrated_mechanistic_projection_v1".to_owned();
        legacy.mechanistic_readout = None;
        legacy.caveats.truncate(2);
        assert!(legacy.validate_against(&candidate.contribution).is_ok());
    }
}
