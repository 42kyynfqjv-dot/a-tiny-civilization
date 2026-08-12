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
    PublicCancerLabCapabilityStatus, PublicCancerNci60BenchmarkPartition,
    PublicCancerNci60BenchmarkSummary, PublicCancerNci60ResponseQualification,
    PublicCancerResearchArtifact, PublicCancerResearchCampaign,
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
    CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION, CANCER_RESEARCH_NOVELTY_METHOD_VERSION,
    CANCER_VIRTUAL_LAB_METHOD_VERSION, CancerNci60ResponseQualification,
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
    qualification_payload: Option<Value>,
    qualification_checksum: Option<Vec<u8>>,
    qualification_created_at: Option<DateTime<Utc>>,
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

#[derive(FromRow)]
struct Nci60BenchmarkStatsRow {
    intervention_kind: String,
    qualifications_opened: i64,
    informative_qualifications: i64,
    comparable_pairs: i64,
    concordant_pairs: i64,
    most_responsive_line_evaluated: i64,
    most_responsive_line_correct: i64,
    least_responsive_line_evaluated: i64,
    least_responsive_line_correct: i64,
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

        let nci60_benchmark_rows = sqlx::query_as::<_, Nci60BenchmarkStatsRow>(
            r#"
            SELECT
                result_payload->'intervention'->>'kind' AS intervention_kind,
                COUNT(*) AS qualifications_opened,
                COUNT(*) FILTER (
                    WHERE (result_payload->>'pairwise_comparison_count')::BIGINT > 0
                ) AS informative_qualifications,
                COALESCE(SUM(
                    (result_payload->>'pairwise_comparison_count')::BIGINT
                ), 0)::BIGINT AS comparable_pairs,
                COALESCE(SUM(
                    (result_payload->>'concordant_pair_count')::BIGINT
                ), 0)::BIGINT AS concordant_pairs,
                COUNT(*) FILTER (
                    WHERE result_payload->>'most_responsive_line_correct' IS NOT NULL
                ) AS most_responsive_line_evaluated,
                COUNT(*) FILTER (
                    WHERE result_payload->>'most_responsive_line_correct' = 'true'
                ) AS most_responsive_line_correct,
                COUNT(*) FILTER (
                    WHERE result_payload->>'least_responsive_line_correct' IS NOT NULL
                ) AS least_responsive_line_evaluated,
                COUNT(*) FILTER (
                    WHERE result_payload->>'least_responsive_line_correct' = 'true'
                ) AS least_responsive_line_correct
            FROM cancer_nci60_response_qualifications
            WHERE world_id=$1 AND method_version=$2
            GROUP BY result_payload->'intervention'->>'kind'
            ORDER BY intervention_kind
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i32::from(
            CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION,
        ))
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        let nci60_benchmark = summarize_nci60_benchmark(&nci60_benchmark_rows)?;

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
                   experiment_memory.completed_at AS experiment_memory_completed_at,
                   qualification.result_payload AS qualification_payload,
                   qualification.result_checksum AS qualification_checksum,
                   qualification.created_at AS qualification_created_at
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
            LEFT JOIN cancer_nci60_response_qualifications AS qualification
              ON qualification.request_id=request.request_id
             AND qualification.method_version=$5
            WHERE request.world_id=$1
              AND result.result_payload->'receipt' <> 'null'::JSONB
            ORDER BY request.ordinal, request.request_id
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(collective_id.as_uuid())
        .bind(i32::from(CANCER_RESEARCH_NOVELTY_METHOD_VERSION))
        .bind(i32::from(CANCER_VIRTUAL_LAB_METHOD_VERSION))
        .bind(i32::from(
            CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION,
        ))
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
            let nci60_qualification = match (
                row.qualification_payload,
                row.qualification_checksum,
                row.qualification_created_at,
            ) {
                (None, None, None) => None,
                (Some(payload), Some(checksum), Some(created_at)) => {
                    let result: CancerNci60ResponseQualification = serde_json::from_value(payload)
                        .map_err(|error| {
                            corrupt(format!("invalid NCI-60 qualification result: {error}"))
                        })?;
                    result
                        .validate_against(&receipt.contribution)
                        .map_err(contract_error)?;
                    let result_hash =
                        digest_from_db(&checksum, "NCI-60 qualification result checksum")?;
                    if result.world_id != world_id
                        || result.request_id != request.request_id
                        || result.artifact_hash != artifact_hash
                        || result
                            .canonical_hash(&receipt.contribution)
                            .map_err(contract_error)?
                            != result_hash
                    {
                        return Err(corrupt(
                            "Cancer World NCI-60 qualification crossed its immutable artifact provenance",
                        ));
                    }
                    Some(PublicCancerNci60ResponseQualification {
                        result,
                        result_hash,
                        created_at,
                    })
                }
                _ => {
                    return Err(corrupt(
                        "Cancer World NCI-60 qualification is only partially persisted",
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
                nci60_qualification,
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
            nci60_benchmark,
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
            Abstracted,
            "Every current screen now exposes sensitive, drug-tolerant, and resistant phenotypic compartments plus treatment selection. Mutation, genomic lineages, plasticity, and longitudinal relapse remain absent.",
        ),
        (
            "Pharmacokinetics, pharmacodynamics, and blood-brain barrier",
            Abstracted,
            "Orthotopic drug-like screens now expose systemic, BBB, unbound-brain, and target-engagement values. They remain dimensionless structural assumptions until a real compound profile is source-calibrated.",
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

fn summarize_nci60_benchmark(
    rows: &[Nci60BenchmarkStatsRow],
) -> Result<PublicCancerNci60BenchmarkSummary, ObserverProjectionStoreError> {
    let mut single_agent = None;
    let mut combination = None;
    for row in rows {
        let partition = nci60_benchmark_partition(row)?;
        let destination = match row.intervention_kind.as_str() {
            "single_agent" => &mut single_agent,
            "combination" => &mut combination,
            _ => return Err(corrupt("unknown NCI-60 benchmark intervention kind")),
        };
        if destination.replace(partition).is_some() {
            return Err(corrupt("duplicate NCI-60 benchmark intervention partition"));
        }
    }
    let single_agent = single_agent.unwrap_or_default();
    let combination = combination.unwrap_or_default();
    let overall = combine_nci60_benchmark_partitions(&single_agent, &combination)?;
    Ok(PublicCancerNci60BenchmarkSummary {
        overall,
        single_agent,
        combination,
        caveats: vec![
            "Public NCI-60 and ALMANAC in-vitro rank benchmarking only; this is not a treatment verdict, patient-efficacy result, or clinical evidence.".to_owned(),
            "These public datasets may have appeared in model training; runtime answer-key isolation does not make this a clean out-of-sample test.".to_owned(),
        ],
    })
}

fn nci60_benchmark_partition(
    row: &Nci60BenchmarkStatsRow,
) -> Result<PublicCancerNci60BenchmarkPartition, ObserverProjectionStoreError> {
    let partition = PublicCancerNci60BenchmarkPartition {
        qualifications_opened: to_u64(row.qualifications_opened, "NCI-60 qualification count")?,
        informative_qualifications: to_u64(
            row.informative_qualifications,
            "informative NCI-60 qualification count",
        )?,
        comparable_pairs: to_u64(row.comparable_pairs, "NCI-60 comparable pair count")?,
        concordant_pairs: to_u64(row.concordant_pairs, "NCI-60 concordant pair count")?,
        pooled_pairwise_concordance_per_mille: None,
        most_responsive_line_evaluated: to_u64(
            row.most_responsive_line_evaluated,
            "NCI-60 top-rank evaluation count",
        )?,
        most_responsive_line_correct: to_u64(
            row.most_responsive_line_correct,
            "NCI-60 correct top-rank count",
        )?,
        least_responsive_line_evaluated: to_u64(
            row.least_responsive_line_evaluated,
            "NCI-60 bottom-rank evaluation count",
        )?,
        least_responsive_line_correct: to_u64(
            row.least_responsive_line_correct,
            "NCI-60 correct bottom-rank count",
        )?,
    };
    finish_nci60_benchmark_partition(partition)
}

fn combine_nci60_benchmark_partitions(
    left: &PublicCancerNci60BenchmarkPartition,
    right: &PublicCancerNci60BenchmarkPartition,
) -> Result<PublicCancerNci60BenchmarkPartition, ObserverProjectionStoreError> {
    let add = |left: u64, right: u64| {
        left.checked_add(right)
            .ok_or_else(|| corrupt("NCI-60 benchmark aggregate overflow"))
    };
    finish_nci60_benchmark_partition(PublicCancerNci60BenchmarkPartition {
        qualifications_opened: add(left.qualifications_opened, right.qualifications_opened)?,
        informative_qualifications: add(
            left.informative_qualifications,
            right.informative_qualifications,
        )?,
        comparable_pairs: add(left.comparable_pairs, right.comparable_pairs)?,
        concordant_pairs: add(left.concordant_pairs, right.concordant_pairs)?,
        pooled_pairwise_concordance_per_mille: None,
        most_responsive_line_evaluated: add(
            left.most_responsive_line_evaluated,
            right.most_responsive_line_evaluated,
        )?,
        most_responsive_line_correct: add(
            left.most_responsive_line_correct,
            right.most_responsive_line_correct,
        )?,
        least_responsive_line_evaluated: add(
            left.least_responsive_line_evaluated,
            right.least_responsive_line_evaluated,
        )?,
        least_responsive_line_correct: add(
            left.least_responsive_line_correct,
            right.least_responsive_line_correct,
        )?,
    })
}

fn finish_nci60_benchmark_partition(
    mut partition: PublicCancerNci60BenchmarkPartition,
) -> Result<PublicCancerNci60BenchmarkPartition, ObserverProjectionStoreError> {
    if partition.informative_qualifications > partition.qualifications_opened
        || partition.concordant_pairs > partition.comparable_pairs
        || partition.most_responsive_line_evaluated > partition.qualifications_opened
        || partition.most_responsive_line_correct > partition.most_responsive_line_evaluated
        || partition.least_responsive_line_evaluated > partition.qualifications_opened
        || partition.least_responsive_line_correct > partition.least_responsive_line_evaluated
        || (partition.comparable_pairs == 0 && partition.concordant_pairs != 0)
    {
        return Err(corrupt("invalid NCI-60 benchmark aggregate"));
    }
    partition.pooled_pairwise_concordance_per_mille = if partition.comparable_pairs == 0 {
        None
    } else {
        let score = (u128::from(partition.concordant_pairs) * 1_000)
            / u128::from(partition.comparable_pairs);
        Some(
            u16::try_from(score)
                .map_err(|_| corrupt("NCI-60 benchmark score exceeds per-mille range"))?,
        )
    };
    Ok(partition)
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

    fn benchmark_row(
        intervention_kind: &str,
        qualifications_opened: i64,
        comparable_pairs: i64,
        concordant_pairs: i64,
        top_correct: i64,
        bottom_correct: i64,
    ) -> Nci60BenchmarkStatsRow {
        Nci60BenchmarkStatsRow {
            intervention_kind: intervention_kind.to_owned(),
            qualifications_opened,
            informative_qualifications: qualifications_opened,
            comparable_pairs,
            concordant_pairs,
            most_responsive_line_evaluated: qualifications_opened,
            most_responsive_line_correct: top_correct,
            least_responsive_line_evaluated: qualifications_opened,
            least_responsive_line_correct: bottom_correct,
        }
    }

    #[test]
    fn nci60_benchmark_pools_pair_counts_instead_of_averaging_scores() {
        let summary = summarize_nci60_benchmark(&[
            benchmark_row("single_agent", 2, 10, 10, 2, 1),
            benchmark_row("combination", 1, 2, 0, 0, 1),
        ])
        .expect("benchmark summary");

        assert_eq!(
            summary.single_agent.pooled_pairwise_concordance_per_mille,
            Some(1_000)
        );
        assert_eq!(
            summary.combination.pooled_pairwise_concordance_per_mille,
            Some(0)
        );
        assert_eq!(summary.overall.qualifications_opened, 3);
        assert_eq!(summary.overall.comparable_pairs, 12);
        assert_eq!(summary.overall.concordant_pairs, 10);
        assert_eq!(
            summary.overall.pooled_pairwise_concordance_per_mille,
            Some(833)
        );
        assert_eq!(summary.overall.most_responsive_line_correct, 2);
        assert_eq!(summary.overall.least_responsive_line_correct, 2);
    }

    #[test]
    fn nci60_benchmark_is_well_formed_before_any_challenge_is_opened() {
        let summary = summarize_nci60_benchmark(&[]).expect("empty benchmark summary");

        assert_eq!(
            summary.overall,
            PublicCancerNci60BenchmarkPartition::default()
        );
        assert_eq!(
            summary.single_agent,
            PublicCancerNci60BenchmarkPartition::default()
        );
        assert_eq!(
            summary.combination,
            PublicCancerNci60BenchmarkPartition::default()
        );
        assert_eq!(summary.caveats.len(), 2);
    }
}
