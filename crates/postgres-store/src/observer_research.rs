use application::{
    CancerResearchCampaignTestAssessment, CancerResearchLadderResult, CancerResearchModelRequest,
    cancer_research_campaign_test_assessment, cancer_research_collective_id,
    cancer_research_contributions_duplicate, cancer_research_memory_bank_id,
};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use observer_projection::{
    ObserverCancerResearchStore, ObserverProjectionStoreError,
    PUBLIC_CANCER_RESEARCH_PROJECTION_VERSION, PublicCancerLabCapability,
    PublicCancerLabCapabilityStatus, PublicCancerResearchArtifact, PublicCancerResearchCampaign,
    PublicCancerResearchCampaignOutcome, PublicCancerResearchDuplicate,
    PublicCancerResearchEvidence, PublicCancerResearchNoveltyAudit,
    PublicCancerResearchProgramSummary, PublicCancerResearchView,
    PublicCancerVirtualExperimentResult, PublicResearchMemoryState,
};
use serde_json::Value;
use sqlx::FromRow;
use std::collections::BTreeMap;
use uuid::Uuid;
use world_domain::{
    CANCER_RESEARCH_NOVELTY_METHOD_VERSION, CANCER_VIRTUAL_LAB_METHOD_VERSION,
    CancerResearchNoveltyAudit, CancerResearchProgram, CancerVirtualExperimentInterpretation,
    CancerVirtualExperimentResult, Digest, WorldId,
};

use crate::PostgresStore;

#[derive(FromRow)]
struct ResearchProjectionRow {
    request_payload: Value,
    request_checksum: Vec<u8>,
    result_payload: Value,
    result_checksum: Vec<u8>,
    created_at: DateTime<Utc>,
    memory_completed_at: Option<DateTime<Utc>>,
    novelty_payload: Option<Value>,
    novelty_checksum: Option<Vec<u8>>,
    novelty_created_at: Option<DateTime<Utc>>,
    experiment_payload: Option<Value>,
    experiment_checksum: Option<Vec<u8>>,
    experiment_created_at: Option<DateTime<Utc>>,
    experiment_memory_completed_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct ResearchEvidenceRow {
    evidence_id: Uuid,
    source_id: String,
    title: String,
    license: String,
    published_at: Option<NaiveDate>,
    content_hash: Vec<u8>,
    retrieved_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct ResearchStatsRow {
    total_requests: i64,
    pending_requests: i64,
    successful_requests: i64,
    unsuccessful_requests: i64,
    memory_queued: i64,
    memory_accepted: i64,
    first_request_payload: Option<Value>,
}

#[async_trait]
impl ObserverCancerResearchStore for PostgresStore {
    async fn public_cancer_research(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<Option<PublicCancerResearchView>, ObserverProjectionStoreError> {
        let collective_id = cancer_research_collective_id(world_id);
        let stats = sqlx::query_as::<_, ResearchStatsRow>(
            r#"
            SELECT
                COUNT(*) AS total_requests,
                COUNT(*) FILTER (WHERE request.completed_at IS NULL) AS pending_requests,
                COUNT(*) FILTER (
                    WHERE result.result_payload->'receipt' <> 'null'::JSONB
                ) AS successful_requests,
                COUNT(*) FILTER (
                    WHERE request.completed_at IS NOT NULL
                      AND (
                          result.request_id IS NULL
                          OR result.result_payload->'receipt' IS NULL
                          OR result.result_payload->'receipt' = 'null'::JSONB
                      )
                ) AS unsuccessful_requests,
                (
                    SELECT COUNT(*)
                    FROM memory_outbox
                    WHERE world_id=$1 AND agent_id=$2
                      AND payload->>'context'='Cancer World research artifact'
                      AND completed_at IS NULL
                ) AS memory_queued,
                (
                    SELECT COUNT(*)
                    FROM memory_outbox
                    WHERE world_id=$1 AND agent_id=$2
                      AND payload->>'context'='Cancer World research artifact'
                      AND completed_at IS NOT NULL
                ) AS memory_accepted,
                (ARRAY_AGG(request.request_payload ORDER BY request.ordinal, request.request_id))[1]
                    AS first_request_payload
            FROM cancer_research_requests AS request
            LEFT JOIN cancer_research_results AS result USING (request_id)
            WHERE request.world_id=$1
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(collective_id.as_uuid())
        .fetch_one(self.pool())
        .await
        .map_err(unavailable)?;
        if stats.total_requests == 0 {
            return Ok(None);
        }
        let first_request: CancerResearchModelRequest = serde_json::from_value(
            stats
                .first_request_payload
                .ok_or_else(|| corrupt("research stats omitted their first request"))?,
        )
        .map_err(|error| corrupt(format!("invalid first research request: {error}")))?;
        first_request
            .validate()
            .map_err(|error| corrupt(format!("invalid first research request: {error}")))?;
        if first_request.selection.world_id != world_id {
            return Err(corrupt("first research request crossed its world boundary"));
        }

        let rows = sqlx::query_as::<_, ResearchProjectionRow>(
            r#"
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum, result.created_at,
                   memory.completed_at AS memory_completed_at,
                   novelty.audit_payload AS novelty_payload,
                   novelty.audit_checksum AS novelty_checksum,
                   novelty.created_at AS novelty_created_at,
                   experiment.result_payload AS experiment_payload,
                   experiment.result_checksum AS experiment_checksum,
                   experiment.created_at AS experiment_created_at,
                   experiment_memory.completed_at AS experiment_memory_completed_at
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN memory_outbox AS memory
              ON memory.world_id=request.world_id
             AND memory.agent_id=$2
             AND (memory.payload->>'ordinal')::BIGINT=request.ordinal
             AND memory.payload->>'context'='Cancer World research artifact'
            LEFT JOIN cancer_research_novelty_audits AS novelty
              ON novelty.request_id=request.request_id
             AND novelty.method_version=$3
            LEFT JOIN cancer_virtual_experiment_results AS experiment
              ON experiment.request_id=request.request_id
             AND experiment.method_version=$4
            LEFT JOIN memory_outbox AS experiment_memory
              ON experiment_memory.world_id=request.world_id
             AND experiment_memory.agent_id=$2
             AND (experiment_memory.payload->>'ordinal')::BIGINT=request.ordinal
             AND experiment_memory.payload->>'context'='Cancer World virtual experiment result'
            WHERE request.world_id=$1
              AND result.result_payload->'receipt' <> 'null'::JSONB
            ORDER BY request.ordinal, request.request_id
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(collective_id.as_uuid())
        .bind(i32::from(CANCER_RESEARCH_NOVELTY_METHOD_VERSION))
        .bind(i32::from(CANCER_VIRTUAL_LAB_METHOD_VERSION))
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        let mut artifacts = Vec::with_capacity(rows.len());
        for row in rows {
            let request: CancerResearchModelRequest =
                serde_json::from_value(row.request_payload)
                    .map_err(|error| corrupt(format!("invalid research request: {error}")))?;
            let result: CancerResearchLadderResult = serde_json::from_value(row.result_payload)
                .map_err(|error| corrupt(format!("invalid research result: {error}")))?;
            if request.selection.world_id != world_id
                || request.canonical_hash().map_err(contract_error)?
                    != digest_from_db(&row.request_checksum, "research request checksum")?
                || Digest::canonical(&result).map_err(contract_error)?
                    != digest_from_db(&row.result_checksum, "research result checksum")?
            {
                return Err(corrupt(
                    "Cancer World research projection failed its durable provenance",
                ));
            }
            let receipt = result
                .receipt
                .ok_or_else(|| corrupt("successful research result omitted its receipt"))?;
            if result.request_id != request.request_id
                || receipt.request_id != request.request_id
                || receipt.request_hash != request.canonical_hash().map_err(contract_error)?
            {
                return Err(corrupt(
                    "historical research result crossed its immutable request provenance",
                ));
            }
            receipt
                .contribution
                .validate_against(&request.selection)
                .map_err(contract_error)?;
            let artifact_hash = Digest::canonical(&receipt.contribution).map_err(contract_error)?;
            let novelty_audit = match (
                row.novelty_payload,
                row.novelty_checksum,
                row.novelty_created_at,
            ) {
                (None, None, None) => None,
                (Some(payload), Some(checksum), Some(created_at)) => {
                    let audit: CancerResearchNoveltyAudit = serde_json::from_value(payload)
                        .map_err(|error| corrupt(format!("invalid novelty audit: {error}")))?;
                    audit.validate().map_err(contract_error)?;
                    let audit_hash = digest_from_db(&checksum, "research novelty audit checksum")?;
                    if audit.world_id != world_id
                        || audit.request_id != request.request_id
                        || audit.artifact_hash != artifact_hash
                        || audit.canonical_hash().map_err(contract_error)? != audit_hash
                    {
                        return Err(corrupt(
                            "Cancer World novelty audit crossed its immutable artifact provenance",
                        ));
                    }
                    Some(PublicCancerResearchNoveltyAudit {
                        audit,
                        audit_hash,
                        created_at,
                    })
                }
                _ => {
                    return Err(corrupt(
                        "Cancer World novelty audit is only partially persisted",
                    ));
                }
            };
            let virtual_experiment = match (
                row.experiment_payload,
                row.experiment_checksum,
                row.experiment_created_at,
            ) {
                (None, None, None) => None,
                (Some(payload), Some(checksum), Some(created_at)) => {
                    let result: CancerVirtualExperimentResult = serde_json::from_value(payload)
                        .map_err(|error| {
                            corrupt(format!("invalid virtual experiment result: {error}"))
                        })?;
                    result
                        .validate_against(&receipt.contribution)
                        .map_err(contract_error)?;
                    let result_hash =
                        digest_from_db(&checksum, "virtual experiment result checksum")?;
                    if result.world_id != world_id
                        || result.request_id != request.request_id
                        || result.artifact_hash != artifact_hash
                        || result
                            .canonical_hash(&receipt.contribution)
                            .map_err(contract_error)?
                            != result_hash
                    {
                        return Err(corrupt(
                            "Cancer World virtual experiment crossed its immutable artifact provenance",
                        ));
                    }
                    Some(PublicCancerVirtualExperimentResult {
                        result,
                        result_hash,
                        memory_state: if row.experiment_memory_completed_at.is_some() {
                            PublicResearchMemoryState::Accepted
                        } else {
                            PublicResearchMemoryState::Queued
                        },
                        created_at,
                    })
                }
                _ => {
                    return Err(corrupt(
                        "Cancer World virtual experiment is only partially persisted",
                    ));
                }
            };
            artifacts.push(PublicCancerResearchArtifact {
                request_id: request.request_id,
                selected_at_tick: request.selection.selected_at_tick,
                ordinal: request.selection.ordinal,
                program: CancerResearchProgram::for_ordinal(request.selection.ordinal),
                target: request.selection.target,
                task: request.selection.task,
                inference_tier: request.selection.inference_tier,
                frozen_candidate_hash: request.selection.frozen_candidate_hash,
                contribution: receipt.contribution,
                artifact_hash,
                evidence: request.selection.evidence,
                recalled_artifact_hashes: request
                    .recalled_memories
                    .into_iter()
                    .map(|memory| memory.source_artifact_hash)
                    .collect(),
                requested_model: receipt.requested_model,
                resolved_model: receipt.resolved_model,
                prompt_tokens: receipt.usage.prompt_tokens,
                completion_tokens: receipt.usage.completion_tokens,
                billed_micro_usd: receipt.billed_micro_usd,
                result_hash: digest_from_db(&row.result_checksum, "research result checksum")?,
                memory_state: if row.memory_completed_at.is_some() {
                    PublicResearchMemoryState::Accepted
                } else {
                    PublicResearchMemoryState::Queued
                },
                novelty_audit,
                virtual_experiment,
                created_at: row.created_at,
                duplicates: Vec::new(),
            });
        }
        let campaigns = reconstruct_campaigns(&artifacts)?;
        let (mut artifacts, duplicate_artifacts) = collapse_duplicate_research(artifacts);
        let distinct_artifacts = u64::try_from(artifacts.len())
            .map_err(|_| corrupt("distinct artifact count overflow"))?;
        let programs = [
            CancerResearchProgram::Devices,
            CancerResearchProgram::Treatments,
        ]
        .into_iter()
        .map(|program| summarize_program(program, &artifacts))
        .collect::<Result<Vec<_>, _>>()?;
        artifacts.truncate(usize::from(limit.clamp(1, 500)));

        let evidence_rows = sqlx::query_as::<_, ResearchEvidenceRow>(
            r#"
            SELECT evidence_id,source_id,title,license,published_at,content_hash,retrieved_at
            FROM cancer_research_literature
            WHERE world_id=$1
            ORDER BY published_at DESC NULLS LAST,evidence_id
            LIMIT $2
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i64::from(limit.clamp(1, 500)))
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        let evidence = evidence_rows
            .into_iter()
            .map(|row| {
                Ok(PublicCancerResearchEvidence {
                    evidence_id: row.evidence_id,
                    source_id: row.source_id,
                    title: row.title,
                    license: row.license,
                    published_at: row.published_at,
                    content_hash: digest_from_db(&row.content_hash, "literature content hash")?,
                    retrieved_at: row.retrieved_at,
                })
            })
            .collect::<Result<Vec<_>, ObserverProjectionStoreError>>()?;

        Ok(Some(PublicCancerResearchView {
            projection_version: PUBLIC_CANCER_RESEARCH_PROJECTION_VERSION,
            world_id,
            memory_bank_id: cancer_research_memory_bank_id(world_id),
            target: first_request.selection.target,
            total_requests: to_u64(stats.total_requests, "total request count")?,
            pending_requests: to_u64(stats.pending_requests, "pending request count")?,
            successful_requests: to_u64(stats.successful_requests, "successful request count")?,
            unsuccessful_requests: to_u64(
                stats.unsuccessful_requests,
                "unsuccessful request count",
            )?,
            distinct_artifacts,
            duplicate_artifacts,
            memory_queued: to_u64(stats.memory_queued, "queued memory count")?,
            memory_accepted: to_u64(stats.memory_accepted, "accepted memory count")?,
            programs,
            campaigns,
            lab_capabilities: cancer_lab_capabilities(),
            artifacts,
            evidence,
        }))
    }
}

fn reconstruct_campaigns(
    artifacts: &[PublicCancerResearchArtifact],
) -> Result<Vec<PublicCancerResearchCampaign>, ObserverProjectionStoreError> {
    let roots = artifacts
        .iter()
        .filter(|artifact| artifact.frozen_candidate_hash.is_none())
        .map(|artifact| (artifact.artifact_hash, artifact))
        .collect::<BTreeMap<_, _>>();
    let mut children = BTreeMap::<Digest, Vec<&PublicCancerResearchArtifact>>::new();
    for artifact in artifacts {
        if let Some(root_hash) = artifact.frozen_candidate_hash {
            children.entry(root_hash).or_default().push(artifact);
        }
    }
    let mut campaigns = Vec::with_capacity(children.len());
    for (root_hash, entries) in children {
        let root = roots.get(&root_hash).copied().ok_or_else(|| {
            corrupt("Cancer World campaign child references a missing root artifact")
        })?;
        if entries.iter().any(|entry| entry.program != root.program) {
            return Err(corrupt(
                "Cancer World campaign crossed its independent program boundary",
            ));
        }
        let mut supporting_tests = 0_u8;
        let mut falsifying_tests = 0_u8;
        let mut inconclusive_tests = 0_u8;
        let mut synthesis_complete = false;
        let mut newest_ordinal = root.ordinal;
        for entry in entries {
            newest_ordinal = newest_ordinal.max(entry.ordinal);
            match entry.task {
                world_domain::CancerResearchTask::DesignIndependentReplication => {
                    let Some(experiment) = entry.virtual_experiment.as_ref() else {
                        continue;
                    };
                    match cancer_research_campaign_test_assessment(&experiment.result) {
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
                }
                world_domain::CancerResearchTask::InterpretReplicationResult => {
                    synthesis_complete = true;
                }
                _ => {
                    return Err(corrupt(
                        "Cancer World campaign contains an invalid follow-up task",
                    ));
                }
            }
        }
        let outcome = campaign_outcome(supporting_tests, falsifying_tests, inconclusive_tests);
        campaigns.push(PublicCancerResearchCampaign {
            campaign_id: application::CancerResearchCampaignDirective::campaign_id(root.request_id),
            program: root.program,
            root_request_id: root.request_id,
            root_artifact_hash: root_hash,
            root_title: root.contribution.title.clone(),
            outcome,
            supporting_tests,
            falsifying_tests,
            inconclusive_tests,
            synthesis_complete,
            newest_ordinal,
        });
    }
    campaigns.sort_by(|left, right| {
        right
            .newest_ordinal
            .cmp(&left.newest_ordinal)
            .then_with(|| left.campaign_id.cmp(&right.campaign_id))
    });
    Ok(campaigns)
}

fn campaign_outcome(
    supporting_tests: u8,
    falsifying_tests: u8,
    inconclusive_tests: u8,
) -> PublicCancerResearchCampaignOutcome {
    let test_count = usize::from(supporting_tests)
        .saturating_add(usize::from(falsifying_tests))
        .saturating_add(usize::from(inconclusive_tests));
    if falsifying_tests > 0 {
        PublicCancerResearchCampaignOutcome::Falsified
    } else if usize::from(supporting_tests)
        >= application::CANCER_RESEARCH_CAMPAIGN_REQUIRED_SUPPORTS
    {
        PublicCancerResearchCampaignOutcome::SurvivedReplicationRound
    } else if test_count >= application::CANCER_RESEARCH_CAMPAIGN_MAX_TESTS {
        PublicCancerResearchCampaignOutcome::Inconclusive
    } else {
        PublicCancerResearchCampaignOutcome::Testing
    }
}

fn cancer_lab_capabilities() -> Vec<PublicCancerLabCapability> {
    use PublicCancerLabCapabilityStatus::{Abstracted, Available, Missing, RequiresRealLab};
    [
        (
            "Closed preregistered experiment plans",
            Available,
            "Plans freeze subject abstraction, modality, targets, endpoint, intensity, exposure, and cohort before execution.",
        ),
        (
            "Adversarial replication campaigns",
            Available,
            "Promoted roots receive up to five distinct tests and survive only after three supporting results with no falsifying result.",
        ),
        (
            "Cell culture, organoid, and mouse subjects",
            Abstracted,
            "These are bounded mathematical subject factors, not cell-level organoids or anatomically exact animals.",
        ),
        (
            "Tumor mechanisms and intervention response",
            Abstracted,
            "Seven mechanism targets, seven intervention modalities, and six endpoints are represented with uncalibrated coefficients.",
        ),
        (
            "Tumor heterogeneity, evolution, and acquired resistance",
            Missing,
            "The lab does not yet simulate competing clones, selection pressure, or longitudinal resistance.",
        ),
        (
            "Pharmacokinetics, pharmacodynamics, and blood-brain barrier",
            Missing,
            "Drug exposure, distribution, metabolism, clearance, and brain penetration are not mechanistically modeled.",
        ),
        (
            "Spatial microenvironment and immune dynamics",
            Missing,
            "Spatial tissue structure, stromal interactions, immune populations, and cytokine dynamics are not represented.",
        ),
        (
            "Combination therapy interactions",
            Missing,
            "Synergy, antagonism, scheduling, and adaptive multi-intervention protocols are not executable yet.",
        ),
        (
            "Whole-organism toxicity and device physics",
            Missing,
            "Multi-organ safety, instrumentation geometry, calibration, manufacturing tolerances, and failure physics are absent.",
        ),
        (
            "Biological or clinical validation",
            RequiresRealLab,
            "No simulation can establish efficacy or safety; wet-lab, animal, and ultimately clinical validation remain external requirements.",
        ),
    ]
    .into_iter()
    .map(|(capability, status, detail)| PublicCancerLabCapability {
        capability: capability.to_owned(),
        status,
        detail: detail.to_owned(),
    })
    .collect()
}

fn collapse_duplicate_research(
    artifacts: Vec<PublicCancerResearchArtifact>,
) -> (Vec<PublicCancerResearchArtifact>, u64) {
    let mut canonical: Vec<PublicCancerResearchArtifact> = Vec::new();
    let mut duplicate_count = 0_u64;
    for artifact in artifacts {
        let duplicate_of = canonical.iter_mut().find(|existing| {
            existing.program == artifact.program
                && cancer_research_contributions_duplicate(
                    &existing.contribution,
                    &artifact.contribution,
                )
        });
        if let Some(original) = duplicate_of {
            original.duplicates.push(PublicCancerResearchDuplicate {
                request_id: artifact.request_id,
                ordinal: artifact.ordinal,
                title: artifact.contribution.title,
                artifact_hash: artifact.artifact_hash,
                result_hash: artifact.result_hash,
                created_at: artifact.created_at,
            });
            duplicate_count = duplicate_count.saturating_add(1);
        } else {
            canonical.push(artifact);
        }
    }
    canonical.sort_by(|left, right| {
        let left_activity = left
            .duplicates
            .last()
            .map_or(left.created_at, |duplicate| duplicate.created_at);
        let right_activity = right
            .duplicates
            .last()
            .map_or(right.created_at, |duplicate| duplicate.created_at);
        right_activity
            .cmp(&left_activity)
            .then_with(|| right.ordinal.cmp(&left.ordinal))
    });
    (canonical, duplicate_count)
}

fn summarize_program(
    program: CancerResearchProgram,
    artifacts: &[PublicCancerResearchArtifact],
) -> Result<PublicCancerResearchProgramSummary, ObserverProjectionStoreError> {
    let entries = artifacts
        .iter()
        .filter(|artifact| artifact.program == program)
        .collect::<Vec<_>>();
    let count = |predicate: fn(&PublicCancerResearchArtifact) -> bool| {
        u64::try_from(
            entries
                .iter()
                .filter(|artifact| predicate(artifact))
                .count(),
        )
        .map_err(|_| corrupt("program summary count overflow"))
    };
    Ok(PublicCancerResearchProgramSummary {
        program,
        distinct_artifacts: u64::try_from(entries.len())
            .map_err(|_| corrupt("program artifact count overflow"))?,
        duplicate_artifacts: entries.iter().try_fold(0_u64, |total, artifact| {
            u64::try_from(artifact.duplicates.len())
                .map_err(|_| corrupt("program duplicate count overflow"))
                .map(|duplicates| total.saturating_add(duplicates))
        })?,
        model_supported: count(|artifact| {
            artifact
                .virtual_experiment
                .as_ref()
                .is_some_and(|experiment| {
                    experiment.result.interpretation
                        == CancerVirtualExperimentInterpretation::ModelSupportsPrediction
                })
        })?,
        model_rejected: count(|artifact| {
            artifact
                .virtual_experiment
                .as_ref()
                .is_some_and(|experiment| {
                    matches!(
                        experiment.result.interpretation,
                        CancerVirtualExperimentInterpretation::ModelShowsNoMaterialEffect
                            | CancerVirtualExperimentInterpretation::ModelShowsConcerningTradeoff
                    )
                })
        })?,
        model_inconclusive: count(|artifact| {
            artifact
                .virtual_experiment
                .as_ref()
                .is_some_and(|experiment| {
                    experiment.result.interpretation
                        == CancerVirtualExperimentInterpretation::ModelInconclusive
                })
        })?,
        awaiting_evaluation: count(|artifact| artifact.virtual_experiment.is_none())?,
        newest_ordinal: entries.iter().map(|artifact| artifact.ordinal).max(),
    })
}

fn digest_from_db(bytes: &[u8], field: &str) -> Result<Digest, ObserverProjectionStoreError> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| corrupt(format!("{field} must contain 32 bytes")))?;
    Ok(Digest::from_bytes(bytes))
}

fn to_u64(value: i64, field: &str) -> Result<u64, ObserverProjectionStoreError> {
    u64::try_from(value).map_err(|_| corrupt(format!("{field} is negative")))
}

fn contract_error(error: impl std::fmt::Display) -> ObserverProjectionStoreError {
    corrupt(error.to_string())
}

fn unavailable(error: sqlx::Error) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Unavailable(error.to_string())
}

fn corrupt(message: impl Into<String>) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Corrupt(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_outcomes_are_computed_not_awarded_by_the_model() {
        assert_eq!(
            campaign_outcome(2, 0, 1),
            PublicCancerResearchCampaignOutcome::Testing
        );
        assert_eq!(
            campaign_outcome(3, 0, 0),
            PublicCancerResearchCampaignOutcome::SurvivedReplicationRound
        );
        assert_eq!(
            campaign_outcome(4, 1, 0),
            PublicCancerResearchCampaignOutcome::Falsified
        );
        assert_eq!(
            campaign_outcome(2, 0, 3),
            PublicCancerResearchCampaignOutcome::Inconclusive
        );
    }
}
