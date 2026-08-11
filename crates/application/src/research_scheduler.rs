use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CancerBurdenState, CancerResearchEvidenceKind, CancerResearchEvidenceReference,
    CancerResearchInferenceTier, CancerResearchProfile, CancerResearchStage, CancerResearchTask,
    CancerTrajectory, Digest, EntityId, OrganismRole, SimTick, WorldExperimentCommitment,
};

use crate::{
    CancerResearchEvidenceDocument, CancerResearchJobStore, CancerResearchMemoryInput,
    CancerResearchModelRequest, MAX_CANCER_RESEARCH_CATALOG_ENTRIES,
    MAX_CANCER_RESEARCH_LITERATURE_DOCUMENTS, StoreError,
};

pub const CANCER_RESEARCH_SCHEDULER_VERSION: u16 = 1;
pub const CANCER_RESEARCH_BLIND_TURNS_PER_SIM_DAY: u32 = 12;
pub const CANCER_RESEARCH_ESCALATION_INTERVAL_DAYS: u32 = 7;
const SECONDS_PER_DAY: u64 = 86_400;
const BLIND_RESEARCH_MAX_OUTPUT_TOKENS: u16 = 4_096;
const EMBEDDED_PRIMITIVES: &str =
    include_str!("../../../data/cancer-research/biological-primitives-v1.json");

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
    let turns_per_day = u64::from(CANCER_RESEARCH_BLIND_TURNS_PER_SIM_DAY);
    if !ticks_per_day.is_multiple_of(turns_per_day) {
        return Err(CancerResearchSchedulerError::InvalidTickDuration);
    }
    let ticks_per_turn = ticks_per_day / turns_per_day;
    let tick = state.tick().get();
    if !tick.is_multiple_of(ticks_per_turn) {
        return Ok(None);
    }
    let turn_ordinal = u32::try_from(tick / ticks_per_turn)
        .map_err(|_| CancerResearchSchedulerError::OrdinalOverflow)?;
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
    let engineering_turn = turn_ordinal % 3 == 2 && !living_engineers.is_empty();
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
            resident_id,
            &living_burdens,
        )?);
        evidence_documents.sort_by(|left, right| left.reference.cmp(&right.reference));
    }
    let escalation_interval_turns = CANCER_RESEARCH_ESCALATION_INTERVAL_DAYS
        .checked_mul(CANCER_RESEARCH_BLIND_TURNS_PER_SIM_DAY)
        .ok_or(CancerResearchSchedulerError::OrdinalOverflow)?;
    let latest_hypothesis = if turn_ordinal > 0 {
        store
            .load_latest_cancer_research_hypothesis(state.world_id(), turn_ordinal)
            .await?
    } else {
        None
    };
    let promoted = if turn_ordinal.is_multiple_of(escalation_interval_turns) {
        latest_hypothesis.clone()
    } else {
        None
    };
    let literature = if promoted.is_some() {
        store
            .load_cancer_research_literature(
                state.world_id(),
                MAX_CANCER_RESEARCH_LITERATURE_DOCUMENTS,
            )
            .await?
    } else {
        Vec::new()
    };
    let (stage, task, inference_tier, frozen_candidate_hash, model_max_output_tokens) =
        if let Some(prior) = &promoted.filter(|_| !literature.is_empty()) {
            prior.validate()?;
            let content = serde_json::to_string(prior.contribution())?;
            let candidate_hash = Digest::canonical(prior.contribution())?;
            evidence_documents.push(CancerResearchEvidenceDocument {
                reference: CancerResearchEvidenceReference {
                    kind: CancerResearchEvidenceKind::FrozenHypothesis,
                    source_id: format!(
                        "cancer-world://{}/artifact/{}",
                        state.world_id(),
                        prior.contribution().contribution_id
                    ),
                    content_hash: Digest::sha256(content.as_bytes()),
                },
                content,
            });
            evidence_documents.extend(literature.into_iter().map(|snapshot| snapshot.document));
            (
                CancerResearchStage::LiteratureAudit,
                CancerResearchTask::ChallengeFrozenHypothesis,
                CancerResearchInferenceTier::Escalation,
                Some(candidate_hash),
                4_096,
            )
        } else {
            (
                CancerResearchStage::BlindDiscovery,
                if engineering_turn && turn_ordinal.is_multiple_of(2) {
                    CancerResearchTask::DesignDiagnosticInstrument
                } else if engineering_turn {
                    CancerResearchTask::DesignTreatmentMachine
                } else if turn_ordinal.is_multiple_of(2) {
                    CancerResearchTask::GenerateMechanisticHypothesis
                } else {
                    CancerResearchTask::ProposeDiscriminatingExperiment
                },
                CancerResearchInferenceTier::Exploration,
                None,
                BLIND_RESEARCH_MAX_OUTPUT_TOKENS,
            )
        };
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
    selected_resident_id: EntityId,
    burdens: &[(EntityId, CancerBurdenState)],
) -> Result<Vec<CancerResearchEvidenceDocument>, CancerResearchSchedulerError> {
    let selected = burdens
        .iter()
        .find(|(resident_id, _)| *resident_id == selected_resident_id)
        .ok_or(CancerResearchSchedulerError::MissingCancerBurden(
            selected_resident_id,
        ))?;
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
    let resident_content = serde_json::to_string(&ResidentBurdenObservation {
        observation_schema_version: 1,
        day_ordinal,
        resident_id: selected_resident_id,
        burden: &selected.1,
    })?;
    let cohort_content = serde_json::to_string(&cohort)?;
    Ok(vec![
        observation_document(
            format!("cancer-world://{world_id}/day/{day_ordinal}/cohort/burden-summary"),
            cohort_content,
        ),
        observation_document(
            format!(
                "cancer-world://{world_id}/day/{day_ordinal}/resident/{selected_resident_id}/burden"
            ),
            resident_content,
        ),
    ])
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
    #[error("embedded Cancer World biological primitives could not be decoded: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("Cancer World research scheduler hashing failed: {0}")]
    Hash(#[from] world_domain::CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{WorldId, WorldSeed};

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
        let documents = cancer_burden_observations(world_id, 0, burdens[1].0, &burdens)
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
    }
}
