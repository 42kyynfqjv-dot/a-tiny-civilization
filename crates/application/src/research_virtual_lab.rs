use world_domain::{
    CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION, CANCER_VIRTUAL_LAB_METHOD_VERSION,
    CancerResearchContractError, CancerResearchContribution, CancerVirtualEndpoint,
    CancerVirtualExperimentInterpretation, CancerVirtualExperimentResult,
    CancerVirtualInterventionModality, CancerVirtualMechanismTarget, CancerVirtualSubjectModel,
    Digest, WorldId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancerVirtualExperimentCandidate {
    pub world_id: WorldId,
    pub request_id: uuid::Uuid,
    pub ordinal: u32,
    pub artifact_hash: Digest,
    pub contribution: CancerResearchContribution,
}

/// Executes the closed plan in a deterministic, intentionally uncalibrated
/// mechanistic model. This makes proposed tests runnable and comparable while
/// preserving the hard boundary between a simulation result and wet-lab evidence.
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
        "a-tiny-civilization:virtual-lab:v1",
        candidate.world_id,
        candidate.request_id,
        candidate.artifact_hash,
        plan_hash,
    ))?;
    let noise_raw = u16::from_be_bytes([noise_hash.as_bytes()[0], noise_hash.as_bytes()[1]]);
    let noise = i64::from(noise_raw % 40_001) - 20_000;

    let subject = subject_factor(plan.subject_model);
    let modality = modality_factor(plan.intervention_modality);
    let affinity = target_endpoint_affinity(plan.primary_target, plan.primary_endpoint);
    let dose = u64::from(plan.intensity_parts_per_million.clamp(25_000, 1_000_000)) / 1_000;
    let exposure = 200_u64
        .saturating_add(u64::from(plan.exposure_hours).saturating_mul(800) / 168)
        .min(1_000);
    let mechanistic_effect = multiply_per_mille(&[subject, modality, affinity, dose, exposure]);

    let (control, intervention) = match plan.primary_endpoint {
        CancerVirtualEndpoint::DetectionSensitivity => {
            let signal =
                (350_000_i64 + i64::from(mechanistic_effect) * 600 + noise).clamp(0, 1_000_000);
            (0_u32, u32::try_from(signal).unwrap_or_default())
        }
        CancerVirtualEndpoint::OffTargetHealthyCellLoss => {
            let toxicity = toxicity_factor(plan.intervention_modality);
            let dose = i64::try_from(dose).unwrap_or(1_000);
            let loss = (i64::from(toxicity) * dose / 1_000 + noise / 2).clamp(0, 1_000_000);
            (0_u32, u32::try_from(loss).unwrap_or_default())
        }
        _ => {
            let reduction = (i64::from(mechanistic_effect) * 800 + noise).clamp(0, 900_000);
            (
                1_000_000_u32,
                u32::try_from(1_000_000_i64 - reduction).unwrap_or_default(),
            )
        }
    };
    let change = i32::try_from(intervention).unwrap_or(i32::MAX)
        - i32::try_from(control).unwrap_or(i32::MAX);
    let uncertainty = 25_000_i32.saturating_add(
        500_000_i32
            / i32::try_from(integer_sqrt(u32::from(plan.cohort_size)).max(1)).unwrap_or(i32::MAX),
    );
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
        model_calibration: "uncalibrated_mechanistic_projection_v1".to_owned(),
        caveats: vec![
            "This is a deterministic computational projection, not wet-lab, animal, or clinical evidence."
                .to_owned(),
            "Effect coefficients are deliberately bounded placeholders pending source-backed calibration and validation."
                .to_owned(),
        ],
    };
    result.validate_against(&candidate.contribution)?;
    Ok(result)
}

fn subject_factor(subject: CancerVirtualSubjectModel) -> u64 {
    match subject {
        CancerVirtualSubjectModel::CellCulture => 720,
        CancerVirtualSubjectModel::TumorOrganoid => 850,
        CancerVirtualSubjectModel::OrthotopicMouse => 930,
    }
}

fn modality_factor(modality: CancerVirtualInterventionModality) -> u64 {
    match modality {
        CancerVirtualInterventionModality::MolecularInhibition => 500,
        CancerVirtualInterventionModality::Radiation => 620,
        CancerVirtualInterventionModality::Thermal => 460,
        CancerVirtualInterventionModality::ElectricField => 390,
        CancerVirtualInterventionModality::TargetedDelivery => 540,
        CancerVirtualInterventionModality::SurgicalResection => 760,
        CancerVirtualInterventionModality::DiagnosticSensing => 700,
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
) -> u64 {
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
        ) => 1_000,
        (_, CancerVirtualEndpoint::DetectionSensitivity) => 900,
        (_, CancerVirtualEndpoint::OffTargetHealthyCellLoss) => 700,
        (_, CancerVirtualEndpoint::RelativeTumorBurden) => 850,
        _ => 620,
    }
}

fn multiply_per_mille(factors: &[u64]) -> u32 {
    let mut value = 1_000_u64;
    for factor in factors {
        value = value.saturating_mul(*factor) / 1_000;
    }
    u32::try_from(value.min(1_000)).unwrap_or(1_000)
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
}
