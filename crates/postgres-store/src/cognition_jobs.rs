use application::{CognitionJobEntry, CognitionJobStore, StoreError};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;
use world_domain::{
    CognitionRequestSelection, Digest, EntityId, EventId, EventSequence, SimTick, WorldId,
};

use crate::PostgresStore;

#[derive(FromRow)]
struct CognitionJobRow {
    request_id: Uuid,
    world_id: Uuid,
    agent_id: Uuid,
    source_sequence: i64,
    source_event_id: Uuid,
    source_event_index: i64,
    selected_tick: i64,
    deadline_tick: i64,
    ordinal: i64,
    selection_schema_version: i32,
    selection: Value,
    selection_checksum: Vec<u8>,
    claim_count: i64,
}

#[async_trait]
impl CognitionJobStore for PostgresStore {
    async fn claim_next_cognition_request(
        &self,
        worker_id: &str,
        claim_lease_seconds: u32,
    ) -> Result<Option<CognitionJobEntry>, StoreError> {
        validate_worker_id(worker_id)?;
        let lease_seconds = i64::from(claim_lease_seconds.max(1));
        let row = sqlx::query_as::<_, CognitionJobRow>(
            r#"
            WITH candidate AS (
                SELECT request.request_id
                FROM cognition_requests AS request
                WHERE request.available_at <= NOW()
                  AND NOT EXISTS (
                      SELECT 1
                      FROM cognition_results AS result
                      WHERE result.request_id = request.request_id
                  )
                  AND NOT EXISTS (
                      SELECT 1
                      FROM cognition_deadline_latches AS latch
                      WHERE latch.request_id = request.request_id
                  )
                  AND (
                      request.claimed_at IS NULL
                      OR request.claimed_at < NOW() - ($2::BIGINT * INTERVAL '1 second')
                  )
                ORDER BY
                    request.deadline_tick ASC,
                    request.selected_tick ASC,
                    request.request_id ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE cognition_requests AS request
            SET
                claimed_by = $1,
                claimed_at = NOW(),
                claim_count = request.claim_count + 1,
                last_error = NULL
            FROM candidate
            WHERE request.request_id = candidate.request_id
            RETURNING
                request.request_id,
                request.world_id,
                request.agent_id,
                request.source_sequence,
                request.source_event_id,
                request.source_event_index,
                request.selected_tick,
                request.deadline_tick,
                request.ordinal,
                request.selection_schema_version,
                request.selection,
                request.selection_checksum,
                request.claim_count
            "#,
        )
        .bind(worker_id)
        .bind(lease_seconds)
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;

        row.map(parse_job).transpose()
    }

    async fn reschedule_cognition_request(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        error: &str,
        retry_after_seconds: u32,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        let retry_after_seconds = i64::from(retry_after_seconds.clamp(1, 3_600));
        let error = error.chars().take(2_048).collect::<String>();
        let updated = sqlx::query(
            r#"
            UPDATE cognition_requests AS request
            SET
                available_at = NOW() + ($3::BIGINT * INTERVAL '1 second'),
                claimed_by = NULL,
                claimed_at = NULL,
                last_error = $4
            WHERE request.request_id = $1
              AND request.claimed_by = $2
              AND NOT EXISTS (
                  SELECT 1
                  FROM cognition_results AS result
                  WHERE result.request_id = request.request_id
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM cognition_deadline_latches AS latch
                  WHERE latch.request_id = request.request_id
              )
            "#,
        )
        .bind(entry.selection.request_id)
        .bind(worker_id)
        .bind(retry_after_seconds)
        .bind(error)
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        if updated.rows_affected() == 1 {
            Ok(())
        } else {
            Err(StoreError::Conflict(format!(
                "cognition request {} is not held by this worker",
                entry.selection.request_id
            )))
        }
    }
}

fn parse_job(row: CognitionJobRow) -> Result<CognitionJobEntry, StoreError> {
    let selection: CognitionRequestSelection =
        serde_json::from_value(row.selection).map_err(corrupt)?;
    selection.validate().map_err(corrupt)?;
    let source_sequence = EventSequence::new(from_i64(row.source_sequence, "source sequence")?);
    let source_event_index = u32::try_from(row.source_event_index)
        .map_err(|_| StoreError::Corrupt("invalid cognition source event index".to_owned()))?;
    let selected_tick = SimTick::new(from_i64(row.selected_tick, "selected tick")?);
    let deadline_tick = SimTick::new(from_i64(row.deadline_tick, "deadline tick")?);
    let ordinal = u32::try_from(row.ordinal)
        .map_err(|_| StoreError::Corrupt("invalid cognition ordinal".to_owned()))?;
    let selection_schema_version = u16::try_from(row.selection_schema_version)
        .map_err(|_| StoreError::Corrupt("invalid cognition selection schema".to_owned()))?;
    let claim_count = u32::try_from(row.claim_count)
        .map_err(|_| StoreError::Corrupt("invalid cognition claim count".to_owned()))?;
    let stored_checksum = digest_from_db(&row.selection_checksum, "selection checksum")?;
    if selection.request_id != row.request_id
        || selection.world_id != WorldId::from_uuid(row.world_id)
        || selection.organism_id != EntityId::from_uuid(row.agent_id)
        || selection.selected_at_tick != selected_tick
        || selection.deadline_tick != deadline_tick
        || selection.ordinal != ordinal
        || selection.schema_version != selection_schema_version
        || selection.canonical_hash().map_err(corrupt)? != stored_checksum
    {
        return Err(StoreError::Corrupt(format!(
            "cognition request {} indexed columns disagree with its selection",
            row.request_id
        )));
    }
    let entry = CognitionJobEntry {
        selection,
        source_sequence,
        source_event_id: EventId::from_uuid(row.source_event_id),
        source_event_index,
        claim_count,
    };
    entry.validate().map_err(corrupt)?;
    Ok(entry)
}

fn validate_worker_id(worker_id: &str) -> Result<(), StoreError> {
    if worker_id.trim().is_empty() || worker_id.len() > 128 {
        Err(StoreError::Conflict(
            "cognition worker identifier must contain 1 to 128 bytes".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn digest_from_db(bytes: &[u8], field: &str) -> Result<Digest, StoreError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
        StoreError::Corrupt(format!("{field} has {} bytes instead of 32", bytes.len()))
    })?;
    Ok(Digest::from_bytes(bytes))
}

fn from_i64(value: i64, field: &str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Corrupt(format!("{field} is negative")))
}

fn operation_error(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(database) = &error {
        let code = database.code().as_deref().map(str::to_owned);
        if matches!(
            code.as_deref(),
            Some("23503" | "23505" | "23514" | "40001" | "P0001")
        ) {
            return StoreError::Conflict(database.message().to_owned());
        }
    }
    StoreError::Unavailable(error.to_string())
}

fn corrupt(error: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt(error.to_string())
}
