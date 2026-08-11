use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CancerResearchEvidenceKind, CancerResearchEvidenceReference, CancerResearchInferenceTier,
    CancerResearchProfile, CancerResearchStage, CancerResearchTask, Digest, EntityId, OrganismRole,
    SimTick, WorldExperimentCommitment,
};

use crate::{
    CancerResearchEvidenceDocument, CancerResearchJobStore, CancerResearchModelRequest, StoreError,
};

pub const CANCER_RESEARCH_SCHEDULER_VERSION: u16 = 1;
pub const CANCER_RESEARCH_BLIND_TURNS_PER_SIM_DAY: u32 = 1;
const SECONDS_PER_DAY: u64 = 86_400;
const BLIND_RESEARCH_MAX_OUTPUT_TOKENS: u16 = 2_048;
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
    let tick = state.tick().get();
    if !tick.is_multiple_of(ticks_per_day) {
        return Ok(None);
    }
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
    let Some(resident_id) =
        select_researcher(state.manifest().seed, day_ordinal, &affected_living_people)?
    else {
        return Ok(None);
    };
    let deadline_tick = tick
        .checked_add(ticks_per_day)
        .map(SimTick::new)
        .ok_or(CancerResearchSchedulerError::TickOverflow)?;
    let evidence_documents = embedded_biological_primitives()?;
    let evidence = evidence_documents
        .iter()
        .map(|document| document.reference.clone())
        .collect();
    let task = if day_ordinal.is_multiple_of(2) {
        CancerResearchTask::GenerateMechanisticHypothesis
    } else {
        CancerResearchTask::ProposeDiscriminatingExperiment
    };
    let selection = world_domain::CancerResearchTurnSelection::new(
        state.world_id(),
        resident_id,
        state.tick(),
        deadline_tick,
        day_ordinal,
        commitment.target,
        CancerResearchStage::BlindDiscovery,
        task,
        CancerResearchInferenceTier::Exploration,
        CancerResearchProfile::seeded(state.manifest().seed, resident_id)?,
        evidence,
        None,
        BLIND_RESEARCH_MAX_OUTPUT_TOKENS,
    )?;
    let request = CancerResearchModelRequest::new(selection, evidence_documents, Vec::new())?;
    let request_id = request.request_id;
    store.enqueue_cancer_research_request(&request).await?;
    Ok(Some(request_id))
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
}
