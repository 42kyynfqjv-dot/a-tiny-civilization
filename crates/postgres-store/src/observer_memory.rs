use application::{CognitionMemoryInput, MemoryRetain};
use async_trait::async_trait;
use observer_projection::{
    ObserverMemoryStore, ObserverProjectionStoreError, PUBLIC_MEMORY_PROJECTION_VERSION,
    PublicMemoryObservation, PublicMemoryRecall, PublicMemoryStream,
};
use serde::Deserialize;
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;
use world_domain::{EntityId, PerceptionChannel, SimTick, WorldId};

use crate::PostgresStore;

#[derive(FromRow)]
struct MemoryRow {
    payload: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectObservation {
    subject_id: Option<EntityId>,
    channel: PerceptionChannel,
    property_code: String,
    quantized_value: i32,
    uncertainty: u16,
}

#[derive(FromRow)]
struct RecallRow {
    request_id: Uuid,
    agent_id: Uuid,
    selected_tick: i64,
    deadline_tick: i64,
    admitted_memory_inputs: Value,
}

#[async_trait]
impl ObserverMemoryStore for PostgresStore {
    async fn public_memory_stream(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<PublicMemoryStream, ObserverProjectionStoreError> {
        let limit = i64::from(limit.clamp(1, 500));
        let memory_rows = sqlx::query_as::<_, MemoryRow>(
            r#"
            WITH ranked AS (
                SELECT payload,source_sequence,operation_id,
                       ROW_NUMBER() OVER (
                           PARTITION BY agent_id ORDER BY source_sequence DESC,operation_id
                       ) AS agent_rank
                FROM memory_outbox
                WHERE world_id=$1
                  AND completed_at IS NOT NULL
                  AND payload->>'context'='canonical-direct-perception-v1'
            )
            SELECT payload
            FROM ranked
            WHERE agent_rank <= 12
            ORDER BY source_sequence DESC,operation_id
            LIMIT $2
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        let mut observations = Vec::with_capacity(memory_rows.len());
        for row in memory_rows {
            let retained: MemoryRetain = serde_json::from_value(row.payload)
                .map_err(|error| corrupt(format!("invalid retained memory: {error}")))?;
            retained
                .validate()
                .map_err(|error| corrupt(format!("invalid retained-memory provenance: {error}")))?;
            if retained.world_id != world_id || retained.context != "canonical-direct-perception-v1"
            {
                return Err(corrupt(
                    "memory row crossed its world or public direct-observation boundary",
                ));
            }
            let direct: DirectObservation = serde_json::from_str(&retained.content)
                .map_err(|error| corrupt(format!("invalid direct observation: {error}")))?;
            observations.push(PublicMemoryObservation {
                document_id: retained.document_id,
                agent_id: retained.agent_id,
                subject_id: direct.subject_id,
                source_sequence: retained.source_sequence,
                tick: retained.sim_tick,
                channel: direct.channel,
                property_code: direct.property_code,
                quantized_value: direct.quantized_value,
                uncertainty: direct.uncertainty,
            });
        }

        let recall_rows = sqlx::query_as::<_, RecallRow>(
            r#"
            SELECT request.request_id,request.agent_id,request.selected_tick,
                   request.deadline_tick,outcome.admitted_memory_inputs
            FROM cognition_recall_outcomes outcome
            JOIN cognition_requests request USING (request_id)
            WHERE request.world_id=$1
              AND jsonb_array_length(outcome.admitted_memory_inputs) > 0
            ORDER BY request.selected_tick DESC,request.request_id
            LIMIT $2
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        let mut recalls = Vec::with_capacity(recall_rows.len());
        for row in recall_rows {
            let memories: Vec<CognitionMemoryInput> =
                serde_json::from_value(row.admitted_memory_inputs)
                    .map_err(|error| corrupt(format!("invalid admitted memories: {error}")))?;
            recalls.push(PublicMemoryRecall {
                request_id: row.request_id,
                agent_id: EntityId::from_uuid(row.agent_id),
                selected_tick: SimTick::new(to_u64(row.selected_tick, "selected tick")?),
                deadline_tick: SimTick::new(to_u64(row.deadline_tick, "deadline tick")?),
                document_ids: memories
                    .into_iter()
                    .map(|memory| memory.document_id)
                    .collect(),
            });
        }

        Ok(PublicMemoryStream {
            projection_version: PUBLIC_MEMORY_PROJECTION_VERSION,
            world_id,
            observations,
            recalls,
        })
    }
}

fn unavailable(error: sqlx::Error) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Unavailable(error.to_string())
}

fn corrupt(message: impl Into<String>) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Corrupt(message.into())
}

fn to_u64(value: i64, field: &str) -> Result<u64, ObserverProjectionStoreError> {
    u64::try_from(value).map_err(|_| corrupt(format!("{field} is negative")))
}
