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

const DIRECT_OBSERVATION_CONTEXT_V1: &str = "canonical-direct-perception-v1";
const DIRECT_OBSERVATION_CONTEXT_V2: &str = "canonical-direct-perception-episode-v2";

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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectObservationEpisode {
    subject_id: Option<EntityId>,
    channel: PerceptionChannel,
    property_code: String,
    quantized_value: i32,
    uncertainty: u16,
    prior_quantized_value: Option<i32>,
    prior_uncertainty: Option<u16>,
    prior_observed_at: Option<SimTick>,
    sampling_reason: EpisodicSamplingReason,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum EpisodicSamplingReason {
    NewAddress,
    AcousticChange,
    MeaningfulChange,
    PeriodicRefresh,
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
                  AND payload->>'context' IN (
                      'canonical-direct-perception-v1',
                      'canonical-direct-perception-episode-v2'
                  )
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
            if retained.world_id != world_id
                || !matches!(
                    retained.context.as_str(),
                    DIRECT_OBSERVATION_CONTEXT_V1 | DIRECT_OBSERVATION_CONTEXT_V2
                )
            {
                return Err(corrupt(
                    "memory row crossed its world or public direct-observation boundary",
                ));
            }
            let direct = parse_direct_observation(&retained)?;
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

fn parse_direct_observation(
    retained: &MemoryRetain,
) -> Result<DirectObservation, ObserverProjectionStoreError> {
    match retained.context.as_str() {
        DIRECT_OBSERVATION_CONTEXT_V1 => serde_json::from_str(&retained.content)
            .map_err(|error| corrupt(format!("invalid direct observation: {error}"))),
        DIRECT_OBSERVATION_CONTEXT_V2 => {
            let episode: DirectObservationEpisode = serde_json::from_str(&retained.content)
                .map_err(|error| corrupt(format!("invalid direct-observation episode: {error}")))?;
            let prior_fields = [
                episode.prior_quantized_value.is_some(),
                episode.prior_uncertainty.is_some(),
                episode.prior_observed_at.is_some(),
            ];
            let valid_prior_shape = match episode.sampling_reason {
                EpisodicSamplingReason::NewAddress => prior_fields.iter().all(|present| !present),
                EpisodicSamplingReason::AcousticChange
                | EpisodicSamplingReason::MeaningfulChange
                | EpisodicSamplingReason::PeriodicRefresh => {
                    prior_fields.iter().all(|present| *present)
                }
            };
            if !valid_prior_shape {
                return Err(corrupt(
                    "direct-observation episode has inconsistent prior-reading provenance",
                ));
            }
            Ok(DirectObservation {
                subject_id: episode.subject_id,
                channel: episode.channel,
                property_code: episode.property_code,
                quantized_value: episode.quantized_value,
                uncertainty: episode.uncertainty,
            })
        }
        _ => Err(corrupt("memory context is not a public direct observation")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{EventSequence, WorldId};

    #[test]
    fn public_memory_parser_accepts_bounded_v2_episodes() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0xE9150DE));
        let agent_id = EntityId::from_uuid(Uuid::from_u128(0xA6E17));
        let retained = MemoryRetain::new(
            world_id,
            agent_id,
            EventSequence::new(9),
            SimTick::new(8),
            0,
            r#"{"subject_id":null,"channel":"sound","property_code":"signal_amplitude","quantized_value":12,"uncertainty":1,"prior_quantized_value":7,"prior_uncertainty":2,"prior_observed_at":"4","sampling_reason":"acoustic_change"}"#,
            DIRECT_OBSERVATION_CONTEXT_V2,
        )
        .expect("valid v2 episode");

        let parsed = parse_direct_observation(&retained).expect("public v2 observation");
        assert_eq!(parsed.subject_id, None);
        assert_eq!(parsed.channel, PerceptionChannel::Sound);
        assert_eq!(parsed.property_code, "signal_amplitude");
        assert_eq!(parsed.quantized_value, 12);
        assert_eq!(parsed.uncertainty, 1);
    }

    #[test]
    fn public_memory_parser_keeps_v1_strict() {
        let retained = MemoryRetain::new(
            WorldId::from_uuid(Uuid::from_u128(0xE9150DF)),
            EntityId::from_uuid(Uuid::from_u128(0xA6E18)),
            EventSequence::new(1),
            SimTick::ZERO,
            0,
            r#"{"subject_id":null,"channel":"touch","property_code":"temperature","quantized_value":1,"uncertainty":0,"unexpected":true}"#,
            DIRECT_OBSERVATION_CONTEXT_V1,
        )
        .expect("retain contract accepts opaque content");
        assert!(parse_direct_observation(&retained).is_err());
    }
}
