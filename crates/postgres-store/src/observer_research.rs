use application::{
    CancerResearchLadderResult, CancerResearchModelRequest, cancer_research_collective_id,
    cancer_research_memory_bank_id, cancer_research_titles_duplicate,
};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use observer_projection::{
    ObserverCancerResearchStore, ObserverProjectionStoreError,
    PUBLIC_CANCER_RESEARCH_PROJECTION_VERSION, PublicCancerResearchArtifact,
    PublicCancerResearchDuplicate, PublicCancerResearchEvidence, PublicCancerResearchView,
    PublicResearchMemoryState,
};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;
use world_domain::{Digest, WorldId};

use crate::PostgresStore;

#[derive(FromRow)]
struct ResearchProjectionRow {
    request_payload: Value,
    request_checksum: Vec<u8>,
    result_payload: Value,
    result_checksum: Vec<u8>,
    created_at: DateTime<Utc>,
    memory_completed_at: Option<DateTime<Utc>>,
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
                   memory.completed_at AS memory_completed_at
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN memory_outbox AS memory
              ON memory.world_id=request.world_id
             AND memory.agent_id=$2
             AND (memory.payload->>'ordinal')::BIGINT=request.ordinal
             AND memory.payload->>'context'='Cancer World research artifact'
            WHERE request.world_id=$1
              AND result.result_payload->'receipt' <> 'null'::JSONB
            ORDER BY request.ordinal, request.request_id
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(collective_id.as_uuid())
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
            artifacts.push(PublicCancerResearchArtifact {
                request_id: request.request_id,
                selected_at_tick: request.selection.selected_at_tick,
                ordinal: request.selection.ordinal,
                target: request.selection.target,
                task: request.selection.task,
                inference_tier: request.selection.inference_tier,
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
                created_at: row.created_at,
                duplicates: Vec::new(),
            });
        }
        let (mut artifacts, duplicate_artifacts) = collapse_duplicate_research(artifacts);
        let distinct_artifacts = u64::try_from(artifacts.len())
            .map_err(|_| corrupt("distinct artifact count overflow"))?;
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
            artifacts,
            evidence,
        }))
    }
}

fn collapse_duplicate_research(
    artifacts: Vec<PublicCancerResearchArtifact>,
) -> (Vec<PublicCancerResearchArtifact>, u64) {
    let mut canonical: Vec<PublicCancerResearchArtifact> = Vec::new();
    let mut duplicate_count = 0_u64;
    for artifact in artifacts {
        let duplicate_of = canonical.iter_mut().find(|existing| {
            existing.contribution.artifact_kind == artifact.contribution.artifact_kind
                && cancer_research_titles_duplicate(
                    &existing.contribution.title,
                    &artifact.contribution.title,
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
