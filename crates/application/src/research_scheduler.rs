use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CancerBurdenState, CancerResearchEvidenceKind, CancerResearchEvidenceReference,
    CancerResearchInferenceTier, CancerResearchProfile, CancerResearchProgram, CancerResearchStage,
    CancerResearchTask, CancerTrajectory, CancerVirtualEndpoint, CancerVirtualExperimentPlan,
    CancerVirtualInterventionModality, CancerVirtualSubjectModel, Digest, EntityId, OrganismRole,
    SimTick, WorldExperimentCommitment,
};

use crate::{
    CANCER_RESEARCH_CAMPAIGN_DIRECTIVE_SCHEMA_VERSION, CANCER_RESEARCH_CAMPAIGN_MAX_TESTS,
    CANCER_RESEARCH_CAMPAIGN_REQUIRED_SUPPORTS, CancerResearchCampaignCandidate,
    CancerResearchCampaignDirective, CancerResearchCampaignOutcome,
    CancerResearchCampaignTestAssessment, CancerResearchCampaignVariation,
    CancerResearchEvidenceDocument, CancerResearchJobStore, CancerResearchMemoryInput,
    CancerResearchModelRequest, MAX_CANCER_RESEARCH_CATALOG_ENTRIES, StoreError,
    cancer_research_campaign_test_assessment,
};

pub const CANCER_RESEARCH_SCHEDULER_VERSION: u16 = 1;
/// Seven turns per ten canonical ticks is 1,008 turns per 24 wall-clock hours
/// at the production runner's one-minute cadence. The ratio is integer and
/// deterministic; a slower runner slows research rather than dropping work.
pub const CANCER_RESEARCH_TURNS_PER_TEN_TICKS: u64 = 7;
pub const CANCER_RESEARCH_SCHEDULE_TICK_SPAN: u64 = 10;
/// One in five turns in each program is reserved for an eligible theory
/// campaign. If nothing has earned promotion, the slot remains ordinary blind
/// discovery rather than manufacturing a promising result.
pub const CANCER_RESEARCH_CAMPAIGN_INTERVAL_PROGRAM_TURNS: u32 = 5;
const SECONDS_PER_DAY: u64 = 86_400;
const BLIND_RESEARCH_MAX_OUTPUT_TOKENS: u16 = 4_096;
const EMBEDDED_PRIMITIVES: &str =
    include_str!("../../../data/cancer-research/biological-primitives-v1.json");
const EMBEDDED_NCI60_CHALLENGE_CATALOGUE: &str = include_str!(
    "../../../data/cancer-research/nci-cellminer-2-15-cns-challenge-catalogue-v1.json"
);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrimitiveBundle {
    schema_version: u16,
    bundle_id: String,
    records: Vec<PrimitiveRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrimitiveRecord {
    source_id: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Nci60ChallengeCatalogue {
    schema_version: u16,
    catalogue_id: String,
    evidence_class: String,
    intended_use: String,
    source_registry_hash: Digest,
    source: serde_json::Value,
    cns_cell_lines: Vec<String>,
    single_agent_partition: serde_json::Value,
    combination_partition: serde_json::Value,
    single_agent_candidates: Vec<Nci60SingleAgentCandidate>,
    combination_candidates: Vec<Nci60CombinationCandidate>,
    leakage_boundary: Nci60CatalogueLeakageBoundary,
    limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Nci60CatalogueLeakageBoundary {
    access_class: String,
    allowed_in_model_context: bool,
    contains_observed_response_values: bool,
    contains_derived_rank_labels: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Nci60ChallengeCompound {
    nsc: u64,
    drug_name: String,
    mechanism: Option<String>,
    fda_approved: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Nci60SingleAgentCandidate {
    challenge_id: String,
    compound: Nci60ChallengeCompound,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Nci60CombinationCandidate {
    challenge_id: String,
    first: Nci60ChallengeCompound,
    second: Nci60ChallengeCompound,
    source_record_count: usize,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct Nci60PromptSafeChallenge<'a> {
    schema_version: u16,
    catalogue_id: &'a str,
    source_candidate_id: &'a str,
    evidence_class: &'a str,
    intended_use: &'a str,
    cns_cell_lines: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    single_agent: Option<&'a Nci60SingleAgentCandidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    combination: Option<&'a Nci60CombinationCandidate>,
    limitations: &'a [String],
}

/// Derives and idempotently enqueues the exact blind-discovery turn due at the
/// current canonical tick. This is a research projection: it does not write a
/// simulation event and cannot affect the causal world.
pub async fn schedule_due_cancer_research_turn<S: CancerResearchJobStore + ?Sized>(
    store: &S,
    state: &sim_engine::EngineState,
) -> Result<Option<Uuid>, CancerResearchSchedulerError> {
    let commitment = match &state.manifest().experiment {
        Some(WorldExperimentCommitment::CancerResearch(commitment)) => commitment,
        None => return Ok(None),
    };
    let configuration = state
        .configuration()
        .ok_or(CancerResearchSchedulerError::MissingWorldConfiguration)?;
    let tick_duration = u64::from(configuration.tick_duration_seconds);
    if tick_duration == 0 || !SECONDS_PER_DAY.is_multiple_of(tick_duration) {
        return Err(CancerResearchSchedulerError::InvalidTickDuration);
    }
    let ticks_per_day = SECONDS_PER_DAY / tick_duration;
    let tick = state.tick().get();
    let Some(turn_ordinal) = research_ordinal_due_at_tick(tick)? else {
        return Ok(None);
    };
    if let Some(existing) = store
        .load_existing_cancer_research_request(state.world_id(), turn_ordinal)
        .await?
    {
        if existing.selection.selected_at_tick != state.tick() {
            return Err(CancerResearchSchedulerError::InvalidExistingRequest);
        }
        return Ok(Some(existing.request_id));
    }
    let program = CancerResearchProgram::for_ordinal(turn_ordinal);
    let program_turn_ordinal = turn_ordinal / 2;
    let day_ordinal = u32::try_from(tick / ticks_per_day)
        .map_err(|_| CancerResearchSchedulerError::OrdinalOverflow)?;
    let affected_living_people = state
        .organisms()
        .filter(|organism| {
            organism.role() == OrganismRole::Person
                && organism.is_alive()
                && state.is_initial_cancer_research_resident(organism.organism_id())
        })
        .map(sim_engine::OrganismState::organism_id)
        .collect::<Vec<_>>();
    let unaffected_founders = state
        .organisms()
        .filter(|organism| {
            organism.role() == OrganismRole::Person
                && organism.is_founder()
                && !state.is_initial_cancer_research_resident(organism.organism_id())
        })
        .map(sim_engine::OrganismState::organism_id)
        .collect::<Vec<_>>();
    let living_unaffected = state
        .organisms()
        .filter(|organism| {
            organism.role() == OrganismRole::Person
                && organism.is_founder()
                && organism.is_alive()
                && !state.is_initial_cancer_research_resident(organism.organism_id())
        })
        .map(sim_engine::OrganismState::organism_id)
        .collect::<BTreeSet<_>>();
    let mut living_engineers =
        select_support_engineering_cohort(state.manifest().seed, &unaffected_founders)?
            .into_iter()
            .filter(|resident_id| living_unaffected.contains(resident_id))
            .collect::<Vec<_>>();
    living_engineers.sort_unstable();
    let engineering_turn = program_turn_ordinal % 3 == 2 && !living_engineers.is_empty();
    let candidates = if engineering_turn {
        &living_engineers
    } else {
        &affected_living_people
    };
    let Some(resident_id) = select_researcher(state.manifest().seed, turn_ordinal, candidates)?
    else {
        return Ok(None);
    };
    let deadline_tick = tick
        .checked_add(ticks_per_day)
        .map(SimTick::new)
        .ok_or(CancerResearchSchedulerError::TickOverflow)?;
    let mut evidence_documents = embedded_biological_primitives()?;
    if state.ruleset_version() >= sim_engine::CANCER_BIOLOGY_RULESET_VERSION {
        let living_burdens = affected_living_people
            .iter()
            .map(|affected_id| {
                state
                    .cancer_burden(*affected_id)
                    .cloned()
                    .map(|burden| (*affected_id, burden))
                    .ok_or(CancerResearchSchedulerError::MissingCancerBurden(
                        *affected_id,
                    ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        evidence_documents.extend(cancer_burden_observations(
            state.world_id(),
            day_ordinal,
            (!engineering_turn).then_some(resident_id),
            &living_burdens,
        )?);
        evidence_documents.sort_by(|left, right| left.reference.cmp(&right.reference));
    }
    let latest_hypothesis = if turn_ordinal > 0 {
        store
            .load_latest_cancer_research_hypothesis(state.world_id(), turn_ordinal, program)
            .await?
    } else {
        None
    };
    let campaign_turn = if program_turn_ordinal % CANCER_RESEARCH_CAMPAIGN_INTERVAL_PROGRAM_TURNS
        == CANCER_RESEARCH_CAMPAIGN_INTERVAL_PROGRAM_TURNS - 1
    {
        store
            .load_cancer_research_campaign_candidate(state.world_id(), turn_ordinal, program)
            .await?
            .map(prepare_campaign_turn)
            .transpose()?
            .flatten()
    } else {
        None
    };
    let (stage, task, inference_tier, frozen_candidate_hash, model_max_output_tokens) =
        if let Some(campaign) = campaign_turn {
            evidence_documents.extend(campaign.evidence_documents);
            (
                CancerResearchStage::IndependentReplication,
                campaign.task,
                campaign.inference_tier,
                Some(campaign.root_artifact_hash),
                campaign.model_max_output_tokens,
            )
        } else {
            (
                CancerResearchStage::BlindDiscovery,
                match program_turn_ordinal % 3 {
                    0 => CancerResearchTask::GenerateMechanisticHypothesis,
                    1 => CancerResearchTask::ProposeDiscriminatingExperiment,
                    _ => match program {
                        CancerResearchProgram::Devices => {
                            CancerResearchTask::DesignDiagnosticInstrument
                        }
                        CancerResearchProgram::Treatments => {
                            CancerResearchTask::DesignTreatmentMachine
                        }
                    },
                },
                CancerResearchInferenceTier::Exploration,
                None,
                BLIND_RESEARCH_MAX_OUTPUT_TOKENS,
            )
        };
    if let Some(challenge) =
        nci60_response_challenge_document_for_turn(state.world_id(), turn_ordinal, stage)?
    {
        evidence_documents.push(challenge);
    }
    evidence_documents.sort_by(|left, right| left.reference.cmp(&right.reference));
    let evidence = evidence_documents
        .iter()
        .map(|document| document.reference.clone())
        .collect();
    let selection = world_domain::CancerResearchTurnSelection::new(
        state.world_id(),
        resident_id,
        state.tick(),
        deadline_tick,
        turn_ordinal,
        commitment.target,
        stage,
        task,
        inference_tier,
        CancerResearchProfile::seeded(state.manifest().seed, resident_id)?,
        evidence,
        frozen_candidate_hash,
        model_max_output_tokens,
    )?;
    let recalled_memories = if stage == CancerResearchStage::BlindDiscovery {
        let mut catalog = store
            .load_cancer_research_catalog(
                state.world_id(),
                turn_ordinal,
                MAX_CANCER_RESEARCH_CATALOG_ENTRIES,
            )
            .await?;
        if let Some(prior) = &latest_hypothesis {
            catalog.push(CancerResearchMemoryInput::from_internal_catalog(
                prior.contribution(),
            )?);
        }
        catalog.sort_by_key(|memory| memory.document_id);
        catalog
    } else {
        Vec::new()
    };
    let request =
        CancerResearchModelRequest::new(selection, evidence_documents, recalled_memories)?;
    let request_id = request.request_id;
    store.enqueue_cancer_research_request(&request).await?;
    Ok(Some(request_id))
}

struct PreparedCampaignTurn {
    task: CancerResearchTask,
    inference_tier: CancerResearchInferenceTier,
    root_artifact_hash: Digest,
    model_max_output_tokens: u16,
    evidence_documents: Vec<CancerResearchEvidenceDocument>,
}

fn prepare_campaign_turn(
    candidate: CancerResearchCampaignCandidate,
) -> Result<Option<PreparedCampaignTurn>, CancerResearchSchedulerError> {
    candidate.root.validate()?;
    let root_contribution = candidate.root.contribution();
    candidate
        .root_experiment
        .validate_against(root_contribution)?;
    if candidate.root_experiment.interpretation
        != world_domain::CancerVirtualExperimentInterpretation::ModelSupportsPrediction
    {
        return Err(CancerResearchSchedulerError::InvalidCampaign);
    }
    let root_plan = root_contribution
        .virtual_experiment_plan
        .as_ref()
        .ok_or(CancerResearchSchedulerError::InvalidCampaign)?;
    let root_artifact_hash = root_contribution.canonical_hash()?;
    let campaign_id =
        CancerResearchCampaignDirective::campaign_id(candidate.root.request.request_id);
    let world_id = candidate.root.request.selection.world_id;
    let mut evidence_documents = vec![campaign_evidence_document(
        CancerResearchEvidenceKind::FrozenHypothesis,
        format!("cancer-world://{world_id}/campaign/{campaign_id}/root"),
        root_contribution,
    )?];
    evidence_documents.push(campaign_evidence_document(
        CancerResearchEvidenceKind::AssayObservation,
        format!("cancer-world://{world_id}/campaign/{campaign_id}/root-experiment"),
        &candidate.root_experiment,
    )?);

    let mut supporting_tests = 0_u8;
    let mut falsifying_tests = 0_u8;
    let mut inconclusive_tests = 0_u8;
    let mut prior_plan_hashes = vec![Digest::canonical(root_plan)?];
    let mut synthesis_complete = false;

    for followup in candidate.followups {
        followup.request.validate()?;
        if followup.request.selection.world_id != world_id
            || followup.request.selection.frozen_candidate_hash != Some(root_artifact_hash)
            || followup.request.selection.stage != CancerResearchStage::IndependentReplication
        {
            return Err(CancerResearchSchedulerError::InvalidCampaign);
        }
        if !followup.request_completed {
            return Ok(None);
        }
        let Some(result) = followup.result else {
            continue;
        };
        let registry = match followup.request.selection.inference_tier {
            CancerResearchInferenceTier::Exploration => {
                crate::CognitionRouteRegistry::cancer_research_exploration()
            }
            CancerResearchInferenceTier::Escalation => {
                crate::CognitionRouteRegistry::cancer_research_escalation()
            }
        };
        result.validate_against(&registry, &followup.request)?;
        let Some(receipt) = result.receipt else {
            continue;
        };
        let contribution = receipt.contribution;
        evidence_documents.push(campaign_evidence_document(
            CancerResearchEvidenceKind::PriorResearchArtifact,
            format!(
                "cancer-world://{world_id}/campaign/{campaign_id}/artifact/{}",
                contribution.contribution_id
            ),
            &contribution,
        )?);
        match followup.request.selection.task {
            CancerResearchTask::DesignIndependentReplication => {
                let experiment = followup.virtual_experiment.as_ref();
                let Some(experiment) = experiment else {
                    return Ok(None);
                };
                experiment.validate_against(&contribution)?;
                let plan = contribution
                    .virtual_experiment_plan
                    .as_ref()
                    .ok_or(CancerResearchSchedulerError::InvalidCampaign)?;
                prior_plan_hashes.push(Digest::canonical(plan)?);
                match cancer_research_campaign_test_assessment(experiment) {
                    CancerResearchCampaignTestAssessment::Supports => {
                        supporting_tests = supporting_tests.saturating_add(1);
                    }
                    CancerResearchCampaignTestAssessment::Falsifies => {
                        falsifying_tests = falsifying_tests.saturating_add(1);
                    }
                    CancerResearchCampaignTestAssessment::Inconclusive => {
                        inconclusive_tests = inconclusive_tests.saturating_add(1);
                    }
                }
                evidence_documents.push(campaign_evidence_document(
                    CancerResearchEvidenceKind::AssayObservation,
                    format!(
                        "cancer-world://{world_id}/campaign/{campaign_id}/experiment/{}",
                        experiment.experiment_id
                    ),
                    &experiment,
                )?);
            }
            CancerResearchTask::InterpretReplicationResult => {
                synthesis_complete = true;
            }
            _ => return Err(CancerResearchSchedulerError::InvalidCampaign),
        }
    }
    if synthesis_complete {
        return Ok(None);
    }

    let test_count = usize::from(supporting_tests)
        .saturating_add(usize::from(falsifying_tests))
        .saturating_add(usize::from(inconclusive_tests));
    let outcome = if falsifying_tests > 0 {
        Some(CancerResearchCampaignOutcome::Falsified)
    } else if usize::from(supporting_tests) >= CANCER_RESEARCH_CAMPAIGN_REQUIRED_SUPPORTS {
        Some(CancerResearchCampaignOutcome::SurvivedReplicationRound)
    } else if test_count >= CANCER_RESEARCH_CAMPAIGN_MAX_TESTS {
        Some(CancerResearchCampaignOutcome::Inconclusive)
    } else {
        None
    };

    let (task, inference_tier, directive) = if let Some(outcome) = outcome {
        (
            CancerResearchTask::InterpretReplicationResult,
            if outcome == CancerResearchCampaignOutcome::SurvivedReplicationRound {
                CancerResearchInferenceTier::Escalation
            } else {
                CancerResearchInferenceTier::Exploration
            },
            CancerResearchCampaignDirective::Synthesis {
                schema_version: CANCER_RESEARCH_CAMPAIGN_DIRECTIVE_SCHEMA_VERSION,
                campaign_id,
                root_artifact_hash,
                outcome,
                supporting_tests,
                falsifying_tests,
                inconclusive_tests,
            },
        )
    } else {
        prior_plan_hashes.sort_unstable();
        prior_plan_hashes.dedup();
        let test_index =
            u8::try_from(test_count).map_err(|_| CancerResearchSchedulerError::InvalidCampaign)?;
        let (variation, required_plan) =
            varied_campaign_plan(root_plan, test_index, &prior_plan_hashes)?;
        (
            CancerResearchTask::DesignIndependentReplication,
            CancerResearchInferenceTier::Exploration,
            CancerResearchCampaignDirective::AdversarialTest {
                schema_version: CANCER_RESEARCH_CAMPAIGN_DIRECTIVE_SCHEMA_VERSION,
                campaign_id,
                root_artifact_hash,
                test_index,
                variation,
                required_plan,
                prior_plan_hashes,
            },
        )
    };
    evidence_documents.push(directive.evidence_document(world_id)?);
    evidence_documents.sort_by(|left, right| left.reference.cmp(&right.reference));
    if evidence_documents
        .windows(2)
        .any(|pair| pair[0].reference >= pair[1].reference)
    {
        return Err(CancerResearchSchedulerError::InvalidCampaign);
    }
    Ok(Some(PreparedCampaignTurn {
        task,
        inference_tier,
        root_artifact_hash,
        model_max_output_tokens: BLIND_RESEARCH_MAX_OUTPUT_TOKENS,
        evidence_documents,
    }))
}

fn campaign_evidence_document(
    kind: CancerResearchEvidenceKind,
    source_id: String,
    value: &impl Serialize,
) -> Result<CancerResearchEvidenceDocument, CancerResearchSchedulerError> {
    let content = serde_json::to_string(value)?;
    Ok(CancerResearchEvidenceDocument {
        reference: CancerResearchEvidenceReference {
            kind,
            source_id,
            content_hash: Digest::sha256(content.as_bytes()),
        },
        content,
    })
}

fn varied_campaign_plan(
    root: &CancerVirtualExperimentPlan,
    test_index: u8,
    prior_plan_hashes: &[Digest],
) -> Result<
    (CancerResearchCampaignVariation, CancerVirtualExperimentPlan),
    CancerResearchSchedulerError,
> {
    let variation = match test_index {
        0 => CancerResearchCampaignVariation::SubjectModel,
        1 => CancerResearchCampaignVariation::Intensity,
        2 => CancerResearchCampaignVariation::Exposure,
        3 => CancerResearchCampaignVariation::EndpointOrTarget,
        4 => CancerResearchCampaignVariation::ModalityOrTarget,
        _ => return Err(CancerResearchSchedulerError::InvalidCampaign),
    };
    let mut plan = root.clone();
    match variation {
        CancerResearchCampaignVariation::SubjectModel => {
            plan.subject_model = rotate_subject_model(root.subject_model);
        }
        CancerResearchCampaignVariation::Intensity => {
            plan.intensity_parts_per_million = if root.intensity_parts_per_million <= 750_000 {
                root.intensity_parts_per_million.saturating_add(250_000)
            } else {
                root.intensity_parts_per_million
                    .saturating_sub(250_000)
                    .max(1)
            };
        }
        CancerResearchCampaignVariation::Exposure => {
            plan.exposure_hours = if root.exposure_hours <= 1_080 {
                root.exposure_hours.saturating_mul(2).min(2_160)
            } else {
                (root.exposure_hours / 2).max(1)
            };
        }
        CancerResearchCampaignVariation::EndpointOrTarget => {
            if root.intervention_modality == CancerVirtualInterventionModality::DiagnosticSensing {
                plan.primary_target = rotate_target(root.primary_target);
                if plan.secondary_target == Some(plan.primary_target) {
                    plan.secondary_target = None;
                }
            } else {
                plan.primary_endpoint = rotate_treatment_endpoint(root.primary_endpoint);
            }
        }
        CancerResearchCampaignVariation::ModalityOrTarget => {
            if root.intervention_modality == CancerVirtualInterventionModality::DiagnosticSensing {
                let next = root
                    .secondary_target
                    .map_or_else(|| rotate_target(root.primary_target), rotate_target);
                plan.secondary_target = (next != root.primary_target).then_some(next);
            } else {
                plan.intervention_modality = rotate_treatment_modality(root.intervention_modality);
            }
        }
    }
    plan.validate()?;
    let mut plan_hash = Digest::canonical(&plan)?;
    let mut cohort_adjustment = 0_u16;
    while prior_plan_hashes.contains(&plan_hash) {
        cohort_adjustment = cohort_adjustment.saturating_add(1);
        if cohort_adjustment > 16 {
            return Err(CancerResearchSchedulerError::InvalidCampaign);
        }
        plan.cohort_size = if root.cohort_size < 4_096 {
            root.cohort_size
                .saturating_add(cohort_adjustment)
                .min(4_096)
        } else {
            root.cohort_size.saturating_sub(cohort_adjustment).max(8)
        };
        plan_hash = Digest::canonical(&plan)?;
    }
    Ok((variation, plan))
}

const fn rotate_subject_model(model: CancerVirtualSubjectModel) -> CancerVirtualSubjectModel {
    match model {
        CancerVirtualSubjectModel::CellCulture => CancerVirtualSubjectModel::TumorOrganoid,
        CancerVirtualSubjectModel::TumorOrganoid => CancerVirtualSubjectModel::OrthotopicMouse,
        CancerVirtualSubjectModel::OrthotopicMouse => CancerVirtualSubjectModel::CellCulture,
    }
}

const fn rotate_treatment_endpoint(endpoint: CancerVirtualEndpoint) -> CancerVirtualEndpoint {
    match endpoint {
        CancerVirtualEndpoint::RelativeTumorBurden => CancerVirtualEndpoint::ViableTumorFraction,
        CancerVirtualEndpoint::ViableTumorFraction => CancerVirtualEndpoint::InvasiveCellFraction,
        CancerVirtualEndpoint::InvasiveCellFraction => CancerVirtualEndpoint::HypoxicCellFraction,
        CancerVirtualEndpoint::HypoxicCellFraction => {
            CancerVirtualEndpoint::OffTargetHealthyCellLoss
        }
        CancerVirtualEndpoint::OffTargetHealthyCellLoss
        | CancerVirtualEndpoint::DetectionSensitivity => CancerVirtualEndpoint::RelativeTumorBurden,
    }
}

const fn rotate_treatment_modality(
    modality: CancerVirtualInterventionModality,
) -> CancerVirtualInterventionModality {
    match modality {
        CancerVirtualInterventionModality::MolecularInhibition => {
            CancerVirtualInterventionModality::Radiation
        }
        CancerVirtualInterventionModality::Radiation => CancerVirtualInterventionModality::Thermal,
        CancerVirtualInterventionModality::Thermal => {
            CancerVirtualInterventionModality::ElectricField
        }
        CancerVirtualInterventionModality::ElectricField => {
            CancerVirtualInterventionModality::TargetedDelivery
        }
        CancerVirtualInterventionModality::TargetedDelivery => {
            CancerVirtualInterventionModality::SurgicalResection
        }
        CancerVirtualInterventionModality::SurgicalResection
        | CancerVirtualInterventionModality::DiagnosticSensing => {
            CancerVirtualInterventionModality::MolecularInhibition
        }
    }
}

const fn rotate_target(
    target: world_domain::CancerVirtualMechanismTarget,
) -> world_domain::CancerVirtualMechanismTarget {
    use world_domain::CancerVirtualMechanismTarget as Target;
    match target {
        Target::CellDivision => Target::DnaRepair,
        Target::DnaRepair => Target::ApoptosisResistance,
        Target::ApoptosisResistance => Target::HypoxiaAdaptation,
        Target::HypoxiaAdaptation => Target::Angiogenesis,
        Target::Angiogenesis => Target::ImmuneEvasion,
        Target::ImmuneEvasion => Target::Invasion,
        Target::Invasion => Target::CellDivision,
    }
}

fn research_ordinal_due_at_tick(tick: u64) -> Result<Option<u32>, CancerResearchSchedulerError> {
    if tick == 0 {
        return Ok(None);
    }
    let scheduled_through_tick = tick
        .checked_mul(CANCER_RESEARCH_TURNS_PER_TEN_TICKS)
        .ok_or(CancerResearchSchedulerError::OrdinalOverflow)?
        / CANCER_RESEARCH_SCHEDULE_TICK_SPAN;
    let scheduled_before_tick = (tick - 1)
        .checked_mul(CANCER_RESEARCH_TURNS_PER_TEN_TICKS)
        .ok_or(CancerResearchSchedulerError::OrdinalOverflow)?
        / CANCER_RESEARCH_SCHEDULE_TICK_SPAN;
    if scheduled_through_tick == scheduled_before_tick {
        return Ok(None);
    }
    u32::try_from(scheduled_through_tick)
        .map(Some)
        .map_err(|_| CancerResearchSchedulerError::OrdinalOverflow)
}

/// Selects exactly one third of the unaffected founder cohort, rounding up, by
/// a seed-bound rank. Membership does not drift when residents die or children
/// are born, so the current live world can adopt this projection safely.
fn select_support_engineering_cohort(
    seed: world_domain::WorldSeed,
    candidates: &[EntityId],
) -> Result<Vec<EntityId>, CancerResearchSchedulerError> {
    if candidates.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(CancerResearchSchedulerError::NonCanonicalCandidates);
    }
    let mut ranked = candidates
        .iter()
        .copied()
        .map(|resident_id| {
            Digest::canonical(&(
                "a-tiny-civilization:cancer-support-engineering-cohort:v1",
                seed,
                resident_id,
            ))
            .map(|rank| (rank, resident_id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ranked.sort_unstable();
    let cohort_size = candidates.len().div_ceil(3);
    Ok(ranked
        .into_iter()
        .take(cohort_size)
        .map(|(_, resident_id)| resident_id)
        .collect())
}

fn select_researcher(
    seed: world_domain::WorldSeed,
    day_ordinal: u32,
    candidates: &[EntityId],
) -> Result<Option<EntityId>, CancerResearchSchedulerError> {
    if candidates.is_empty() {
        return Ok(None);
    }
    if candidates.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(CancerResearchSchedulerError::NonCanonicalCandidates);
    }
    let digest = Digest::canonical(&(
        "a-tiny-civilization:cancer-researcher-selection:v1",
        seed,
        day_ordinal,
    ))?;
    let rank = u64::from_be_bytes(
        digest.as_bytes()[..8]
            .try_into()
            .expect("SHA-256 digest contains at least eight bytes"),
    );
    let index = usize::try_from(rank % candidates.len() as u64)
        .expect("modulo candidate length fits usize");
    Ok(Some(candidates[index]))
}

fn embedded_biological_primitives()
-> Result<Vec<CancerResearchEvidenceDocument>, CancerResearchSchedulerError> {
    let bundle: PrimitiveBundle = serde_json::from_str(EMBEDDED_PRIMITIVES)?;
    if bundle.schema_version != CANCER_RESEARCH_SCHEDULER_VERSION
        || bundle.bundle_id != "atc-cancer-biological-primitives-v1"
        || bundle.records.is_empty()
    {
        return Err(CancerResearchSchedulerError::InvalidPrimitiveBundle);
    }
    let mut documents = bundle
        .records
        .into_iter()
        .map(|record| CancerResearchEvidenceDocument {
            reference: CancerResearchEvidenceReference {
                kind: CancerResearchEvidenceKind::BiologicalPrimitive,
                source_id: record.source_id,
                content_hash: Digest::sha256(record.content.as_bytes()),
            },
            content: record.content,
        })
        .collect::<Vec<_>>();
    documents.sort_by(|left, right| left.reference.cmp(&right.reference));
    if documents
        .windows(2)
        .any(|pair| pair[0].reference >= pair[1].reference)
    {
        return Err(CancerResearchSchedulerError::InvalidPrimitiveBundle);
    }
    Ok(documents)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Nci60ChallengeClass {
    SingleAgent,
    Combination,
}

impl Nci60ChallengeClass {
    const fn selection_domain(self) -> &'static str {
        match self {
            Self::SingleAgent => "single-agent",
            Self::Combination => "combination",
        }
    }
}

/// Returns a challenge only for an ordinary treatment-discovery turn. Device
/// work and campaign turns must remain independent of the held-out benchmark.
fn nci60_response_challenge_document_for_turn(
    world_id: world_domain::WorldId,
    turn_ordinal: u32,
    stage: CancerResearchStage,
) -> Result<Option<CancerResearchEvidenceDocument>, CancerResearchSchedulerError> {
    if CancerResearchProgram::for_ordinal(turn_ordinal) != CancerResearchProgram::Treatments
        || stage != CancerResearchStage::BlindDiscovery
    {
        return Ok(None);
    }
    let catalogue = embedded_nci60_challenge_catalogue()?;
    nci60_response_challenge_document(world_id, turn_ordinal, &catalogue).map(Some)
}

fn embedded_nci60_challenge_catalogue()
-> Result<Nci60ChallengeCatalogue, CancerResearchSchedulerError> {
    let catalogue: Nci60ChallengeCatalogue =
        serde_json::from_str(EMBEDDED_NCI60_CHALLENGE_CATALOGUE)?;
    if catalogue.schema_version != 1
        || catalogue.catalogue_id != "nci-cellminer-2-15-cns-challenge-catalogue-v1"
        || catalogue.evidence_class != "in_vitro_immortalized_cell_line_response_challenge_metadata"
        || catalogue.intended_use.trim().is_empty()
        || catalogue.source_registry_hash == Digest::ZERO
        || !catalogue.source.is_object()
        || !catalogue.single_agent_partition.is_object()
        || !catalogue.combination_partition.is_object()
        || catalogue.cns_cell_lines.len() != 6
        || catalogue.single_agent_candidates.is_empty()
        || catalogue.combination_candidates.is_empty()
        || catalogue.leakage_boundary.access_class != "prompt_safe_candidate_metadata"
        || !catalogue.leakage_boundary.allowed_in_model_context
        || catalogue.leakage_boundary.contains_observed_response_values
        || catalogue.leakage_boundary.contains_derived_rank_labels
        || catalogue.limitations.len() < 3
    {
        return Err(CancerResearchSchedulerError::InvalidResponseChallengeCatalogue);
    }
    Ok(catalogue)
}

/// Selects one prompt-safe, label-free held-out challenge from the embedded
/// catalogue. The treatment-program ordinal fixes the class cadence: one
/// single-agent challenge followed by three combination challenges. Within
/// each class, a world-bound affine permutation visits every candidate exactly
/// once before the cycle repeats. No observed value or answer-key commitment
/// enters this process.
fn nci60_response_challenge_document(
    world_id: world_domain::WorldId,
    turn_ordinal: u32,
    catalogue: &Nci60ChallengeCatalogue,
) -> Result<CancerResearchEvidenceDocument, CancerResearchSchedulerError> {
    let (challenge_class, class_ordinal) = nci60_challenge_slot(turn_ordinal)
        .ok_or(CancerResearchSchedulerError::InvalidResponseChallengeTurn)?;
    let challenge_id = Uuid::new_v5(
        &world_id.as_uuid(),
        format!("nci60-response-challenge:v1:{turn_ordinal}").as_bytes(),
    );
    let (source_candidate_id, source_id, content) =
        if challenge_class == Nci60ChallengeClass::SingleAgent {
            let index = nci60_permutation_index(
                world_id,
                challenge_class,
                class_ordinal,
                catalogue.single_agent_candidates.len(),
            )?;
            let candidate = &catalogue.single_agent_candidates[index];
            if candidate.compound.nsc == 0 || candidate.challenge_id.trim().is_empty() {
                return Err(CancerResearchSchedulerError::InvalidResponseChallengeCatalogue);
            }
            let content = serde_json::to_string(&Nci60PromptSafeChallenge {
                schema_version: 1,
                catalogue_id: &catalogue.catalogue_id,
                source_candidate_id: &candidate.challenge_id,
                evidence_class: &catalogue.evidence_class,
                intended_use: &catalogue.intended_use,
                cns_cell_lines: &catalogue.cns_cell_lines,
                single_agent: Some(candidate),
                combination: None,
                limitations: &catalogue.limitations,
            })?;
            (
                candidate.challenge_id.as_str(),
                format!(
                    "cancer-world://nci60-response-challenge/{challenge_id}/single-agent/{}",
                    candidate.compound.nsc
                ),
                content,
            )
        } else {
            let index = nci60_permutation_index(
                world_id,
                challenge_class,
                class_ordinal,
                catalogue.combination_candidates.len(),
            )?;
            let candidate = &catalogue.combination_candidates[index];
            if candidate.first.nsc == 0
                || candidate.first.nsc >= candidate.second.nsc
                || candidate.challenge_id.trim().is_empty()
            {
                return Err(CancerResearchSchedulerError::InvalidResponseChallengeCatalogue);
            }
            let content = serde_json::to_string(&Nci60PromptSafeChallenge {
                schema_version: 1,
                catalogue_id: &catalogue.catalogue_id,
                source_candidate_id: &candidate.challenge_id,
                evidence_class: &catalogue.evidence_class,
                intended_use: &catalogue.intended_use,
                cns_cell_lines: &catalogue.cns_cell_lines,
                single_agent: None,
                combination: Some(candidate),
                limitations: &catalogue.limitations,
            })?;
            (
                candidate.challenge_id.as_str(),
                format!(
                    "cancer-world://nci60-response-challenge/{challenge_id}/combination/{}-{}",
                    candidate.first.nsc, candidate.second.nsc
                ),
                content,
            )
        };
    if content.contains("response_rank")
        || content.contains("activity_z_milli")
        || content.contains("combo_score_milli")
        || content.contains("answer_payload")
    {
        return Err(CancerResearchSchedulerError::InvalidResponseChallengeCatalogue);
    }
    debug_assert!(!source_candidate_id.is_empty());
    Ok(CancerResearchEvidenceDocument {
        reference: CancerResearchEvidenceReference {
            kind: CancerResearchEvidenceKind::ResponseChallenge,
            source_id,
            content_hash: Digest::sha256(content.as_bytes()),
        },
        content,
    })
}

/// Maps the global scheduler ordinal to a class-local challenge ordinal. Real
/// treatment turns are the odd global ordinals, so class selection must be
/// based on this program-local ordinal rather than the global parity/cadence.
fn nci60_challenge_slot(turn_ordinal: u32) -> Option<(Nci60ChallengeClass, u32)> {
    if CancerResearchProgram::for_ordinal(turn_ordinal) != CancerResearchProgram::Treatments {
        return None;
    }
    let treatment_turn_ordinal = turn_ordinal / 2;
    if treatment_turn_ordinal.is_multiple_of(4) {
        Some((Nci60ChallengeClass::SingleAgent, treatment_turn_ordinal / 4))
    } else {
        Some((
            Nci60ChallengeClass::Combination,
            treatment_turn_ordinal - treatment_turn_ordinal / 4 - 1,
        ))
    }
}

fn nci60_permutation_index(
    world_id: world_domain::WorldId,
    challenge_class: Nci60ChallengeClass,
    class_ordinal: u32,
    candidate_count: usize,
) -> Result<usize, CancerResearchSchedulerError> {
    let modulus = u64::try_from(candidate_count)
        .map_err(|_| CancerResearchSchedulerError::InvalidResponseChallengeCatalogue)?;
    if modulus == 0 {
        return Err(CancerResearchSchedulerError::InvalidResponseChallengeCatalogue);
    }
    let entropy = Digest::canonical(&(
        "a-tiny-civilization:nci60-response-challenge-permutation:v1",
        world_id,
        challenge_class.selection_domain(),
    ))?;
    let offset = u64::from_be_bytes(
        entropy.as_bytes()[..8]
            .try_into()
            .expect("SHA-256 digest contains at least eight bytes"),
    ) % modulus;
    let mut step = u64::from_be_bytes(
        entropy.as_bytes()[8..16]
            .try_into()
            .expect("SHA-256 digest contains at least sixteen bytes"),
    ) % modulus;
    if step == 0 {
        step = 1;
    }
    while greatest_common_divisor(step, modulus) != 1 {
        step = (step + 1) % modulus;
        if step == 0 {
            step = 1;
        }
    }
    let phase = u64::from(class_ordinal) % modulus;
    let index = (u128::from(offset) + u128::from(phase) * u128::from(step)) % u128::from(modulus);
    usize::try_from(index)
        .map_err(|_| CancerResearchSchedulerError::InvalidResponseChallengeCatalogue)
}

const fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[derive(Serialize)]
struct ResidentBurdenObservation<'a> {
    observation_schema_version: u16,
    day_ordinal: u32,
    resident_id: EntityId,
    burden: &'a CancerBurdenState,
}

#[derive(Serialize)]
struct CohortBurdenObservation {
    observation_schema_version: u16,
    day_ordinal: u32,
    living_affected_count: u32,
    primary_burden_min_parts_per_million: u32,
    primary_burden_median_parts_per_million: u32,
    primary_burden_max_parts_per_million: u32,
    metastatic_positive_count: u32,
    metastatic_burden_max_parts_per_million: u32,
    clone_diversity_min_units: u16,
    clone_diversity_median_units: u16,
    clone_diversity_max_units: u16,
    growing_count: u32,
    stable_count: u32,
    shrinking_count: u32,
    spreading_count: u32,
    recurring_count: u32,
}

fn cancer_burden_observations(
    world_id: world_domain::WorldId,
    day_ordinal: u32,
    selected_affected_resident_id: Option<EntityId>,
    burdens: &[(EntityId, CancerBurdenState)],
) -> Result<Vec<CancerResearchEvidenceDocument>, CancerResearchSchedulerError> {
    if burdens.is_empty() {
        return Err(CancerResearchSchedulerError::EmptyCancerBurdenCohort);
    }
    let mut primary = burdens
        .iter()
        .map(|(_, burden)| burden.primary_burden_parts_per_million)
        .collect::<Vec<_>>();
    let mut clone_diversity = burdens
        .iter()
        .map(|(_, burden)| burden.clone_diversity_units)
        .collect::<Vec<_>>();
    primary.sort_unstable();
    clone_diversity.sort_unstable();
    let count =
        u32::try_from(burdens.len()).map_err(|_| CancerResearchSchedulerError::OrdinalOverflow)?;
    let cohort = CohortBurdenObservation {
        observation_schema_version: 1,
        day_ordinal,
        living_affected_count: count,
        primary_burden_min_parts_per_million: primary[0],
        primary_burden_median_parts_per_million: primary[primary.len() / 2],
        primary_burden_max_parts_per_million: *primary
            .last()
            .expect("nonempty burden cohort has a maximum"),
        metastatic_positive_count: u32::try_from(
            burdens
                .iter()
                .filter(|(_, burden)| burden.metastatic_burden_parts_per_million > 0)
                .count(),
        )
        .map_err(|_| CancerResearchSchedulerError::OrdinalOverflow)?,
        metastatic_burden_max_parts_per_million: burdens
            .iter()
            .map(|(_, burden)| burden.metastatic_burden_parts_per_million)
            .max()
            .expect("nonempty burden cohort has a metastatic maximum"),
        clone_diversity_min_units: clone_diversity[0],
        clone_diversity_median_units: clone_diversity[clone_diversity.len() / 2],
        clone_diversity_max_units: *clone_diversity
            .last()
            .expect("nonempty burden cohort has a diversity maximum"),
        growing_count: trajectory_count(burdens, CancerTrajectory::Growing)?,
        stable_count: trajectory_count(burdens, CancerTrajectory::Stable)?,
        shrinking_count: trajectory_count(burdens, CancerTrajectory::Shrinking)?,
        spreading_count: trajectory_count(burdens, CancerTrajectory::Spreading)?,
        recurring_count: trajectory_count(burdens, CancerTrajectory::Recurring)?,
    };
    let cohort_content = serde_json::to_string(&cohort)?;
    let mut documents = vec![observation_document(
        format!("cancer-world://{world_id}/day/{day_ordinal}/cohort/burden-summary"),
        cohort_content,
    )];
    if let Some(selected_resident_id) = selected_affected_resident_id {
        let selected = burdens
            .iter()
            .find(|(resident_id, _)| *resident_id == selected_resident_id)
            .ok_or(CancerResearchSchedulerError::MissingCancerBurden(
                selected_resident_id,
            ))?;
        let resident_content = serde_json::to_string(&ResidentBurdenObservation {
            observation_schema_version: 1,
            day_ordinal,
            resident_id: selected_resident_id,
            burden: &selected.1,
        })?;
        documents.push(observation_document(
            format!(
                "cancer-world://{world_id}/day/{day_ordinal}/resident/{selected_resident_id}/burden"
            ),
            resident_content,
        ));
    }
    Ok(documents)
}

fn trajectory_count(
    burdens: &[(EntityId, CancerBurdenState)],
    trajectory: CancerTrajectory,
) -> Result<u32, CancerResearchSchedulerError> {
    u32::try_from(
        burdens
            .iter()
            .filter(|(_, burden)| burden.trajectory == trajectory)
            .count(),
    )
    .map_err(|_| CancerResearchSchedulerError::OrdinalOverflow)
}

fn observation_document(source_id: String, content: String) -> CancerResearchEvidenceDocument {
    CancerResearchEvidenceDocument {
        reference: CancerResearchEvidenceReference {
            kind: CancerResearchEvidenceKind::AssayObservation,
            source_id,
            content_hash: Digest::sha256(content.as_bytes()),
        },
        content,
    }
}

#[derive(Debug, Error)]
pub enum CancerResearchSchedulerError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Research(#[from] world_domain::CancerResearchContractError),
    #[error(transparent)]
    ModelContract(#[from] crate::CancerResearchModelContractError),
    #[error("Cancer World is missing its committed world configuration")]
    MissingWorldConfiguration,
    #[error("Cancer World tick duration does not divide one simulated day")]
    InvalidTickDuration,
    #[error("Cancer World research day ordinal exceeded u32")]
    OrdinalOverflow,
    #[error("Cancer World research deadline overflowed simulation time")]
    TickOverflow,
    #[error("Cancer World researcher candidates are not uniquely ordered")]
    NonCanonicalCandidates,
    #[error("Cancer World resident {0} is missing canonical cancer-burden state")]
    MissingCancerBurden(EntityId),
    #[error("Cancer World has no living affected burden observations")]
    EmptyCancerBurdenCohort,
    #[error("embedded Cancer World biological primitives are invalid")]
    InvalidPrimitiveBundle,
    #[error("embedded NCI-60 response challenge catalogue is invalid or leaks labels")]
    InvalidResponseChallengeCatalogue,
    #[error("NCI-60 response challenges are only valid on treatment-program turns")]
    InvalidResponseChallengeTurn,
    #[error("Cancer World research campaign lineage is invalid")]
    InvalidCampaign,
    #[error("existing Cancer World research request does not match its deterministic tick")]
    InvalidExistingRequest,
    #[error("embedded Cancer World biological primitives could not be decoded: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("Cancer World research scheduler hashing failed: {0}")]
    Hash(#[from] world_domain::CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION, WorldId, WorldSeed};

    #[test]
    fn embedded_primitives_are_content_addressed_sorted_and_treatment_free() {
        let documents = embedded_biological_primitives().expect("embedded primitives");
        assert_eq!(documents.len(), 8);
        assert!(
            documents
                .windows(2)
                .all(|pair| pair[0].reference < pair[1].reference)
        );
        for document in documents {
            assert_eq!(
                document.reference.content_hash,
                Digest::sha256(document.content.as_bytes())
            );
            let lowercase = document.content.to_ascii_lowercase();
            assert!(!lowercase.contains("dosage"));
            assert!(!lowercase.contains("treatment protocol"));
        }
    }

    #[test]
    fn response_challenges_are_deterministic_complete_and_label_free() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x6000));
        let catalogue = embedded_nci60_challenge_catalogue().expect("challenge catalogue");
        let single =
            nci60_response_challenge_document(world_id, 1, &catalogue).expect("single challenge");
        let repeated =
            nci60_response_challenge_document(world_id, 1, &catalogue).expect("repeated challenge");
        let combination = nci60_response_challenge_document(world_id, 3, &catalogue)
            .expect("combination challenge");
        assert_eq!(single, repeated);
        assert_eq!(
            single.reference.kind,
            CancerResearchEvidenceKind::ResponseChallenge
        );
        assert!(single.reference.source_id.contains("/single-agent/"));
        assert!(combination.reference.source_id.contains("/combination/"));
        for challenge in [single, combination] {
            assert_eq!(
                challenge.reference.content_hash,
                Digest::sha256(challenge.content.as_bytes())
            );
            let value: serde_json::Value =
                serde_json::from_str(&challenge.content).expect("challenge JSON");
            assert_eq!(value["cns_cell_lines"].as_array().map(Vec::len), Some(6));
            let encoded = challenge.content.to_ascii_lowercase();
            assert!(!encoded.contains("response_rank"));
            assert!(!encoded.contains("activity_z_milli"));
            assert!(!encoded.contains("combo_score_milli"));
            assert!(!encoded.contains("answer_payload"));
        }
    }

    #[test]
    fn response_challenge_cycle_uses_both_classes_on_actual_treatment_ordinals() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x6001));
        let catalogue = embedded_nci60_challenge_catalogue().expect("challenge catalogue");
        let mut single_ids = BTreeSet::new();
        let mut combination_ids = BTreeSet::new();

        for treatment_turn_ordinal in 0..512_u32 {
            let turn_ordinal = treatment_turn_ordinal * 2 + 1;
            let first = nci60_response_challenge_document(world_id, turn_ordinal, &catalogue)
                .expect("challenge");
            let repeated = nci60_response_challenge_document(world_id, turn_ordinal, &catalogue)
                .expect("repeated challenge");
            assert_eq!(first, repeated);

            let content: serde_json::Value =
                serde_json::from_str(&first.content).expect("challenge JSON");
            let source_candidate_id = content["source_candidate_id"]
                .as_str()
                .expect("source candidate ID")
                .to_owned();
            if first.reference.source_id.contains("/single-agent/") {
                assert!(single_ids.insert(source_candidate_id));
            } else {
                assert!(first.reference.source_id.contains("/combination/"));
                assert!(combination_ids.insert(source_candidate_id));
            }
        }

        assert_eq!(single_ids.len(), 128);
        assert_eq!(combination_ids.len(), 384);
    }

    #[test]
    fn response_challenge_permutations_cover_each_catalogue_class_before_reuse() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x6002));
        let catalogue = embedded_nci60_challenge_catalogue().expect("challenge catalogue");
        for (class, candidate_count) in [
            (
                Nci60ChallengeClass::SingleAgent,
                catalogue.single_agent_candidates.len(),
            ),
            (
                Nci60ChallengeClass::Combination,
                catalogue.combination_candidates.len(),
            ),
        ] {
            let indices = (0..u32::try_from(candidate_count).expect("bounded catalogue"))
                .map(|ordinal| {
                    nci60_permutation_index(world_id, class, ordinal, candidate_count)
                        .expect("permutation index")
                })
                .collect::<BTreeSet<_>>();
            assert_eq!(indices.len(), candidate_count);
            assert_eq!(indices.first(), Some(&0));
            assert_eq!(indices.last(), Some(&(candidate_count - 1)));
            assert_eq!(
                nci60_permutation_index(
                    world_id,
                    class,
                    u32::try_from(candidate_count).expect("bounded catalogue"),
                    candidate_count,
                )
                .expect("first repeated cycle index"),
                nci60_permutation_index(world_id, class, 0, candidate_count)
                    .expect("first permutation index")
            );
        }
    }

    #[test]
    fn response_challenges_skip_device_and_campaign_turns() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x6003));
        assert!(
            nci60_response_challenge_document_for_turn(
                world_id,
                2,
                CancerResearchStage::BlindDiscovery,
            )
            .expect("device turn")
            .is_none()
        );
        assert!(
            nci60_response_challenge_document_for_turn(
                world_id,
                1,
                CancerResearchStage::IndependentReplication,
            )
            .expect("campaign turn")
            .is_none()
        );
        assert!(
            nci60_response_challenge_document_for_turn(
                world_id,
                1,
                CancerResearchStage::BlindDiscovery,
            )
            .expect("treatment discovery turn")
            .is_some()
        );
    }

    #[test]
    fn researcher_selection_is_stable_and_rejects_reordered_candidates() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(37));
        let candidates = (0..8)
            .map(|ordinal| EntityId::deterministic(world_id, format!("r-{ordinal}").as_bytes()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let first = select_researcher(WorldSeed::new(37), 4, &candidates)
            .expect("selection")
            .expect("candidate");
        let repeated = select_researcher(WorldSeed::new(37), 4, &candidates)
            .expect("selection")
            .expect("candidate");
        assert_eq!(first, repeated);

        let mut reversed = candidates;
        reversed.reverse();
        assert!(matches!(
            select_researcher(WorldSeed::new(37), 4, &reversed),
            Err(CancerResearchSchedulerError::NonCanonicalCandidates)
        ));
    }

    #[test]
    fn accelerated_schedule_emits_1008_balanced_turns_per_1440_ticks() {
        let turns = (1..=1_440_u64)
            .filter_map(|tick| research_ordinal_due_at_tick(tick).expect("bounded schedule"))
            .collect::<Vec<_>>();
        assert_eq!(turns.len(), 1_008);
        assert_eq!(turns.first(), Some(&1));
        assert_eq!(turns.last(), Some(&1_008));
        assert_eq!(
            turns
                .iter()
                .filter(|ordinal| {
                    CancerResearchProgram::for_ordinal(**ordinal) == CancerResearchProgram::Devices
                })
                .count(),
            504
        );
        assert_eq!(
            turns
                .iter()
                .filter(|ordinal| {
                    CancerResearchProgram::for_ordinal(**ordinal)
                        == CancerResearchProgram::Treatments
                })
                .count(),
            504
        );
    }

    #[test]
    fn support_engineering_cohort_is_stable_and_exactly_one_third_rounded_up() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x500));
        let candidates = (0..500)
            .map(|ordinal| {
                EntityId::deterministic(world_id, format!("unaffected-{ordinal:03}").as_bytes())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let first = select_support_engineering_cohort(WorldSeed::new(37), &candidates)
            .expect("engineering cohort");
        let repeated = select_support_engineering_cohort(WorldSeed::new(37), &candidates)
            .expect("repeated engineering cohort");
        assert_eq!(first.len(), 167);
        assert_eq!(first, repeated);
        assert_eq!(first.iter().copied().collect::<BTreeSet<_>>().len(), 167);
    }

    #[test]
    fn campaign_variations_are_distinct_preregistered_plans() {
        let root = CancerVirtualExperimentPlan {
            schema_version: CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION,
            subject_model: CancerVirtualSubjectModel::CellCulture,
            intervention_modality: CancerVirtualInterventionModality::MolecularInhibition,
            primary_target: world_domain::CancerVirtualMechanismTarget::DnaRepair,
            secondary_target: None,
            primary_endpoint: CancerVirtualEndpoint::RelativeTumorBurden,
            intensity_parts_per_million: 400_000,
            exposure_hours: 72,
            cohort_size: 64,
        };
        let mut prior_hashes = vec![Digest::canonical(&root).expect("root hash")];
        let mut variations = Vec::new();
        for test_index in 0..u8::try_from(CANCER_RESEARCH_CAMPAIGN_MAX_TESTS).expect("test count") {
            prior_hashes.sort_unstable();
            let (variation, plan) =
                varied_campaign_plan(&root, test_index, &prior_hashes).expect("varied plan");
            plan.validate().expect("valid campaign plan");
            let plan_hash = Digest::canonical(&plan).expect("plan hash");
            assert!(!prior_hashes.contains(&plan_hash));
            prior_hashes.push(plan_hash);
            assert!(!variations.contains(&variation));
            variations.push(variation);
        }
        assert_eq!(variations.len(), CANCER_RESEARCH_CAMPAIGN_MAX_TESTS);

        let diagnostic = CancerVirtualExperimentPlan {
            intervention_modality: CancerVirtualInterventionModality::DiagnosticSensing,
            primary_endpoint: CancerVirtualEndpoint::DetectionSensitivity,
            ..root
        };
        let mut diagnostic_hashes = vec![Digest::canonical(&diagnostic).expect("diagnostic hash")];
        for test_index in 0..u8::try_from(CANCER_RESEARCH_CAMPAIGN_MAX_TESTS).expect("test count") {
            diagnostic_hashes.sort_unstable();
            let (_, plan) = varied_campaign_plan(&diagnostic, test_index, &diagnostic_hashes)
                .expect("diagnostic varied plan");
            assert_eq!(
                plan.intervention_modality,
                CancerVirtualInterventionModality::DiagnosticSensing
            );
            assert_eq!(
                plan.primary_endpoint,
                CancerVirtualEndpoint::DetectionSensitivity
            );
            diagnostic_hashes.push(Digest::canonical(&plan).expect("diagnostic plan hash"));
        }
    }

    #[test]
    fn burden_observations_are_bounded_content_addressed_measurements() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(38));
        let burdens = (0..3_u8)
            .map(|ordinal| {
                let resident_id = EntityId::deterministic(world_id, &[b'b', b'-', b'0' + ordinal]);
                CancerBurdenState::seeded_initial(
                    WorldSeed::new(38),
                    resident_id,
                    world_domain::CancerResearchTarget::AdultGlioblastoma,
                )
                .map(|burden| (resident_id, burden))
                .expect("seeded burden")
            })
            .collect::<Vec<_>>();
        let documents = cancer_burden_observations(world_id, 0, Some(burdens[1].0), &burdens)
            .expect("burden observations");
        assert_eq!(documents.len(), 2);
        assert!(documents.iter().all(|document| {
            document.reference.kind == CancerResearchEvidenceKind::AssayObservation
                && document.reference.content_hash == Digest::sha256(document.content.as_bytes())
                && !document.content.to_ascii_lowercase().contains("treatment")
        }));
        let cohort = documents
            .iter()
            .find(|document| document.reference.source_id.ends_with("burden-summary"))
            .expect("cohort observation");
        let value: serde_json::Value = serde_json::from_str(&cohort.content).expect("valid JSON");
        assert_eq!(value["living_affected_count"], 3);
        assert_eq!(value["growing_count"], 3);

        let engineering_documents = cancer_burden_observations(world_id, 0, None, &burdens)
            .expect("unaffected engineer receives cohort observations");
        assert_eq!(engineering_documents.len(), 1);
        assert!(
            engineering_documents[0]
                .reference
                .source_id
                .ends_with("burden-summary")
        );
    }
}
