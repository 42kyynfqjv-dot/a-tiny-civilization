use application::{
    CognitionMemoryInput, MAX_COGNITION_RECALLED_MEMORIES, MEMORY_PAYLOAD_VERSION,
    MemoryOutboxEntry, MemoryOutboxStore, MemoryRecallOutcome, MemoryRecallRequest, MemoryRetain,
    MemoryRetainReceipt, StoreError,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;
use world_domain::{EntityId, EventSequence, WorldId};

use crate::PostgresStore;

#[derive(FromRow)]
struct MemoryOutboxRow {
    operation_id: Uuid,
    document_id: Uuid,
    world_id: Uuid,
    agent_id: Uuid,
    source_sequence: i64,
    bank_id: String,
    payload_version: i32,
    payload: Value,
    attempt_count: i32,
}

#[async_trait]
impl MemoryOutboxStore for PostgresStore {
    async fn claim_next_memory(
        &self,
        worker_id: &str,
        claim_lease_seconds: u32,
    ) -> Result<Option<MemoryOutboxEntry>, StoreError> {
        validate_worker_id(worker_id)?;
        let lease_seconds = i64::from(claim_lease_seconds.max(1));
        let row = sqlx::query_as::<_, MemoryOutboxRow>(
            r#"
            WITH active_ordinary_world AS (
                SELECT id
                FROM worlds
                WHERE status = 'running'
                  AND manifest -> 'experiment' IS NULL
                ORDER BY created_at DESC, id ASC
                LIMIT 1
            ),
            preferred AS (
                SELECT operation_id
                FROM memory_outbox AS delivery
                WHERE delivery.world_id = (SELECT id FROM active_ordinary_world)
                  AND delivery.completed_at IS NULL
                  AND delivery.available_at <= NOW()
                  AND (
                      delivery.claimed_at IS NULL
                      OR delivery.claimed_at < NOW() - ($2::BIGINT * INTERVAL '1 second')
                  )
                ORDER BY delivery.available_at ASC, delivery.created_at ASC,
                         delivery.operation_id ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            ),
            fallback AS (
                SELECT operation_id
                FROM memory_outbox AS delivery
                WHERE NOT EXISTS (SELECT 1 FROM preferred)
                  AND delivery.completed_at IS NULL
                  AND delivery.available_at <= NOW()
                  AND (
                      delivery.claimed_at IS NULL
                      OR delivery.claimed_at < NOW() - ($2::BIGINT * INTERVAL '1 second')
                  )
                ORDER BY delivery.available_at ASC, delivery.created_at ASC,
                         delivery.operation_id ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            ),
            candidate AS (
                SELECT operation_id FROM preferred
                UNION ALL
                SELECT operation_id FROM fallback
            )
            UPDATE memory_outbox AS delivery
            SET
                claimed_by = $1,
                claimed_at = NOW(),
                attempt_count = delivery.attempt_count + 1,
                last_error = NULL
            FROM candidate
            WHERE delivery.operation_id = candidate.operation_id
            RETURNING
                delivery.operation_id,
                delivery.document_id,
                delivery.world_id,
                delivery.agent_id,
                delivery.source_sequence,
                delivery.bank_id,
                delivery.payload_version,
                delivery.payload,
                delivery.attempt_count
            "#,
        )
        .bind(worker_id)
        .bind(lease_seconds)
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;

        row.map(parse_entry).transpose()
    }

    async fn mark_memory_accepted(
        &self,
        worker_id: &str,
        entry: &MemoryOutboxEntry,
        receipt: &MemoryRetainReceipt,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.retain.validate().map_err(corrupt)?;
        if receipt.operation_id != entry.retain.operation_id
            || receipt.remote_operation_id.trim().is_empty()
            || receipt.adapter_version.trim().is_empty()
        {
            return Err(StoreError::Conflict(
                "memory acknowledgement does not match the claimed delivery".to_owned(),
            ));
        }

        let updated = sqlx::query(
            r#"
            UPDATE memory_outbox
            SET
                completed_at = NOW(),
                remote_operation_id = $3,
                adapter_version = $4,
                last_error = NULL
            WHERE operation_id = $1
              AND claimed_by = $2
              AND completed_at IS NULL
            "#,
        )
        .bind(entry.retain.operation_id)
        .bind(worker_id)
        .bind(&receipt.remote_operation_id)
        .bind(&receipt.adapter_version)
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        require_single_update(updated.rows_affected(), entry.retain.operation_id)
    }

    async fn reschedule_memory(
        &self,
        worker_id: &str,
        entry: &MemoryOutboxEntry,
        error: &str,
        retry_after_seconds: u32,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.retain.validate().map_err(corrupt)?;
        let retry_after_seconds = i64::from(retry_after_seconds.clamp(1, 3_600));
        let error = error.chars().take(2_048).collect::<String>();
        let updated = sqlx::query(
            r#"
            UPDATE memory_outbox
            SET
                available_at = NOW() + ($3::BIGINT * INTERVAL '1 second'),
                claimed_by = NULL,
                claimed_at = NULL,
                last_error = $4
            WHERE operation_id = $1
              AND claimed_by = $2
              AND completed_at IS NULL
            "#,
        )
        .bind(entry.retain.operation_id)
        .bind(worker_id)
        .bind(retry_after_seconds)
        .bind(error)
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        require_single_update(updated.rows_affected(), entry.retain.operation_id)
    }

    async fn admit_recall_for_cognition(
        &self,
        request: &MemoryRecallRequest,
        outcome: &MemoryRecallOutcome,
    ) -> Result<Vec<CognitionMemoryInput>, StoreError> {
        outcome.validate_against(request).map_err(corrupt)?;
        let MemoryRecallOutcome::Available { results, .. } = outcome else {
            return Ok(Vec::new());
        };
        let mut admitted = Vec::with_capacity(results.len().min(MAX_COGNITION_RECALLED_MEMORIES));
        for recalled in results.iter().take(MAX_COGNITION_RECALLED_MEMORIES) {
            let payload = sqlx::query_scalar::<_, Value>(
                r#"
                SELECT payload
                FROM memory_outbox
                WHERE document_id = $1
                  AND world_id = $2
                  AND agent_id = $3
                  AND bank_id = $4
                  AND payload_version = $5
                  AND completed_at IS NOT NULL
                "#,
            )
            .bind(recalled.document_id)
            .bind(request.world_id.as_uuid())
            .bind(request.agent_id.as_uuid())
            .bind(&request.bank_id)
            .bind(i32::from(MEMORY_PAYLOAD_VERSION))
            .fetch_optional(self.pool())
            .await
            .map_err(operation_error)?
            .ok_or_else(|| {
                StoreError::Conflict(format!(
                    "recalled document {} was not accepted for this life",
                    recalled.document_id
                ))
            })?;
            let retained: MemoryRetain = serde_json::from_value(payload).map_err(corrupt)?;
            retained.validate().map_err(corrupt)?;
            if retained.document_id != recalled.document_id
                || retained.world_id != request.world_id
                || retained.agent_id != request.agent_id
                || retained.bank_id != request.bank_id
                || retained.source_sequence != recalled.source_sequence
                || retained.sim_tick != recalled.sim_tick
                || retained.ordinal != recalled.ordinal
                || retained.content != recalled.text
                || retained.context != recalled.context
            {
                return Err(StoreError::Conflict(format!(
                    "recalled document {} differs from its accepted local source",
                    recalled.document_id
                )));
            }
            admitted.push(CognitionMemoryInput {
                document_id: retained.document_id,
                source_sequence: retained.source_sequence,
                sim_tick: retained.sim_tick,
                content: retained.content,
                context: retained.context,
            });
        }
        admitted.sort_by_key(|memory| memory.document_id);
        Ok(admitted)
    }
}

fn parse_entry(row: MemoryOutboxRow) -> Result<MemoryOutboxEntry, StoreError> {
    let retain: MemoryRetain = serde_json::from_value(row.payload).map_err(corrupt)?;
    retain.validate().map_err(corrupt)?;
    let source_sequence = u64::try_from(row.source_sequence)
        .map(EventSequence::new)
        .map_err(|_| StoreError::Corrupt("negative memory source sequence".to_owned()))?;
    let payload_version = u16::try_from(row.payload_version)
        .map_err(|_| StoreError::Corrupt("invalid memory payload version".to_owned()))?;
    if retain.operation_id != row.operation_id
        || retain.document_id != row.document_id
        || retain.world_id != WorldId::from_uuid(row.world_id)
        || retain.agent_id != EntityId::from_uuid(row.agent_id)
        || retain.source_sequence != source_sequence
        || retain.bank_id != row.bank_id
        || retain.payload_version != payload_version
    {
        return Err(StoreError::Corrupt(format!(
            "memory delivery {} indexed columns disagree with its payload",
            row.operation_id
        )));
    }
    let attempt_count = u32::try_from(row.attempt_count)
        .map_err(|_| StoreError::Corrupt("invalid memory attempt count".to_owned()))?;
    Ok(MemoryOutboxEntry {
        retain,
        attempt_count,
    })
}

fn validate_worker_id(worker_id: &str) -> Result<(), StoreError> {
    if worker_id.trim().is_empty() || worker_id.len() > 128 {
        Err(StoreError::Conflict(
            "memory worker identifier must contain 1 to 128 bytes".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn require_single_update(rows: u64, operation_id: Uuid) -> Result<(), StoreError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(StoreError::Conflict(format!(
            "memory delivery {operation_id} is not held by this worker"
        )))
    }
}

fn operation_error(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(database) = &error {
        let code = database.code().as_deref().map(str::to_owned);
        if matches!(code.as_deref(), Some("23505" | "23514" | "40001" | "P0001")) {
            return StoreError::Conflict(database.message().to_owned());
        }
    }
    StoreError::Unavailable(error.to_string())
}

fn corrupt(error: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt(error.to_string())
}
