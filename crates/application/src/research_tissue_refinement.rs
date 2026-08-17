use std::collections::BTreeSet;

use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CANCER_TISSUE_REFINEMENT_CAVEATS, CANCER_TISSUE_REFINEMENT_MAX_CELLS,
    CANCER_TISSUE_REFINEMENT_MAX_STEPS, CANCER_TISSUE_REFINEMENT_METHOD_VERSION,
    CANCER_TISSUE_REFINEMENT_PROTOCOL_SCHEMA_VERSION,
    CANCER_TISSUE_REFINEMENT_RESULT_SCHEMA_VERSION,
    CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION, CANCER_VIRTUAL_LAB_METHOD_VERSION,
    CancerResearchContractError, CancerResearchStage, CancerTissueRefinementContractError,
    CancerTissueRefinementFieldModel, CancerTissueRefinementProtocol, CancerTissueRefinementResult,
    CancerTissueRefinementScenario, CancerTissueRefinementScenarioSummary,
    CancerTissueRefinementSnapshot, CancerTissueRefinementTermination,
    CancerTissueRefinementUncertaintyEnvelope, CancerVirtualCloneFractions,
    CancerVirtualExperimentInterpretation, CancerVirtualExperimentResult,
    CancerVirtualInterventionModality, CancerVirtualSubjectModel, Digest,
};

use crate::{
    CancerResearchCampaignDirective, CancerResearchCampaignTestAssessment,
    CancerVirtualExperimentCandidate, cancer_research_campaign_test_assessment,
};

const PARTS_PER_MILLION: u32 = 1_000_000;
const MAX_CAMPAIGN_TESTS: usize = 5;
const REQUIRED_SUPPORTING_TESTS: usize = 3;
const OXYGEN_HYPOXIA_THRESHOLD: u32 = 250_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancerTissueRefinementCampaignExperiment {
    pub frozen_root_artifact_hash: Digest,
    pub candidate: CancerVirtualExperimentCandidate,
    pub result: CancerVirtualExperimentResult,
}

/// Provenance of the successful synthesis that closed the immutable campaign.
/// The PostgreSQL adapter derives these hashes from validated durable request
/// and result bytes; callers cannot substitute prose for the computed outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancerTissueRefinementSurvivalEvidence {
    pub synthesis_request_id: Uuid,
    pub synthesis_request_hash: Digest,
    pub synthesis_result_hash: Digest,
    pub campaign_id: Uuid,
    pub root_artifact_hash: Digest,
    pub supporting_tests: u8,
    pub falsifying_tests: u8,
    pub inconclusive_tests: u8,
}

/// Complete, closed campaign evidence supplied to the refinement selector.
/// The caller must include every campaign experiment; omitting an adverse test
/// is invalid persistence behavior and the durable selector must enforce that
/// completeness when this pure engine is wired to a store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancerTissueRefinementCandidate {
    pub campaign_id: Uuid,
    pub root: CancerVirtualExperimentCandidate,
    pub root_result: CancerVirtualExperimentResult,
    pub campaign_experiments: Vec<CancerTissueRefinementCampaignExperiment>,
    pub survival_evidence: CancerTissueRefinementSurvivalEvidence,
}

impl CancerTissueRefinementCandidate {
    pub fn validate_survivor(&self) -> Result<(), CancerTissueRefinementError> {
        validate_current_result(&self.root_result, &self.root)?;
        if self.campaign_id != CancerResearchCampaignDirective::campaign_id(self.root.request_id)
            || self.root_result.interpretation
                != CancerVirtualExperimentInterpretation::ModelSupportsPrediction
            || !(REQUIRED_SUPPORTING_TESTS..=MAX_CAMPAIGN_TESTS)
                .contains(&self.campaign_experiments.len())
        {
            return Err(CancerTissueRefinementError::IneligibleCampaign);
        }

        let root_hash = self.root.contribution.canonical_hash()?;
        let root_plan_hash = Digest::canonical(
            self.root
                .contribution
                .virtual_experiment_plan
                .as_ref()
                .ok_or(CancerTissueRefinementError::IneligibleCampaign)?,
        )?;
        let mut request_ids = BTreeSet::from([self.root.request_id]);
        let mut artifact_hashes = BTreeSet::from([root_hash]);
        let mut plan_hashes = BTreeSet::from([root_plan_hash]);
        let mut result_hashes = BTreeSet::new();
        let mut supporting = 0_usize;
        let mut inconclusive = 0_usize;

        for experiment in &self.campaign_experiments {
            validate_current_result(&experiment.result, &experiment.candidate)?;
            let plan_hash = Digest::canonical(
                experiment
                    .candidate
                    .contribution
                    .virtual_experiment_plan
                    .as_ref()
                    .ok_or(CancerTissueRefinementError::IneligibleCampaign)?,
            )?;
            let result_hash = experiment
                .result
                .canonical_hash(&experiment.candidate.contribution)?;
            if experiment.frozen_root_artifact_hash != root_hash
                || experiment.candidate.world_id != self.root.world_id
                || experiment.candidate.contribution.stage
                    != CancerResearchStage::IndependentReplication
                || !request_ids.insert(experiment.candidate.request_id)
                || !artifact_hashes.insert(experiment.candidate.artifact_hash)
                || !plan_hashes.insert(plan_hash)
                || !result_hashes.insert(result_hash)
            {
                return Err(CancerTissueRefinementError::IneligibleCampaign);
            }
            match cancer_research_campaign_test_assessment(&experiment.result) {
                CancerResearchCampaignTestAssessment::Supports => supporting += 1,
                CancerResearchCampaignTestAssessment::Inconclusive => inconclusive += 1,
                CancerResearchCampaignTestAssessment::Falsifies => {
                    return Err(CancerTissueRefinementError::IneligibleCampaign);
                }
            }
        }
        if supporting < REQUIRED_SUPPORTING_TESTS {
            return Err(CancerTissueRefinementError::IneligibleCampaign);
        }
        if self.survival_evidence.synthesis_request_id.is_nil()
            || self.survival_evidence.synthesis_request_hash == Digest::ZERO
            || self.survival_evidence.synthesis_result_hash == Digest::ZERO
            || self.survival_evidence.campaign_id != self.campaign_id
            || self.survival_evidence.root_artifact_hash != root_hash
            || usize::from(self.survival_evidence.supporting_tests) != supporting
            || self.survival_evidence.falsifying_tests != 0
            || usize::from(self.survival_evidence.inconclusive_tests) != inconclusive
            || usize::from(self.survival_evidence.supporting_tests)
                + usize::from(self.survival_evidence.inconclusive_tests)
                != self.campaign_experiments.len()
        {
            return Err(CancerTissueRefinementError::IneligibleCampaign);
        }
        Ok(())
    }
}

fn validate_current_result(
    result: &CancerVirtualExperimentResult,
    candidate: &CancerVirtualExperimentCandidate,
) -> Result<(), CancerTissueRefinementError> {
    if candidate.request_id != candidate.contribution.request_id
        || candidate.artifact_hash != candidate.contribution.canonical_hash()?
        || result.world_id != candidate.world_id
        || result.schema_version != CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION
        || result.method_version != CANCER_VIRTUAL_LAB_METHOD_VERSION
        || result.mechanistic_readout.is_none()
    {
        return Err(CancerTissueRefinementError::IneligibleCampaign);
    }
    result.validate_against(&candidate.contribution)?;
    Ok(())
}

/// Derives the only accepted method-1 protocol from immutable method-2 campaign
/// evidence. There is no free numeric input for a model or operator to tune.
pub fn prepare_cancer_tissue_refinement_protocol(
    candidate: &CancerTissueRefinementCandidate,
) -> Result<CancerTissueRefinementProtocol, CancerTissueRefinementError> {
    candidate.validate_survivor()?;
    let plan = candidate
        .root
        .contribution
        .virtual_experiment_plan
        .as_ref()
        .ok_or(CancerTissueRefinementError::IneligibleCampaign)?;
    let edge = match plan.subject_model {
        CancerVirtualSubjectModel::CellCulture => 16,
        CancerVirtualSubjectModel::TumorOrganoid => 24,
        CancerVirtualSubjectModel::OrthotopicMouse => 32,
    };
    let modeled_exposure_hours = plan.exposure_hours.min(CANCER_TISSUE_REFINEMENT_MAX_STEPS);
    let maximum_steps = modeled_exposure_hours;
    let snapshot_every_steps = maximum_steps.div_ceil(16).max(1);
    let mut campaign_result_hashes = candidate
        .campaign_experiments
        .iter()
        .map(|experiment| {
            experiment
                .result
                .canonical_hash(&experiment.candidate.contribution)
        })
        .collect::<Result<Vec<_>, _>>()?;
    campaign_result_hashes.sort_unstable();
    let protocol = CancerTissueRefinementProtocol {
        schema_version: CANCER_TISSUE_REFINEMENT_PROTOCOL_SCHEMA_VERSION,
        method_version: CANCER_TISSUE_REFINEMENT_METHOD_VERSION,
        refinement_id: CancerTissueRefinementProtocol::deterministic_id(
            candidate.campaign_id,
            CANCER_TISSUE_REFINEMENT_METHOD_VERSION,
        ),
        world_id: candidate.root.world_id,
        campaign_id: candidate.campaign_id,
        root_request_id: candidate.root.request_id,
        root_artifact_hash: candidate.root.artifact_hash,
        root_plan_hash: Digest::canonical(plan)?,
        root_result_hash: candidate
            .root_result
            .canonical_hash(&candidate.root.contribution)?,
        survival_synthesis_request_id: candidate.survival_evidence.synthesis_request_id,
        survival_synthesis_request_hash: candidate.survival_evidence.synthesis_request_hash,
        survival_synthesis_result_hash: candidate.survival_evidence.synthesis_result_hash,
        campaign_result_hashes,
        field_model: field_model(plan.intervention_modality),
        lattice_width: edge,
        lattice_height: edge,
        initial_cell_count: u32::from(edge) * u32::from(edge) * 2,
        cell_capacity: CANCER_TISSUE_REFINEMENT_MAX_CELLS,
        maximum_steps,
        snapshot_every_steps,
        requested_exposure_hours: plan.exposure_hours,
        modeled_exposure_hours,
        horizon_truncated: modeled_exposure_hours < plan.exposure_hours,
        scenarios: CancerTissueRefinementScenario::ALL.to_vec(),
    };
    protocol.validate()?;
    Ok(protocol)
}

const fn field_model(
    modality: CancerVirtualInterventionModality,
) -> CancerTissueRefinementFieldModel {
    match modality {
        CancerVirtualInterventionModality::MolecularInhibition
        | CancerVirtualInterventionModality::Thermal
        | CancerVirtualInterventionModality::TargetedDelivery => {
            CancerTissueRefinementFieldModel::DiffusiveExposure
        }
        CancerVirtualInterventionModality::Radiation
        | CancerVirtualInterventionModality::ElectricField
        | CancerVirtualInterventionModality::DiagnosticSensing => {
            CancerTissueRefinementFieldModel::DeviceField
        }
        CancerVirtualInterventionModality::SurgicalResection => {
            CancerTissueRefinementFieldModel::ResectionMask
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LatticeSite {
    sensitive: u32,
    tolerant: u32,
    resistant: u32,
    oxygen: u32,
    nutrient: u32,
    intervention: u32,
}

impl LatticeSite {
    const fn cells(self) -> u32 {
        self.sensitive
            .saturating_add(self.tolerant)
            .saturating_add(self.resistant)
    }
}

/// Runs all three preregistered structural-assumption scenarios. The protocol
/// must byte-for-byte equal the one derived from the survivor evidence.
pub fn execute_cancer_tissue_refinement(
    candidate: &CancerTissueRefinementCandidate,
    protocol: &CancerTissueRefinementProtocol,
) -> Result<CancerTissueRefinementResult, CancerTissueRefinementError> {
    let expected = prepare_cancer_tissue_refinement_protocol(candidate)?;
    if protocol != &expected {
        return Err(CancerTissueRefinementError::ProtocolDoesNotMatchCampaign);
    }
    let plan = candidate
        .root
        .contribution
        .virtual_experiment_plan
        .as_ref()
        .ok_or(CancerTissueRefinementError::IneligibleCampaign)?;
    let readout = candidate
        .root_result
        .mechanistic_readout
        .as_ref()
        .ok_or(CancerTissueRefinementError::IneligibleCampaign)?;
    let protocol_hash = protocol.canonical_hash()?;
    let mut scenario_summaries = Vec::with_capacity(3);
    let mut snapshots = Vec::new();

    for scenario in CancerTissueRefinementScenario::ALL {
        let (summary, mut scenario_snapshots) = execute_scenario(
            protocol,
            protocol_hash,
            plan.intervention_modality,
            plan.intensity_parts_per_million,
            readout.target_engagement_parts_per_million,
            &readout.baseline_clones,
            scenario,
        );
        scenario_summaries.push(summary);
        snapshots.append(&mut scenario_snapshots);
    }

    let minimum = scenario_summaries
        .iter()
        .map(|summary| summary.final_viable_cells)
        .min()
        .unwrap_or_default();
    let maximum = scenario_summaries
        .iter()
        .map(|summary| summary.final_viable_cells)
        .max()
        .unwrap_or_default();
    let all_scenarios_completed = scenario_summaries.iter().all(|summary| {
        summary.termination == CancerTissueRefinementTermination::CompletedBoundedHorizon
    });
    let result = CancerTissueRefinementResult {
        schema_version: CANCER_TISSUE_REFINEMENT_RESULT_SCHEMA_VERSION,
        method_version: CANCER_TISSUE_REFINEMENT_METHOD_VERSION,
        refinement_id: protocol.refinement_id,
        world_id: protocol.world_id,
        protocol_hash,
        scenario_summaries,
        snapshots,
        uncertainty: CancerTissueRefinementUncertaintyEnvelope {
            minimum_final_viable_cells: minimum,
            maximum_final_viable_cells: maximum,
            final_viable_spread_parts_per_million_of_initial: maximum
                .saturating_sub(minimum)
                .saturating_mul(PARTS_PER_MILLION)
                / protocol.initial_cell_count,
            all_scenarios_completed,
        },
        evidence_class: "uncalibrated_deterministic_tissue_projection".to_owned(),
        caveats: CANCER_TISSUE_REFINEMENT_CAVEATS
            .iter()
            .map(|caveat| (*caveat).to_owned())
            .collect(),
    };
    result.validate_against(protocol)?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn execute_scenario(
    protocol: &CancerTissueRefinementProtocol,
    protocol_hash: Digest,
    modality: CancerVirtualInterventionModality,
    intensity: u32,
    target_engagement: u32,
    baseline_clones: &CancerVirtualCloneFractions,
    scenario: CancerTissueRefinementScenario,
) -> (
    CancerTissueRefinementScenarioSummary,
    Vec<CancerTissueRefinementSnapshot>,
) {
    let width = usize::from(protocol.lattice_width);
    let height = usize::from(protocol.lattice_height);
    let mut lattice = initialize_lattice(
        protocol,
        protocol_hash,
        baseline_clones,
        scenario,
        intensity,
        target_engagement,
        modality,
    );
    let initial_cells = total_cells(&lattice);
    let mut snapshots = Vec::new();
    let mut completed_steps = 0_u16;
    let mut termination = CancerTissueRefinementTermination::CompletedBoundedHorizon;

    for step in 1..=protocol.maximum_steps {
        update_fields(
            &mut lattice,
            width,
            height,
            protocol.field_model,
            modality,
            intensity,
            target_engagement,
            scenario,
        );
        let update = update_cells(&lattice, modality, protocol_hash, scenario, step);
        let Ok(mut next) = update else {
            termination = CancerTissueRefinementTermination::NumericalInvariantRejected;
            break;
        };
        if total_cells(&next) > protocol.cell_capacity {
            termination = CancerTissueRefinementTermination::CellCapacityReached;
            break;
        }
        if step % 4 == 0 {
            redistribute_invasive_cells(&mut next, width, height, step);
        }
        lattice = next;
        completed_steps = step;
        if step % protocol.snapshot_every_steps == 0 || step == protocol.maximum_steps {
            snapshots.push(snapshot(&lattice, protocol, scenario, step));
        }
    }
    if snapshots
        .last()
        .is_none_or(|snapshot| snapshot.step != completed_steps)
    {
        snapshots.push(snapshot(&lattice, protocol, scenario, completed_steps));
    }
    let final_snapshot = snapshots
        .last()
        .expect("every scenario has a final snapshot");
    let summary = CancerTissueRefinementScenarioSummary {
        scenario,
        termination,
        completed_steps,
        initial_viable_cells: initial_cells,
        final_viable_cells: final_snapshot.viable_cells,
        final_treatment_sensitive_cells: final_snapshot.treatment_sensitive_cells,
        final_drug_tolerant_cells: final_snapshot.drug_tolerant_cells,
        final_resistant_cells: final_snapshot.resistant_cells,
        final_mean_oxygen_parts_per_million: final_snapshot.mean_oxygen_parts_per_million,
        final_mean_nutrient_parts_per_million: final_snapshot.mean_nutrient_parts_per_million,
        final_mean_intervention_field_parts_per_million: final_snapshot
            .mean_intervention_field_parts_per_million,
        final_hypoxic_cell_fraction_parts_per_million: final_snapshot
            .hypoxic_cell_fraction_parts_per_million,
        final_invasive_front_fraction_parts_per_million: final_snapshot
            .invasive_front_fraction_parts_per_million,
        lattice_site_updates: u64::from(completed_steps)
            * u64::from(protocol.lattice_width)
            * u64::from(protocol.lattice_height),
    };
    (summary, snapshots)
}

#[allow(clippy::too_many_arguments)]
fn initialize_lattice(
    protocol: &CancerTissueRefinementProtocol,
    protocol_hash: Digest,
    clones: &CancerVirtualCloneFractions,
    scenario: CancerTissueRefinementScenario,
    intensity: u32,
    target_engagement: u32,
    modality: CancerVirtualInterventionModality,
) -> Vec<LatticeSite> {
    let width = usize::from(protocol.lattice_width);
    let height = usize::from(protocol.lattice_height);
    let mut lattice = vec![LatticeSite::default(); width * height];
    let center_x = width / 2;
    let center_y = height / 2;
    let radius = width.min(height) / 5 + 1;
    for (index, site) in lattice.iter_mut().enumerate() {
        let x = index % width;
        let y = index / width;
        let distance = squared_distance(x, y, center_x, center_y);
        let radial_loss = u32::try_from(distance.min(100))
            .unwrap_or_default()
            .saturating_mul(2_000);
        site.oxygen = 720_000_u32.saturating_sub(radial_loss).max(320_000);
        site.nutrient = 680_000_u32.saturating_sub(radial_loss / 2).max(340_000);
        site.intervention = initial_intervention_field(
            protocol.field_model,
            modality,
            x,
            y,
            width,
            height,
            scenario_scaled(
                source_strength(intensity, target_engagement, modality),
                scenario,
            ),
        );
    }

    let mut state = u64::from_be_bytes(
        protocol_hash.as_bytes()[0..8]
            .try_into()
            .expect("digest prefix"),
    ) ^ scenario_salt(scenario);
    let clone_offset = u32::try_from(state % u64::from(PARTS_PER_MILLION)).unwrap_or_default();
    for ordinal in 0..protocol.initial_cell_count {
        let mut selected = center_y * width + center_x;
        for _ in 0..64 {
            state = splitmix64(state);
            let x = usize::try_from(state % u64::try_from(width).unwrap_or(1)).unwrap_or_default();
            state = splitmix64(state);
            let y = usize::try_from(state % u64::try_from(height).unwrap_or(1)).unwrap_or_default();
            if squared_distance(x, y, center_x, center_y) <= radius * radius {
                selected = y * width + x;
                break;
            }
        }
        let selector = (u64::from(ordinal) * u64::from(PARTS_PER_MILLION)
            / u64::from(protocol.initial_cell_count)
            + u64::from(clone_offset))
            % u64::from(PARTS_PER_MILLION);
        let site = &mut lattice[selected];
        if selector < u64::from(clones.treatment_sensitive_parts_per_million) {
            site.sensitive += 1;
        } else if selector
            < u64::from(
                clones
                    .treatment_sensitive_parts_per_million
                    .saturating_add(clones.drug_tolerant_parts_per_million),
            )
        {
            site.tolerant += 1;
        } else {
            site.resistant += 1;
        }
    }
    lattice
}

#[allow(clippy::too_many_arguments)]
fn update_fields(
    lattice: &mut [LatticeSite],
    width: usize,
    height: usize,
    field_model: CancerTissueRefinementFieldModel,
    modality: CancerVirtualInterventionModality,
    intensity: u32,
    target_engagement: u32,
    scenario: CancerTissueRefinementScenario,
) {
    let previous = lattice.to_vec();
    let source = scenario_scaled(
        source_strength(intensity, target_engagement, modality),
        scenario,
    );
    for (index, site) in lattice.iter_mut().enumerate() {
        let x = index % width;
        let y = index / width;
        let boundary = x == 0 || y == 0 || x + 1 == width || y + 1 == height;
        let population = previous[index].cells();
        site.oxygen = if boundary {
            820_000
        } else {
            diffuse(
                &previous,
                index,
                width,
                height,
                |entry| entry.oxygen,
                180_000,
            )
            .saturating_sub(population.saturating_mul(1_200))
        };
        site.nutrient = if boundary {
            780_000
        } else {
            diffuse(
                &previous,
                index,
                width,
                height,
                |entry| entry.nutrient,
                120_000,
            )
            .saturating_sub(population.saturating_mul(900))
        };
        site.intervention = match field_model {
            CancerTissueRefinementFieldModel::DiffusiveExposure => {
                let diffused = diffuse(
                    &previous,
                    index,
                    width,
                    height,
                    |entry| entry.intervention,
                    220_000,
                );
                let decayed = multiply_ppm(diffused, 990_000);
                if is_exposure_source(modality, x, y, width, height) {
                    decayed.max(source)
                } else {
                    decayed
                }
            }
            CancerTissueRefinementFieldModel::DeviceField => {
                initial_intervention_field(field_model, modality, x, y, width, height, source)
            }
            CancerTissueRefinementFieldModel::ResectionMask => previous[index].intervention,
        };
    }
}

fn diffuse(
    lattice: &[LatticeSite],
    index: usize,
    width: usize,
    height: usize,
    field: impl Fn(LatticeSite) -> u32,
    coefficient: u32,
) -> u32 {
    let x = index % width;
    let y = index / width;
    let mut total = 0_u64;
    let mut count = 0_u64;
    if x > 0 {
        total += u64::from(field(lattice[index - 1]));
        count += 1;
    }
    if x + 1 < width {
        total += u64::from(field(lattice[index + 1]));
        count += 1;
    }
    if y > 0 {
        total += u64::from(field(lattice[index - width]));
        count += 1;
    }
    if y + 1 < height {
        total += u64::from(field(lattice[index + width]));
        count += 1;
    }
    let current = i64::from(field(lattice[index]));
    let mean = i64::try_from(total / count.max(1)).unwrap_or(i64::MAX);
    let delta = (mean - current) * i64::from(coefficient) / i64::from(PARTS_PER_MILLION);
    u32::try_from((current + delta).clamp(0, i64::from(PARTS_PER_MILLION))).unwrap_or_default()
}

fn update_cells(
    lattice: &[LatticeSite],
    modality: CancerVirtualInterventionModality,
    protocol_hash: Digest,
    scenario: CancerTissueRefinementScenario,
    step: u16,
) -> Result<Vec<LatticeSite>, ()> {
    let mut next = lattice.to_vec();
    for (index, site) in lattice.iter().copied().enumerate() {
        let resource = site.oxygen.min(site.nutrient);
        let hypoxia_death = if site.oxygen < OXYGEN_HYPOXIA_THRESHOLD {
            (OXYGEN_HYPOXIA_THRESHOLD - site.oxygen).saturating_mul(120_000)
                / OXYGEN_HYPOXIA_THRESHOLD
        } else {
            0
        };
        let base_kill = modality_kill_rate(modality);
        let treatment_rate = multiply_ppm(site.intervention, base_kill);
        let salt = u64::from_be_bytes(
            protocol_hash.as_bytes()[8..16]
                .try_into()
                .expect("digest salt"),
        ) ^ scenario_salt(scenario)
            ^ u64::from(step)
            ^ u64::try_from(index).unwrap_or(u64::MAX).rotate_left(17);
        next[index].sensitive = update_clone(
            site.sensitive,
            multiply_ppm(2_800, resource),
            multiply_ppm(treatment_rate, 780_000).saturating_add(hypoxia_death),
            salt,
        )?;
        next[index].tolerant = update_clone(
            site.tolerant,
            multiply_ppm(2_300, resource),
            multiply_ppm(treatment_rate, 360_000)
                .saturating_add(multiply_ppm(hypoxia_death, 700_000)),
            salt.rotate_left(21),
        )?;
        next[index].resistant = update_clone(
            site.resistant,
            multiply_ppm(1_900, resource),
            multiply_ppm(treatment_rate, 120_000)
                .saturating_add(multiply_ppm(hypoxia_death, 450_000)),
            salt.rotate_left(42),
        )?;
        if modality == CancerVirtualInterventionModality::SurgicalResection
            && step == 1
            && site.intervention >= 500_000
        {
            next[index].sensitive = 0;
            next[index].tolerant = 0;
            next[index].resistant = 0;
        }
    }
    Ok(next)
}

fn update_clone(count: u32, growth_rate: u32, death_rate: u32, salt: u64) -> Result<u32, ()> {
    let births = deterministic_scaled_count(count, growth_rate.min(PARTS_PER_MILLION), salt)?;
    let deaths = deterministic_scaled_count(
        count,
        death_rate.min(PARTS_PER_MILLION),
        salt.rotate_left(29),
    )?;
    count
        .checked_add(births)
        .map(|value| value.saturating_sub(deaths))
        .ok_or(())
}

fn deterministic_scaled_count(count: u32, rate: u32, salt: u64) -> Result<u32, ()> {
    let product = u64::from(count).checked_mul(u64::from(rate)).ok_or(())?;
    let whole = product / u64::from(PARTS_PER_MILLION);
    let remainder = product % u64::from(PARTS_PER_MILLION);
    let rounded = whole + u64::from(splitmix64(salt) % u64::from(PARTS_PER_MILLION) < remainder);
    u32::try_from(rounded).map_err(|_| ())
}

fn redistribute_invasive_cells(
    lattice: &mut [LatticeSite],
    width: usize,
    height: usize,
    step: u16,
) {
    let previous = lattice.to_vec();
    for index in 0..previous.len() {
        if previous[index].cells() < 5 {
            continue;
        }
        let neighbors = neighbors(index, width, height);
        let Some(destination) = neighbors
            .into_iter()
            .min_by_key(|neighbor| (previous[*neighbor].cells(), *neighbor))
        else {
            continue;
        };
        if previous[destination].cells() >= previous[index].cells() {
            continue;
        }
        let prefer_resistant = (usize::from(step) + index) % 2 == 0;
        if prefer_resistant && lattice[index].resistant > 0 {
            lattice[index].resistant -= 1;
            lattice[destination].resistant += 1;
        } else if lattice[index].tolerant > 0 {
            lattice[index].tolerant -= 1;
            lattice[destination].tolerant += 1;
        } else if lattice[index].resistant > 0 {
            lattice[index].resistant -= 1;
            lattice[destination].resistant += 1;
        }
    }
}

fn neighbors(index: usize, width: usize, height: usize) -> Vec<usize> {
    let x = index % width;
    let y = index / width;
    let mut values = Vec::with_capacity(4);
    if x > 0 {
        values.push(index - 1);
    }
    if x + 1 < width {
        values.push(index + 1);
    }
    if y > 0 {
        values.push(index - width);
    }
    if y + 1 < height {
        values.push(index + width);
    }
    values
}

fn snapshot(
    lattice: &[LatticeSite],
    protocol: &CancerTissueRefinementProtocol,
    scenario: CancerTissueRefinementScenario,
    step: u16,
) -> CancerTissueRefinementSnapshot {
    let sites = u64::try_from(lattice.len()).unwrap_or(1).max(1);
    let sensitive = lattice
        .iter()
        .map(|site| u64::from(site.sensitive))
        .sum::<u64>();
    let tolerant = lattice
        .iter()
        .map(|site| u64::from(site.tolerant))
        .sum::<u64>();
    let resistant = lattice
        .iter()
        .map(|site| u64::from(site.resistant))
        .sum::<u64>();
    let total = sensitive + tolerant + resistant;
    let hypoxic = lattice
        .iter()
        .filter(|site| site.oxygen < OXYGEN_HYPOXIA_THRESHOLD)
        .map(|site| u64::from(site.cells()))
        .sum::<u64>();
    let width = usize::from(protocol.lattice_width);
    let height = usize::from(protocol.lattice_height);
    let center_x = width / 2;
    let center_y = height / 2;
    let initial_radius = width.min(height) / 5 + 1;
    let invasive = lattice
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            squared_distance(index % width, index / width, center_x, center_y)
                > initial_radius * initial_radius
        })
        .map(|(_, site)| u64::from(site.cells()))
        .sum::<u64>();
    CancerTissueRefinementSnapshot {
        scenario,
        step,
        viable_cells: u32::try_from(total).unwrap_or(u32::MAX),
        treatment_sensitive_cells: u32::try_from(sensitive).unwrap_or(u32::MAX),
        drug_tolerant_cells: u32::try_from(tolerant).unwrap_or(u32::MAX),
        resistant_cells: u32::try_from(resistant).unwrap_or(u32::MAX),
        mean_oxygen_parts_per_million: u32::try_from(
            lattice
                .iter()
                .map(|site| u64::from(site.oxygen))
                .sum::<u64>()
                / sites,
        )
        .unwrap_or(u32::MAX),
        mean_nutrient_parts_per_million: u32::try_from(
            lattice
                .iter()
                .map(|site| u64::from(site.nutrient))
                .sum::<u64>()
                / sites,
        )
        .unwrap_or(u32::MAX),
        mean_intervention_field_parts_per_million: u32::try_from(
            lattice
                .iter()
                .map(|site| u64::from(site.intervention))
                .sum::<u64>()
                / sites,
        )
        .unwrap_or(u32::MAX),
        hypoxic_cell_fraction_parts_per_million: fraction(hypoxic, total),
        invasive_front_fraction_parts_per_million: fraction(invasive, total),
    }
}

fn fraction(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    u32::try_from(numerator.saturating_mul(u64::from(PARTS_PER_MILLION)) / denominator)
        .unwrap_or(PARTS_PER_MILLION)
        .min(PARTS_PER_MILLION)
}

fn total_cells(lattice: &[LatticeSite]) -> u32 {
    u32::try_from(
        lattice
            .iter()
            .map(|site| u64::from(site.cells()))
            .sum::<u64>(),
    )
    .unwrap_or(u32::MAX)
}

fn initial_intervention_field(
    field_model: CancerTissueRefinementFieldModel,
    modality: CancerVirtualInterventionModality,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    source: u32,
) -> u32 {
    match field_model {
        CancerTissueRefinementFieldModel::DiffusiveExposure => {
            if is_exposure_source(modality, x, y, width, height) {
                source
            } else {
                0
            }
        }
        CancerTissueRefinementFieldModel::DeviceField => {
            let center_x = width / 2;
            let distance = x.abs_diff(center_x);
            let span = center_x.max(1);
            let uniformity = 700_000_u32.saturating_add(
                300_000_u32.saturating_mul(
                    u32::try_from(span.saturating_sub(distance.min(span))).unwrap_or_default(),
                ) / u32::try_from(span).unwrap_or(1),
            );
            multiply_ppm(source, uniformity)
        }
        CancerTissueRefinementFieldModel::ResectionMask => {
            let radius = width.min(height) / 6 + 1;
            if squared_distance(x, y, width / 2, height / 2) <= radius * radius {
                source
            } else {
                0
            }
        }
    }
}

fn is_exposure_source(
    modality: CancerVirtualInterventionModality,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> bool {
    match modality {
        CancerVirtualInterventionModality::MolecularInhibition => {
            x == 0 || y == 0 || x + 1 == width || y + 1 == height
        }
        CancerVirtualInterventionModality::TargetedDelivery
        | CancerVirtualInterventionModality::Thermal => {
            x.abs_diff(width / 2) <= 1 && y.abs_diff(height / 2) <= 1
        }
        _ => false,
    }
}

fn source_strength(
    intensity: u32,
    target_engagement: u32,
    modality: CancerVirtualInterventionModality,
) -> u32 {
    match modality {
        CancerVirtualInterventionModality::MolecularInhibition
        | CancerVirtualInterventionModality::TargetedDelivery => target_engagement,
        _ => intensity,
    }
}

const fn modality_kill_rate(modality: CancerVirtualInterventionModality) -> u32 {
    match modality {
        CancerVirtualInterventionModality::MolecularInhibition => 32_000,
        CancerVirtualInterventionModality::Radiation => 28_000,
        CancerVirtualInterventionModality::Thermal => 24_000,
        CancerVirtualInterventionModality::ElectricField => 18_000,
        CancerVirtualInterventionModality::TargetedDelivery => 36_000,
        CancerVirtualInterventionModality::SurgicalResection
        | CancerVirtualInterventionModality::DiagnosticSensing => 0,
    }
}

fn scenario_scaled(value: u32, scenario: CancerTissueRefinementScenario) -> u32 {
    let scale = match scenario {
        CancerTissueRefinementScenario::LowerFieldBound => 750_000,
        CancerTissueRefinementScenario::NominalAssumptions => 1_000_000,
        CancerTissueRefinementScenario::UpperFieldBound => 1_250_000,
    };
    multiply_ppm(value, scale).min(PARTS_PER_MILLION)
}

fn multiply_ppm(left: u32, right: u32) -> u32 {
    u32::try_from(u64::from(left) * u64::from(right) / u64::from(PARTS_PER_MILLION))
        .unwrap_or(u32::MAX)
}

fn squared_distance(x: usize, y: usize, center_x: usize, center_y: usize) -> usize {
    x.abs_diff(center_x).pow(2) + y.abs_diff(center_y).pow(2)
}

const fn scenario_salt(scenario: CancerTissueRefinementScenario) -> u64 {
    match scenario {
        CancerTissueRefinementScenario::LowerFieldBound => 0x243f_6a88_85a3_08d3,
        CancerTissueRefinementScenario::NominalAssumptions => 0x1319_8a2e_0370_7344,
        CancerTissueRefinementScenario::UpperFieldBound => 0xa409_3822_299f_31d0,
    }
}

const fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancerTissueRefinementJobLease {
    pub refinement_id: Uuid,
    pub protocol_hash: Digest,
}

/// Process-local half of the one-job-at-a-time worker contract. A durable
/// adapter must pair this with an atomic single-row claim; this state machine
/// prevents the pure executor from accepting concurrent work by construction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CancerTissueRefinementWorkSlot {
    active: Option<CancerTissueRefinementJobLease>,
}

impl CancerTissueRefinementWorkSlot {
    pub fn begin(
        &mut self,
        protocol: &CancerTissueRefinementProtocol,
    ) -> Result<CancerTissueRefinementJobLease, CancerTissueRefinementError> {
        if self.active.is_some() {
            return Err(CancerTissueRefinementError::WorkerBusy);
        }
        let lease = CancerTissueRefinementJobLease {
            refinement_id: protocol.refinement_id,
            protocol_hash: protocol.canonical_hash()?,
        };
        self.active = Some(lease);
        Ok(lease)
    }

    pub fn finish(
        &mut self,
        lease: CancerTissueRefinementJobLease,
        protocol: &CancerTissueRefinementProtocol,
        result: &CancerTissueRefinementResult,
    ) -> Result<Digest, CancerTissueRefinementError> {
        if self.active != Some(lease)
            || lease.refinement_id != protocol.refinement_id
            || lease.protocol_hash != protocol.canonical_hash()?
        {
            return Err(CancerTissueRefinementError::JobLeaseMismatch);
        }
        let result_hash = result.canonical_hash(protocol)?;
        self.active = None;
        Ok(result_hash)
    }

    pub fn release_failed(
        &mut self,
        lease: CancerTissueRefinementJobLease,
    ) -> Result<(), CancerTissueRefinementError> {
        if self.active != Some(lease) {
            return Err(CancerTissueRefinementError::JobLeaseMismatch);
        }
        self.active = None;
        Ok(())
    }

    #[must_use]
    pub const fn is_busy(&self) -> bool {
        self.active.is_some()
    }
}

#[derive(Debug, Error)]
pub enum CancerTissueRefinementError {
    #[error("only a complete adversarial campaign survivor is eligible for tissue refinement")]
    IneligibleCampaign,
    #[error("the supplied tissue protocol does not match the preregistered campaign protocol")]
    ProtocolDoesNotMatchCampaign,
    #[error("the bounded tissue worker already has an active job")]
    WorkerBusy,
    #[error("the tissue worker lease does not match the active job")]
    JobLeaseMismatch,
    #[error(transparent)]
    ResearchContract(#[from] CancerResearchContractError),
    #[error(transparent)]
    TissueContract(#[from] CancerTissueRefinementContractError),
    #[error(transparent)]
    CanonicalHash(#[from] world_domain::CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute_cancer_virtual_experiment;
    use world_domain::{
        CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION, CancerResearchArtifactKind,
        CancerResearchClaim, CancerResearchContribution, CancerResearchInferenceTier,
        CancerResearchProfile, CancerResearchTarget, CancerResearchTask,
        CancerResearchTurnSelection, CancerVirtualEndpoint, CancerVirtualExperimentPlan,
        CancerVirtualMechanismTarget, EntityId, SimTick, WorldId, WorldSeed,
    };

    fn experiment(
        world_id: WorldId,
        ordinal: u32,
        stage: CancerResearchStage,
        exposure_hours: u16,
        modality: CancerVirtualInterventionModality,
    ) -> CancerVirtualExperimentCandidate {
        experiment_with_subject(
            world_id,
            ordinal,
            stage,
            exposure_hours,
            modality,
            CancerVirtualSubjectModel::TumorOrganoid,
            CancerVirtualEndpoint::ViableTumorFraction,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn experiment_with_subject(
        world_id: WorldId,
        ordinal: u32,
        stage: CancerResearchStage,
        exposure_hours: u16,
        modality: CancerVirtualInterventionModality,
        subject_model: CancerVirtualSubjectModel,
        endpoint: CancerVirtualEndpoint,
    ) -> CancerVirtualExperimentCandidate {
        let resident_id = EntityId::deterministic(world_id, &ordinal.to_be_bytes());
        let frozen_candidate_hash = (stage == CancerResearchStage::IndependentReplication)
            .then(|| Digest::sha256(b"test-frozen-root"));
        let selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(u64::from(ordinal) * 2 + 1),
            SimTick::new(u64::from(ordinal) * 2 + 2),
            ordinal,
            CancerResearchTarget::AdultGlioblastoma,
            stage,
            if stage == CancerResearchStage::IndependentReplication {
                CancerResearchTask::DesignIndependentReplication
            } else {
                CancerResearchTask::ProposeDiscriminatingExperiment
            },
            CancerResearchInferenceTier::Exploration,
            CancerResearchProfile::seeded(WorldSeed::new(42), resident_id).expect("profile"),
            Vec::new(),
            frozen_candidate_hash,
            512,
        )
        .expect("selection");
        let contribution = CancerResearchContribution::new_with_virtual_experiment(
            &selection,
            CancerResearchArtifactKind::ExperimentProposal,
            format!("Preregistered experiment {ordinal}"),
            "A bounded virtual comparison with a frozen plan.",
            vec![CancerResearchClaim {
                statement: "The intervention changes viable tumor fraction.".to_owned(),
                testable_prediction: "The intervention differs from control.".to_owned(),
                falsification_test:
                    "The bounded model interval does not show the predicted direction.".to_owned(),
                citation_hashes: Vec::new(),
            }],
            Some(CancerVirtualExperimentPlan {
                schema_version: CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION,
                subject_model,
                intervention_modality: modality,
                primary_target: CancerVirtualMechanismTarget::CellDivision,
                secondary_target: None,
                primary_endpoint: endpoint,
                intensity_parts_per_million: 900_000,
                exposure_hours,
                cohort_size: 128 + u16::try_from(ordinal).unwrap_or_default(),
            }),
        )
        .expect("contribution");
        CancerVirtualExperimentCandidate {
            world_id,
            request_id: contribution.request_id,
            ordinal,
            artifact_hash: contribution.canonical_hash().expect("artifact hash"),
            contribution,
        }
    }

    fn surviving_candidate() -> CancerTissueRefinementCandidate {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x5155));
        let root = experiment(
            world_id,
            1,
            CancerResearchStage::BlindDiscovery,
            168,
            CancerVirtualInterventionModality::MolecularInhibition,
        );
        let root_result = execute_cancer_virtual_experiment(&root).expect("root result");
        assert_eq!(
            root_result.interpretation,
            CancerVirtualExperimentInterpretation::ModelSupportsPrediction
        );
        let root_hash = root.artifact_hash;
        let campaign_id = CancerResearchCampaignDirective::campaign_id(root.request_id);
        let campaign_experiments = [96_u16, 120, 144]
            .into_iter()
            .enumerate()
            .map(|(offset, exposure)| {
                let candidate = experiment(
                    world_id,
                    u32::try_from(offset).expect("offset") + 2,
                    CancerResearchStage::IndependentReplication,
                    exposure,
                    CancerVirtualInterventionModality::MolecularInhibition,
                );
                let result = execute_cancer_virtual_experiment(&candidate).expect("result");
                assert_eq!(
                    cancer_research_campaign_test_assessment(&result),
                    CancerResearchCampaignTestAssessment::Supports
                );
                CancerTissueRefinementCampaignExperiment {
                    frozen_root_artifact_hash: root_hash,
                    candidate,
                    result,
                }
            })
            .collect();
        CancerTissueRefinementCandidate {
            campaign_id,
            survival_evidence: CancerTissueRefinementSurvivalEvidence {
                synthesis_request_id: Uuid::from_u128(0x5155_ffff),
                synthesis_request_hash: Digest::sha256(b"synthesis request"),
                synthesis_result_hash: Digest::sha256(b"synthesis result"),
                campaign_id,
                root_artifact_hash: root_hash,
                supporting_tests: 3,
                falsifying_tests: 0,
                inconclusive_tests: 0,
            },
            root,
            root_result,
            campaign_experiments,
        }
    }

    #[test]
    fn protocol_and_execution_are_bit_for_bit_repeatable() {
        let candidate = surviving_candidate();
        let first_protocol =
            prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        let second_protocol =
            prepare_cancer_tissue_refinement_protocol(&candidate).expect("same protocol");
        assert_eq!(first_protocol, second_protocol);
        let first =
            execute_cancer_tissue_refinement(&candidate, &first_protocol).expect("first result");
        let second =
            execute_cancer_tissue_refinement(&candidate, &second_protocol).expect("second result");
        assert_eq!(first, second);
        assert_eq!(
            first.canonical_hash(&first_protocol).expect("hash"),
            second.canonical_hash(&second_protocol).expect("same hash")
        );
    }

    #[test]
    fn output_and_execution_stay_inside_every_declared_ceiling() {
        let candidate = surviving_candidate();
        let protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        let result = execute_cancer_tissue_refinement(&candidate, &protocol).expect("result");
        assert!(protocol.lattice_width <= 32);
        assert!(protocol.lattice_height <= 32);
        assert!(protocol.maximum_steps <= 256);
        assert!(protocol.cell_capacity <= CANCER_TISSUE_REFINEMENT_MAX_CELLS);
        assert!(result.snapshots.len() <= 48);
        assert!(
            result
                .scenario_summaries
                .iter()
                .all(|summary| summary.final_viable_cells <= protocol.cell_capacity)
        );
        assert!(result.validate_against(&protocol).is_ok());
    }

    #[test]
    fn uncertainty_scenarios_are_ordered_and_do_not_claim_efficacy() {
        let candidate = surviving_candidate();
        let protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        let result = execute_cancer_tissue_refinement(&candidate, &protocol).expect("result");
        assert_eq!(
            result
                .scenario_summaries
                .iter()
                .map(|summary| summary.scenario)
                .collect::<Vec<_>>(),
            CancerTissueRefinementScenario::ALL
        );
        assert!(
            result.uncertainty.minimum_final_viable_cells
                <= result.uncertainty.maximum_final_viable_cells
        );
        let public_words = serde_json::to_string(&result)
            .expect("serialize")
            .to_lowercase();
        assert!(!public_words.contains("cure"));
        assert!(!public_words.contains("clinically effective"));
        assert!(!public_words.contains("treatment works"));
    }

    #[test]
    fn incomplete_or_adverse_campaigns_never_enter_the_tissue_tier() {
        let mut incomplete = surviving_candidate();
        incomplete.campaign_experiments.truncate(2);
        assert!(matches!(
            prepare_cancer_tissue_refinement_protocol(&incomplete),
            Err(CancerTissueRefinementError::IneligibleCampaign)
        ));

        let mut adverse = surviving_candidate();
        adverse.campaign_experiments[0].result.interpretation =
            CancerVirtualExperimentInterpretation::ModelShowsNoMaterialEffect;
        assert!(matches!(
            prepare_cancer_tissue_refinement_protocol(&adverse),
            Err(CancerTissueRefinementError::IneligibleCampaign)
        ));
    }

    #[test]
    fn a_post_registration_protocol_change_is_rejected() {
        let candidate = surviving_candidate();
        let mut protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        protocol.maximum_steps -= 1;
        assert!(matches!(
            execute_cancer_tissue_refinement(&candidate, &protocol),
            Err(CancerTissueRefinementError::ProtocolDoesNotMatchCampaign)
        ));
    }

    #[test]
    fn one_work_slot_cannot_claim_two_jobs() {
        let candidate = surviving_candidate();
        let protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        let mut slot = CancerTissueRefinementWorkSlot::default();
        let lease = slot.begin(&protocol).expect("first claim");
        assert!(slot.is_busy());
        assert!(matches!(
            slot.begin(&protocol),
            Err(CancerTissueRefinementError::WorkerBusy)
        ));
        let result = execute_cancer_tissue_refinement(&candidate, &protocol).expect("result");
        slot.finish(lease, &protocol, &result).expect("finish");
        assert!(!slot.is_busy());
    }

    #[test]
    fn method_two_result_remains_valid_after_refinement() {
        let candidate = surviving_candidate();
        let before = candidate
            .root_result
            .canonical_hash(&candidate.root.contribution)
            .expect("method-2 hash");
        let protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        execute_cancer_tissue_refinement(&candidate, &protocol).expect("refinement");
        let after = candidate
            .root_result
            .canonical_hash(&candidate.root.contribution)
            .expect("same method-2 hash");
        assert_eq!(before, after);
        assert_eq!(candidate.root_result.method_version, 2);
    }

    #[test]
    fn modalities_select_a_preregistered_spatial_field_family() {
        assert_eq!(
            field_model(CancerVirtualInterventionModality::TargetedDelivery),
            CancerTissueRefinementFieldModel::DiffusiveExposure
        );
        assert_eq!(
            field_model(CancerVirtualInterventionModality::ElectricField),
            CancerTissueRefinementFieldModel::DeviceField
        );
        assert_eq!(
            field_model(CancerVirtualInterventionModality::SurgicalResection),
            CancerTissueRefinementFieldModel::ResectionMask
        );
    }

    #[test]
    fn field_uncertainty_changes_the_exposure_trace_without_changing_the_protocol() {
        let candidate = surviving_candidate();
        let protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        let result = execute_cancer_tissue_refinement(&candidate, &protocol).expect("result");
        let exposure = result
            .scenario_summaries
            .iter()
            .map(|summary| summary.final_mean_intervention_field_parts_per_million)
            .collect::<Vec<_>>();
        assert!(exposure[0] < exposure[1]);
        assert!(exposure[1] < exposure[2]);
    }

    #[test]
    fn cell_capacity_is_an_explicit_early_termination_not_a_favorable_result() {
        let candidate = surviving_candidate();
        let mut protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        protocol.cell_capacity = protocol.initial_cell_count;
        protocol.validate().expect("lower cap remains valid");
        let protocol_hash = protocol.canonical_hash().expect("protocol hash");
        let readout = candidate
            .root_result
            .mechanistic_readout
            .as_ref()
            .expect("readout");
        let (summary, snapshots) = execute_scenario(
            &protocol,
            protocol_hash,
            CancerVirtualInterventionModality::DiagnosticSensing,
            1_000_000,
            1_000_000,
            &readout.baseline_clones,
            CancerTissueRefinementScenario::NominalAssumptions,
        );
        assert_eq!(
            summary.termination,
            CancerTissueRefinementTermination::CellCapacityReached
        );
        assert!(summary.completed_steps < protocol.maximum_steps);
        assert_eq!(
            snapshots.last().expect("final snapshot").step,
            summary.completed_steps
        );
    }

    #[test]
    fn result_contract_rejects_summary_or_disclaimer_tampering() {
        let candidate = surviving_candidate();
        let protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        let result = execute_cancer_tissue_refinement(&candidate, &protocol).expect("result");
        let mut changed_summary = result.clone();
        changed_summary.scenario_summaries[0].final_mean_oxygen_parts_per_million += 1;
        assert!(changed_summary.validate_against(&protocol).is_err());
        let mut changed_caveat = result;
        changed_caveat.caveats[0] = "This model proves efficacy.".to_owned();
        assert!(changed_caveat.validate_against(&protocol).is_err());
    }

    #[test]
    fn duplicated_followup_or_legacy_root_is_ineligible() {
        let mut duplicate = surviving_candidate();
        duplicate
            .campaign_experiments
            .push(duplicate.campaign_experiments[0].clone());
        assert!(matches!(
            prepare_cancer_tissue_refinement_protocol(&duplicate),
            Err(CancerTissueRefinementError::IneligibleCampaign)
        ));

        let mut legacy = surviving_candidate();
        legacy.root_result.schema_version =
            world_domain::LEGACY_CANCER_VIRTUAL_EXPERIMENT_RESULT_SCHEMA_VERSION;
        legacy.root_result.method_version = 1;
        legacy.root_result.experiment_id = CancerVirtualExperimentResult::deterministic_id(
            legacy.root_result.request_id,
            legacy.root_result.method_version,
        );
        legacy.root_result.model_calibration = "uncalibrated_mechanistic_projection_v1".to_owned();
        legacy.root_result.mechanistic_readout = None;
        legacy.root_result.caveats.truncate(2);
        assert!(
            legacy
                .root_result
                .validate_against(&legacy.root.contribution)
                .is_ok()
        );
        assert!(matches!(
            prepare_cancer_tissue_refinement_protocol(&legacy),
            Err(CancerTissueRefinementError::IneligibleCampaign)
        ));
    }

    #[test]
    fn long_exposure_is_disclosed_as_truncated_instead_of_time_compressed() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x5156));
        let root = experiment_with_subject(
            world_id,
            1,
            CancerResearchStage::BlindDiscovery,
            2_160,
            CancerVirtualInterventionModality::DiagnosticSensing,
            CancerVirtualSubjectModel::OrthotopicMouse,
            CancerVirtualEndpoint::DetectionSensitivity,
        );
        let root_result = execute_cancer_virtual_experiment(&root).expect("root result");
        let root_hash = root.artifact_hash;
        let campaign_id = CancerResearchCampaignDirective::campaign_id(root.request_id);
        let followups = [2_u32, 3, 4]
            .into_iter()
            .map(|ordinal| {
                let candidate = experiment_with_subject(
                    world_id,
                    ordinal,
                    CancerResearchStage::IndependentReplication,
                    2_160 - u16::try_from(ordinal).expect("ordinal"),
                    CancerVirtualInterventionModality::DiagnosticSensing,
                    CancerVirtualSubjectModel::OrthotopicMouse,
                    CancerVirtualEndpoint::DetectionSensitivity,
                );
                let result = execute_cancer_virtual_experiment(&candidate).expect("result");
                CancerTissueRefinementCampaignExperiment {
                    frozen_root_artifact_hash: root_hash,
                    candidate,
                    result,
                }
            })
            .collect();
        let candidate = CancerTissueRefinementCandidate {
            campaign_id,
            survival_evidence: CancerTissueRefinementSurvivalEvidence {
                synthesis_request_id: Uuid::from_u128(0x5156_ffff),
                synthesis_request_hash: Digest::sha256(b"long synthesis request"),
                synthesis_result_hash: Digest::sha256(b"long synthesis result"),
                campaign_id,
                root_artifact_hash: root_hash,
                supporting_tests: 3,
                falsifying_tests: 0,
                inconclusive_tests: 0,
            },
            root,
            root_result,
            campaign_experiments: followups,
        };
        let protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        assert_eq!(protocol.requested_exposure_hours, 2_160);
        assert_eq!(protocol.modeled_exposure_hours, 256);
        assert_eq!(protocol.maximum_steps, 256);
        assert!(protocol.horizon_truncated);
    }

    #[test]
    fn failed_lease_release_requires_the_exact_active_job() {
        let candidate = surviving_candidate();
        let protocol = prepare_cancer_tissue_refinement_protocol(&candidate).expect("protocol");
        let mut slot = CancerTissueRefinementWorkSlot::default();
        let lease = slot.begin(&protocol).expect("claim");
        let wrong = CancerTissueRefinementJobLease {
            refinement_id: Uuid::from_u128(999),
            protocol_hash: lease.protocol_hash,
        };
        assert!(matches!(
            slot.release_failed(wrong),
            Err(CancerTissueRefinementError::JobLeaseMismatch)
        ));
        assert!(slot.is_busy());
        slot.release_failed(lease).expect("exact lease");
        assert!(!slot.is_busy());
    }
}
