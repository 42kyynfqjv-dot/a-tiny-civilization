use application::{
    COGNITION_HARD_STOP_MICRO_USD_PER_MONTH, COGNITION_TARGET_MICRO_USD_PER_MONTH,
    CognitionAttemptPersistenceState, CognitionBillingClass, CognitionJobEntry, CognitionJobStore,
    CognitionModelRoute, CognitionRecallRecord, CognitionRouteAttempt, CognitionRouteAttemptRecord,
    CognitionRouteAttemptStatus, CognitionRoutePurpose, CognitionRouteRegistry,
    MAX_PAID_COGNITION_RESERVATION_MICRO_USD, MemoryOutboxStore, ModelCognitionLadderResult,
    ModelCognitionReceipt, ModelCognitionRequest, PaidCognitionAuthorization,
    PaidCognitionReservationDecision, StoreError, is_network_terminal_status, is_skip_status,
};
use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::Value;
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;
use world_domain::{
    CognitionDeadlineInput, CognitionInputOutcome, CognitionModelEvidence,
    CognitionRequestSelection, CognitionUnavailableReason, Digest, EntityId, EventId,
    EventSequence, SimTick, WorldId,
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

#[derive(FromRow)]
struct CognitionRecallRow {
    recall_request: Value,
    recall_request_checksum: Vec<u8>,
    recall_outcome: Value,
    recall_outcome_checksum: Vec<u8>,
    admitted_memory_inputs: Value,
    admitted_memory_inputs_checksum: Vec<u8>,
}

#[derive(FromRow)]
struct CognitionAttemptRow {
    route_index: i32,
    provider_slug: String,
    requested_model: String,
    billing_class: String,
    dispatch_state: String,
    normalized_status: Option<String>,
    attempt_payload: Option<Value>,
    attempt_checksum: Option<Vec<u8>>,
    receipt_payload: Option<Value>,
}

#[derive(FromRow)]
struct CostReservationRow {
    billing_month: NaiveDate,
    reserved_micro_usd: i64,
    status: String,
    actual_micro_usd: Option<i64>,
}

#[derive(FromRow)]
struct DueCognitionRow {
    request_id: Uuid,
    deadline_tick: i64,
    selection: Value,
    selection_checksum: Vec<u8>,
    recall_outcome_checksum: Option<Vec<u8>>,
    route_registry_checksum: Option<Vec<u8>>,
    result_payload: Option<Value>,
    result_checksum: Option<Vec<u8>>,
    latch_world_id: Option<Uuid>,
    latch_deadline_tick: Option<i64>,
    latch_target_sequence: Option<i64>,
    latch_kind: Option<String>,
    latch_payload: Option<Value>,
    latch_checksum: Option<Vec<u8>>,
}

#[async_trait]
impl CognitionJobStore for PostgresStore {
    async fn latch_due_cognition_inputs(
        &self,
        world_id: WorldId,
        target_sequence: EventSequence,
        target_tick: SimTick,
    ) -> Result<Vec<CognitionDeadlineInput>, StoreError> {
        if target_sequence == EventSequence::ZERO || target_tick == SimTick::ZERO {
            return Err(StoreError::Conflict(
                "cognition latch target must be a non-genesis transition".to_owned(),
            ));
        }
        let target_sequence_i64 = to_i64(target_sequence.get(), "cognition target sequence")?;
        let target_tick_i64 = to_i64(target_tick.get(), "cognition target tick")?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        let world = sqlx::query_as::<_, (String, i64, i64)>(
            r#"
            SELECT status, current_tick, current_sequence
            FROM worlds
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(world_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(operation_error)?
        .ok_or_else(|| StoreError::NotFound(format!("world {world_id}")))?;
        if world.0 != "running"
            || world.1.checked_add(1) != Some(target_tick_i64)
            || world.2.checked_add(1) != Some(target_sequence_i64)
        {
            return Err(StoreError::Conflict(
                "cognition latch target is not the world's next running transition".to_owned(),
            ));
        }

        let rows = sqlx::query_as::<_, DueCognitionRow>(
            r#"
            SELECT
                request.request_id,
                request.deadline_tick,
                request.selection,
                request.selection_checksum,
                recall.recall_outcome_checksum,
                result.route_registry_checksum,
                result.result_payload,
                result.result_checksum,
                latch.world_id AS latch_world_id,
                latch.deadline_tick AS latch_deadline_tick,
                latch.target_sequence AS latch_target_sequence,
                latch.latch_kind,
                latch.latch_payload,
                latch.latch_checksum
            FROM cognition_requests AS request
            LEFT JOIN cognition_recall_outcomes AS recall
                ON recall.request_id = request.request_id
            LEFT JOIN cognition_results AS result
                ON result.request_id = request.request_id
            LEFT JOIN cognition_deadline_latches AS latch
                ON latch.request_id = request.request_id
            LEFT JOIN cognition_latch_consumptions AS consumption
                ON consumption.request_id = request.request_id
            WHERE request.world_id = $1
              AND request.deadline_tick <= $2
              AND consumption.request_id IS NULL
            ORDER BY request.request_id ASC
            FOR UPDATE OF request
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(target_tick_i64)
        .fetch_all(&mut *transaction)
        .await
        .map_err(operation_error)?;

        let mut inputs = Vec::with_capacity(rows.len());
        for row in rows {
            if row.deadline_tick != target_tick_i64 {
                return Err(StoreError::Corrupt(format!(
                    "cognition request {} passed its deadline without consumption",
                    row.request_id
                )));
            }
            let selection = parse_due_selection(&row)?;
            let input = if let Some(payload) = row.latch_payload.clone() {
                parse_existing_latch(
                    &row,
                    &selection,
                    world_id,
                    target_sequence,
                    target_tick,
                    payload,
                )?
            } else {
                if row.latch_target_sequence.is_some()
                    || row.latch_world_id.is_some()
                    || row.latch_deadline_tick.is_some()
                    || row.latch_kind.is_some()
                    || row.latch_checksum.is_some()
                {
                    return Err(StoreError::Corrupt(
                        "cognition latch columns are only partially present".to_owned(),
                    ));
                }
                let input = build_deadline_input(&row, &selection)?;
                let input_json = serde_json::to_value(&input).map_err(corrupt)?;
                let input_checksum = input.canonical_hash().map_err(corrupt)?;
                let latch_kind = match &input.outcome {
                    CognitionInputOutcome::Model(_) => "model_result",
                    CognitionInputOutcome::Unavailable { .. } => "unavailable",
                };
                sqlx::query(
                    r#"
                    INSERT INTO cognition_deadline_latches (
                        request_id,
                        world_id,
                        deadline_tick,
                        target_sequence,
                        latch_kind,
                        latch_payload,
                        latch_checksum
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    "#,
                )
                .bind(input.request_id)
                .bind(world_id.as_uuid())
                .bind(target_tick_i64)
                .bind(target_sequence_i64)
                .bind(latch_kind)
                .bind(input_json)
                .bind(input_checksum.as_bytes().as_slice())
                .execute(&mut *transaction)
                .await
                .map_err(operation_error)?;
                input
            };
            inputs.push(input);
        }
        transaction.commit().await.map_err(operation_error)?;
        Ok(inputs)
    }

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
                  AND (
                      NOT EXISTS (
                          SELECT 1
                          FROM cognition_deadline_latches AS latch
                          WHERE latch.request_id = request.request_id
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM cognition_route_attempts AS attempt
                          WHERE attempt.request_id = request.request_id
                            AND attempt.dispatch_state = 'dispatched'
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM cognition_cost_reservations AS reservation
                          WHERE reservation.request_id = request.request_id
                            AND reservation.status = 'reserved'
                      )
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

    async fn cognition_deadline_is_latched(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<bool, StoreError> {
        entry.validate().map_err(corrupt)?;
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM cognition_deadline_latches WHERE request_id = $1)",
        )
        .bind(entry.selection.request_id)
        .fetch_one(self.pool())
        .await
        .map_err(operation_error)
    }

    async fn record_cognition_recall(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        recall: &CognitionRecallRecord,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        recall.validate_against(entry).map_err(corrupt)?;
        let admitted = self
            .admit_recall_for_cognition(&recall.request, &recall.outcome)
            .await?;
        if admitted != recall.admitted_memories {
            return Err(StoreError::Conflict(
                "cognition recall differs from accepted local memory provenance".to_owned(),
            ));
        }
        let request_json = serde_json::to_value(&recall.request).map_err(corrupt)?;
        let request_checksum = recall.request.canonical_hash().map_err(corrupt)?;
        let outcome_json = serde_json::to_value(&recall.outcome).map_err(corrupt)?;
        let outcome_checksum = Digest::canonical(&recall.outcome).map_err(corrupt)?;
        let memories_json = serde_json::to_value(&recall.admitted_memories).map_err(corrupt)?;
        let memories_checksum = Digest::canonical(&recall.admitted_memories).map_err(corrupt)?;

        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.selection.request_id).await?;
        sqlx::query(
            r#"
            INSERT INTO cognition_recall_outcomes (
                request_id,
                recall_request,
                recall_request_checksum,
                recall_outcome,
                recall_outcome_checksum,
                admitted_memory_inputs,
                admitted_memory_inputs_checksum
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(entry.selection.request_id)
        .bind(request_json)
        .bind(request_checksum.as_bytes().as_slice())
        .bind(outcome_json)
        .bind(outcome_checksum.as_bytes().as_slice())
        .bind(memories_json)
        .bind(memories_checksum.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn load_cognition_recall(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<Option<CognitionRecallRecord>, StoreError> {
        entry.validate().map_err(corrupt)?;
        let row = sqlx::query_as::<_, CognitionRecallRow>(
            r#"
            SELECT
                recall_request,
                recall_request_checksum,
                recall_outcome,
                recall_outcome_checksum,
                admitted_memory_inputs,
                admitted_memory_inputs_checksum
            FROM cognition_recall_outcomes
            WHERE request_id = $1
            "#,
        )
        .bind(entry.selection.request_id)
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        row.map(|row| parse_recall(row, entry)).transpose()
    }

    async fn begin_cognition_route_attempt(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        route_index: u16,
        route: &CognitionModelRoute,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        route.validate().map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.selection.request_id).await?;
        require_recorded_recall(&mut transaction, entry.selection.request_id).await?;
        if route.billing_class == CognitionBillingClass::PaidApproved {
            let reserved = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM cognition_cost_reservations
                    WHERE request_id = $1
                      AND status = 'reserved'
                )
                "#,
            )
            .bind(entry.selection.request_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(operation_error)?;
            if !reserved {
                return Err(StoreError::Conflict(
                    "paid cognition dispatch requires an active durable reservation".to_owned(),
                ));
            }
        }
        sqlx::query(
            r#"
            INSERT INTO cognition_route_attempts (
                request_id,
                route_index,
                provider_slug,
                requested_model,
                billing_class,
                dispatch_state,
                network_dispatched,
                dispatched_at
            )
            VALUES ($1, $2, $3, $4, $5, 'dispatched', TRUE, NOW())
            "#,
        )
        .bind(entry.selection.request_id)
        .bind(i32::from(route_index))
        .bind(route.provider.as_str())
        .bind(&route.requested_model)
        .bind(billing_class_text(route.billing_class))
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn record_cognition_route_skip(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        attempt: &CognitionRouteAttempt,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        if !is_skip_status(attempt.status) {
            return Err(StoreError::Conflict(
                "a non-skip cognition status cannot be inserted without dispatch".to_owned(),
            ));
        }
        let route = route_from_attempt(attempt)?;
        let attempt_json = serde_json::to_value(attempt).map_err(corrupt)?;
        let attempt_checksum = Digest::canonical(attempt).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.selection.request_id).await?;
        require_recorded_recall(&mut transaction, entry.selection.request_id).await?;
        sqlx::query(
            r#"
            INSERT INTO cognition_route_attempts (
                request_id,
                route_index,
                provider_slug,
                requested_model,
                billing_class,
                dispatch_state,
                network_dispatched,
                normalized_status,
                attempt_payload,
                attempt_checksum,
                completed_at
            )
            VALUES ($1, $2, $3, $4, $5, 'skipped', FALSE, $6, $7, $8, NOW())
            "#,
        )
        .bind(entry.selection.request_id)
        .bind(i32::from(attempt.route_index))
        .bind(route.provider.as_str())
        .bind(&route.requested_model)
        .bind(billing_class_text(route.billing_class))
        .bind(attempt_status_text(attempt.status))
        .bind(attempt_json)
        .bind(attempt_checksum.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn finish_cognition_route_attempt(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        request: &ModelCognitionRequest,
        attempt: &CognitionRouteAttempt,
        receipt: Option<&ModelCognitionReceipt>,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        validate_model_request_for_job(self, entry, request).await?;
        if !is_network_terminal_status(attempt.status) {
            return Err(StoreError::Conflict(
                "network attempt requires a terminal network status".to_owned(),
            ));
        }
        let route = route_from_attempt(attempt)?;
        match (attempt.status, receipt) {
            (CognitionRouteAttemptStatus::Succeeded, Some(receipt)) => {
                receipt.validate_against(&route, request).map_err(corrupt)?;
            }
            (CognitionRouteAttemptStatus::Succeeded, None) => {
                return Err(StoreError::Conflict(
                    "successful cognition attempt omitted its receipt".to_owned(),
                ));
            }
            (_, Some(_)) => {
                return Err(StoreError::Conflict(
                    "failed cognition attempt cannot carry a receipt".to_owned(),
                ));
            }
            (_, None) => {}
        }
        finish_attempt(
            self,
            worker_id,
            entry,
            attempt,
            receipt,
            CognitionAttemptPersistenceState::Completed,
        )
        .await
    }

    async fn abandon_cognition_route_attempt(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        route_index: u16,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        let records = self.list_cognition_route_attempts(entry).await?;
        let record = records
            .iter()
            .find(|record| record.route_index == route_index)
            .ok_or_else(|| {
                StoreError::Conflict(format!(
                    "cognition route attempt {route_index} does not exist"
                ))
            })?;
        if record.persistence_state != CognitionAttemptPersistenceState::Dispatched {
            return Err(StoreError::Conflict(format!(
                "cognition route attempt {route_index} is not in flight"
            )));
        }
        let attempt = CognitionRouteAttempt {
            route_index,
            provider: record.route.provider.clone(),
            requested_model: record.route.requested_model.clone(),
            billing_class: record.route.billing_class,
            status: CognitionRouteAttemptStatus::Unavailable,
        };
        finish_attempt(
            self,
            worker_id,
            entry,
            &attempt,
            None,
            CognitionAttemptPersistenceState::Abandoned,
        )
        .await
    }

    async fn list_cognition_route_attempts(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<Vec<CognitionRouteAttemptRecord>, StoreError> {
        entry.validate().map_err(corrupt)?;
        let rows = sqlx::query_as::<_, CognitionAttemptRow>(
            r#"
            SELECT
                route_index,
                provider_slug,
                requested_model,
                billing_class,
                dispatch_state,
                normalized_status,
                attempt_payload,
                attempt_checksum,
                receipt_payload
            FROM cognition_route_attempts
            WHERE request_id = $1
            ORDER BY route_index ASC
            "#,
        )
        .bind(entry.selection.request_id)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        rows.into_iter().map(parse_attempt).collect()
    }

    async fn complete_cognition_request(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        registry: &CognitionRouteRegistry,
        purpose: CognitionRoutePurpose,
        request: &ModelCognitionRequest,
        result: &ModelCognitionLadderResult,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        validate_model_request_for_job(self, entry, request).await?;
        result
            .validate_against(registry, purpose, request)
            .map_err(corrupt)?;
        let persisted = self.list_cognition_route_attempts(entry).await?;
        if persisted
            .iter()
            .any(|record| record.persistence_state == CognitionAttemptPersistenceState::Dispatched)
        {
            return Err(StoreError::Conflict(
                "cannot complete cognition while a network attempt is in flight".to_owned(),
            ));
        }
        let persisted_attempts = persisted
            .iter()
            .map(|record| {
                record.attempt.clone().ok_or_else(|| {
                    StoreError::Corrupt("terminal cognition attempt omitted payload".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let persisted_receipt = persisted
            .iter()
            .find_map(|record| record.receipt.as_ref())
            .cloned();
        if persisted_attempts != result.attempts || persisted_receipt != result.receipt {
            return Err(StoreError::Conflict(
                "cognition result differs from its durable route-attempt prefix".to_owned(),
            ));
        }
        let result_json = serde_json::to_value(result).map_err(corrupt)?;
        let result_checksum = Digest::canonical(result).map_err(corrupt)?;
        let registry_checksum = registry.canonical_hash(purpose).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.selection.request_id).await?;
        sqlx::query(
            r#"
            INSERT INTO cognition_results (
                request_id,
                route_policy_version,
                route_registry_checksum,
                result_payload,
                result_checksum
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(entry.selection.request_id)
        .bind(i32::from(registry.policy_version))
        .bind(registry_checksum.as_bytes().as_slice())
        .bind(result_json)
        .bind(result_checksum.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn reserve_paid_cognition(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        route: &CognitionModelRoute,
        reserved_micro_usd: u64,
    ) -> Result<PaidCognitionReservationDecision, StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        route.validate().map_err(corrupt)?;
        if route.billing_class != CognitionBillingClass::PaidApproved
            || reserved_micro_usd == 0
            || reserved_micro_usd > MAX_PAID_COGNITION_RESERVATION_MICRO_USD
        {
            return Err(StoreError::Conflict(
                "paid cognition reservation is outside the approved route or per-call cap"
                    .to_owned(),
            ));
        }
        let reserved_i64 = to_i64(reserved_micro_usd, "paid cognition reservation")?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.selection.request_id).await?;
        require_recorded_recall(&mut transaction, entry.selection.request_id).await?;
        let billing_month: NaiveDate =
            sqlx::query_scalar("SELECT date_trunc('month', CURRENT_DATE)::DATE")
                .fetch_one(&mut *transaction)
                .await
                .map_err(operation_error)?;
        sqlx::query(
            r#"
            INSERT INTO cognition_cost_accounts (
                billing_month,
                target_micro_usd,
                hard_stop_micro_usd
            )
            VALUES ($1, $2, $3)
            ON CONFLICT (billing_month) DO NOTHING
            "#,
        )
        .bind(billing_month)
        .bind(to_i64(
            COGNITION_TARGET_MICRO_USD_PER_MONTH,
            "monthly cognition target",
        )?)
        .bind(to_i64(
            COGNITION_HARD_STOP_MICRO_USD_PER_MONTH,
            "monthly cognition hard stop",
        )?)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        let existing =
            fetch_cost_reservation(&mut transaction, entry.selection.request_id, false).await?;
        if let Some(existing) = existing {
            if existing.billing_month == billing_month
                && existing.reserved_micro_usd == reserved_i64
                && existing.status == "reserved"
            {
                transaction.commit().await.map_err(operation_error)?;
                return Ok(PaidCognitionReservationDecision::Authorized(
                    PaidCognitionAuthorization {
                        request_id: entry.selection.request_id,
                        billing_month,
                        reserved_micro_usd,
                    },
                ));
            }
            return Err(StoreError::Conflict(
                "cognition request already has a different paid reservation".to_owned(),
            ));
        }
        let account = sqlx::query_as::<_, (i64, i64, i64)>(
            r#"
            SELECT reserved_micro_usd, spent_micro_usd, hard_stop_micro_usd
            FROM cognition_cost_accounts
            WHERE billing_month = $1
            FOR UPDATE
            "#,
        )
        .bind(billing_month)
        .fetch_one(&mut *transaction)
        .await
        .map_err(operation_error)?;
        let would_use = account
            .0
            .checked_add(account.1)
            .and_then(|used| used.checked_add(reserved_i64))
            .ok_or_else(|| {
                StoreError::Conflict("cognition cost arithmetic overflowed".to_owned())
            })?;
        if would_use > account.2 {
            transaction.commit().await.map_err(operation_error)?;
            return Ok(PaidCognitionReservationDecision::DeniedHardStop);
        }
        sqlx::query(
            r#"
            INSERT INTO cognition_cost_reservations (
                request_id,
                billing_month,
                reserved_micro_usd,
                status
            )
            VALUES ($1, $2, $3, 'reserved')
            "#,
        )
        .bind(entry.selection.request_id)
        .bind(billing_month)
        .bind(reserved_i64)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        sqlx::query(
            r#"
            UPDATE cognition_cost_accounts
            SET reserved_micro_usd = reserved_micro_usd + $2, updated_at = NOW()
            WHERE billing_month = $1
            "#,
        )
        .bind(billing_month)
        .bind(reserved_i64)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)?;
        Ok(PaidCognitionReservationDecision::Authorized(
            PaidCognitionAuthorization {
                request_id: entry.selection.request_id,
                billing_month,
                reserved_micro_usd,
            },
        ))
    }

    async fn load_paid_cognition_authorization(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<Option<PaidCognitionAuthorization>, StoreError> {
        entry.validate().map_err(corrupt)?;
        let row = sqlx::query_as::<_, CostReservationRow>(
            r#"
            SELECT billing_month, reserved_micro_usd, status, actual_micro_usd
            FROM cognition_cost_reservations
            WHERE request_id = $1
            "#,
        )
        .bind(entry.selection.request_id)
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.status != "reserved" {
            return Ok(None);
        }
        if row.actual_micro_usd.is_some() {
            return Err(StoreError::Corrupt(
                "active cognition reservation unexpectedly has an actual cost".to_owned(),
            ));
        }
        let reserved_micro_usd = u64::try_from(row.reserved_micro_usd)
            .map_err(|_| StoreError::Corrupt("negative cognition reservation amount".to_owned()))?;
        let authorization = PaidCognitionAuthorization {
            request_id: entry.selection.request_id,
            billing_month: row.billing_month,
            reserved_micro_usd,
        };
        authorization.validate_against(entry).map_err(corrupt)?;
        Ok(Some(authorization))
    }

    async fn settle_paid_cognition(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        authorization: &PaidCognitionAuthorization,
        receipt: &ModelCognitionReceipt,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        authorization.validate_against(entry).map_err(corrupt)?;
        if receipt.request_id != entry.selection.request_id
            || receipt.billed_micro_usd > authorization.reserved_micro_usd
        {
            return Err(StoreError::Conflict(
                "paid cognition receipt exceeds or differs from its reservation".to_owned(),
            ));
        }
        let receipt_json = serde_json::to_value(receipt).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_external_resolution_claim(&mut transaction, worker_id, entry.selection.request_id)
            .await?;
        let persisted_receipt = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT receipt_payload
            FROM cognition_route_attempts
            WHERE request_id = $1
              AND billing_class = 'paid_approved'
              AND dispatch_state = 'completed'
              AND normalized_status = 'succeeded'
            "#,
        )
        .bind(entry.selection.request_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(operation_error)?;
        if persisted_receipt.as_ref() != Some(&receipt_json) {
            return Err(StoreError::Conflict(
                "paid cognition receipt is not the durable successful attempt".to_owned(),
            ));
        }
        resolve_paid_reservation(
            &mut transaction,
            authorization,
            PaidReservationResolution::Settled(receipt.billed_micro_usd),
        )
        .await?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn release_paid_cognition(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        authorization: &PaidCognitionAuthorization,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        authorization.validate_against(entry).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_external_resolution_claim(&mut transaction, worker_id, entry.selection.request_id)
            .await?;
        let dispatched =
            paid_attempt_was_dispatched(&mut transaction, authorization.request_id).await?;
        if dispatched {
            return Err(StoreError::Conflict(
                "a dispatched paid call cannot release its reservation".to_owned(),
            ));
        }
        resolve_paid_reservation(
            &mut transaction,
            authorization,
            PaidReservationResolution::Released,
        )
        .await?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn mark_paid_cognition_indeterminate(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        authorization: &PaidCognitionAuthorization,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        authorization.validate_against(entry).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_external_resolution_claim(&mut transaction, worker_id, entry.selection.request_id)
            .await?;
        if !paid_attempt_was_dispatched(&mut transaction, authorization.request_id).await? {
            return Err(StoreError::Conflict(
                "an undispatched paid call cannot become billing-indeterminate".to_owned(),
            ));
        }
        resolve_paid_reservation(
            &mut transaction,
            authorization,
            PaidReservationResolution::Indeterminate,
        )
        .await?;
        transaction.commit().await.map_err(operation_error)
    }
}

fn parse_due_selection(row: &DueCognitionRow) -> Result<CognitionRequestSelection, StoreError> {
    let selection: CognitionRequestSelection =
        serde_json::from_value(row.selection.clone()).map_err(corrupt)?;
    selection.validate().map_err(corrupt)?;
    if selection.request_id != row.request_id
        || to_i64(selection.deadline_tick.get(), "cognition deadline tick")? != row.deadline_tick
        || selection.canonical_hash().map_err(corrupt)?
            != digest_from_db(&row.selection_checksum, "selection checksum")?
    {
        return Err(StoreError::Corrupt(format!(
            "due cognition request {} disagrees with its selection",
            row.request_id
        )));
    }
    Ok(selection)
}

fn build_deadline_input(
    row: &DueCognitionRow,
    selection: &CognitionRequestSelection,
) -> Result<CognitionDeadlineInput, StoreError> {
    let recall_outcome_hash = row
        .recall_outcome_checksum
        .as_deref()
        .map(|bytes| digest_from_db(bytes, "recall outcome checksum"))
        .transpose()?
        .unwrap_or(Digest::ZERO);
    match (
        &row.result_payload,
        &row.result_checksum,
        &row.route_registry_checksum,
    ) {
        (None, None, None) => CognitionDeadlineInput::unavailable(
            selection,
            recall_outcome_hash,
            Digest::ZERO,
            Digest::ZERO,
            CognitionUnavailableReason::DeadlineNoResult,
        )
        .map_err(corrupt),
        (Some(payload), Some(result_checksum), Some(registry_checksum)) => {
            let result: ModelCognitionLadderResult =
                serde_json::from_value(payload.clone()).map_err(corrupt)?;
            let result_hash = digest_from_db(result_checksum, "cognition result checksum")?;
            let route_registry_hash = digest_from_db(registry_checksum, "route registry checksum")?;
            if result.request_id != selection.request_id
                || result.route_registry_hash != route_registry_hash
                || Digest::canonical(&result).map_err(corrupt)? != result_hash
            {
                return Err(StoreError::Corrupt(
                    "cognition result disagrees with its immutable checksums".to_owned(),
                ));
            }
            if let Some(receipt) = &result.receipt {
                if recall_outcome_hash == Digest::ZERO {
                    return Err(StoreError::Corrupt(
                        "successful cognition result has no recorded recall outcome".to_owned(),
                    ));
                }
                CognitionDeadlineInput::model(
                    selection,
                    recall_outcome_hash,
                    route_registry_hash,
                    result_hash,
                    CognitionModelEvidence {
                        provider_slug: receipt.provider.as_str().to_owned(),
                        requested_model: receipt.requested_model.clone(),
                        resolved_model: receipt.resolved_model.clone(),
                        provider_response_hash: receipt.provider_response_hash,
                        adapter_version: receipt.adapter_version.clone(),
                        prompt_tokens: receipt.usage.prompt_tokens,
                        completion_tokens: receipt.usage.completion_tokens,
                        billed_micro_usd: receipt.billed_micro_usd,
                        action_kind: receipt.action_kind,
                        contact_region: receipt.contact_region,
                        signal_intensity: receipt.signal_intensity,
                    },
                )
                .map_err(corrupt)
            } else {
                let reason = if result.attempts.iter().any(|attempt| {
                    attempt.status == CognitionRouteAttemptStatus::SkippedPaidUnauthorized
                }) {
                    CognitionUnavailableReason::BudgetDenied
                } else {
                    CognitionUnavailableReason::LadderExhausted
                };
                CognitionDeadlineInput::unavailable(
                    selection,
                    recall_outcome_hash,
                    route_registry_hash,
                    result_hash,
                    reason,
                )
                .map_err(corrupt)
            }
        }
        _ => Err(StoreError::Corrupt(
            "cognition result payload and checksums are only partially present".to_owned(),
        )),
    }
}

fn parse_existing_latch(
    row: &DueCognitionRow,
    selection: &CognitionRequestSelection,
    world_id: WorldId,
    target_sequence: EventSequence,
    target_tick: SimTick,
    payload: Value,
) -> Result<CognitionDeadlineInput, StoreError> {
    let input: CognitionDeadlineInput = serde_json::from_value(payload).map_err(corrupt)?;
    input.validate_against(selection).map_err(corrupt)?;
    let expected_kind = match &input.outcome {
        CognitionInputOutcome::Model(_) => "model_result",
        CognitionInputOutcome::Unavailable { .. } => "unavailable",
    };
    let checksum = row
        .latch_checksum
        .as_deref()
        .ok_or_else(|| StoreError::Corrupt("cognition latch checksum is missing".to_owned()))?;
    if row.latch_world_id != Some(world_id.as_uuid())
        || row.latch_deadline_tick != Some(to_i64(target_tick.get(), "cognition target tick")?)
        || row.latch_target_sequence
            != Some(to_i64(target_sequence.get(), "cognition target sequence")?)
        || row.latch_kind.as_deref() != Some(expected_kind)
        || input.canonical_hash().map_err(corrupt)?
            != digest_from_db(checksum, "cognition latch checksum")?
    {
        return Err(StoreError::Corrupt(
            "cognition deadline latch disagrees with its indexed columns".to_owned(),
        ));
    }
    Ok(input)
}

fn parse_recall(
    row: CognitionRecallRow,
    entry: &CognitionJobEntry,
) -> Result<CognitionRecallRecord, StoreError> {
    let request = serde_json::from_value(row.recall_request).map_err(corrupt)?;
    let outcome = serde_json::from_value(row.recall_outcome).map_err(corrupt)?;
    let admitted_memories = serde_json::from_value(row.admitted_memory_inputs).map_err(corrupt)?;
    let recall = CognitionRecallRecord {
        request,
        outcome,
        admitted_memories,
    };
    recall.validate_against(entry).map_err(corrupt)?;
    if recall.request.canonical_hash().map_err(corrupt)?
        != digest_from_db(&row.recall_request_checksum, "recall request checksum")?
        || Digest::canonical(&recall.outcome).map_err(corrupt)?
            != digest_from_db(&row.recall_outcome_checksum, "recall outcome checksum")?
        || Digest::canonical(&recall.admitted_memories).map_err(corrupt)?
            != digest_from_db(
                &row.admitted_memory_inputs_checksum,
                "admitted memory checksum",
            )?
    {
        return Err(StoreError::Corrupt(
            "cognition recall indexed checksums disagree with its payload".to_owned(),
        ));
    }
    Ok(recall)
}

fn parse_attempt(row: CognitionAttemptRow) -> Result<CognitionRouteAttemptRecord, StoreError> {
    let route_index = u16::try_from(row.route_index)
        .map_err(|_| StoreError::Corrupt("invalid cognition route index".to_owned()))?;
    let route = CognitionModelRoute {
        provider: application::CognitionProviderId::new(row.provider_slug).map_err(corrupt)?,
        requested_model: row.requested_model,
        billing_class: parse_billing_class(&row.billing_class)?,
    };
    route.validate().map_err(corrupt)?;
    let persistence_state = match row.dispatch_state.as_str() {
        "skipped" => CognitionAttemptPersistenceState::Skipped,
        "dispatched" => CognitionAttemptPersistenceState::Dispatched,
        "completed" => CognitionAttemptPersistenceState::Completed,
        "abandoned" => CognitionAttemptPersistenceState::Abandoned,
        other => {
            return Err(StoreError::Corrupt(format!(
                "unknown cognition attempt state {other}"
            )));
        }
    };
    let attempt: Option<CognitionRouteAttempt> = row
        .attempt_payload
        .map(serde_json::from_value)
        .transpose()
        .map_err(corrupt)?;
    if let (Some(attempt), Some(checksum)) = (&attempt, &row.attempt_checksum) {
        if Digest::canonical(attempt).map_err(corrupt)?
            != digest_from_db(checksum, "attempt checksum")?
        {
            return Err(StoreError::Corrupt(
                "cognition attempt checksum mismatch".to_owned(),
            ));
        }
    } else if attempt.is_some() != row.attempt_checksum.is_some() {
        return Err(StoreError::Corrupt(
            "cognition attempt payload/checksum pair is incomplete".to_owned(),
        ));
    }
    let receipt: Option<ModelCognitionReceipt> = row
        .receipt_payload
        .map(serde_json::from_value)
        .transpose()
        .map_err(corrupt)?;
    if let (Some(status), Some(attempt)) = (&row.normalized_status, &attempt) {
        if parse_attempt_status(status)? != attempt.status {
            return Err(StoreError::Corrupt(
                "cognition attempt status disagrees with its payload".to_owned(),
            ));
        }
    } else if row.normalized_status.is_some() != attempt.is_some() {
        return Err(StoreError::Corrupt(
            "cognition attempt status/payload pair is incomplete".to_owned(),
        ));
    }
    let record = CognitionRouteAttemptRecord {
        route_index,
        route,
        persistence_state,
        attempt,
        receipt,
    };
    record.validate().map_err(corrupt)?;
    Ok(record)
}

fn route_from_attempt(attempt: &CognitionRouteAttempt) -> Result<CognitionModelRoute, StoreError> {
    let route = CognitionModelRoute {
        provider: attempt.provider.clone(),
        requested_model: attempt.requested_model.clone(),
        billing_class: attempt.billing_class,
    };
    route.validate().map_err(corrupt)?;
    Ok(route)
}

async fn validate_model_request_for_job(
    store: &PostgresStore,
    entry: &CognitionJobEntry,
    request: &ModelCognitionRequest,
) -> Result<(), StoreError> {
    entry.validate().map_err(corrupt)?;
    request.validate().map_err(corrupt)?;
    let recall = store
        .load_cognition_recall(entry)
        .await?
        .ok_or_else(|| StoreError::Conflict("cognition recall is not recorded".to_owned()))?;
    let expected =
        ModelCognitionRequest::from_selection(&entry.selection, recall.admitted_memories)
            .map_err(corrupt)?;
    if request == &expected {
        Ok(())
    } else {
        Err(StoreError::Conflict(
            "model request differs from the canonical selection and recorded recall".to_owned(),
        ))
    }
}

async fn finish_attempt(
    store: &PostgresStore,
    worker_id: &str,
    entry: &CognitionJobEntry,
    attempt: &CognitionRouteAttempt,
    receipt: Option<&ModelCognitionReceipt>,
    persistence_state: CognitionAttemptPersistenceState,
) -> Result<(), StoreError> {
    let route = route_from_attempt(attempt)?;
    let record = CognitionRouteAttemptRecord {
        route_index: attempt.route_index,
        route: route.clone(),
        persistence_state,
        attempt: Some(attempt.clone()),
        receipt: receipt.cloned(),
    };
    record.validate().map_err(corrupt)?;
    if persistence_state == CognitionAttemptPersistenceState::Abandoned
        && attempt.status != CognitionRouteAttemptStatus::Unavailable
    {
        return Err(StoreError::Conflict(
            "abandoned cognition attempt must normalize to unavailable".to_owned(),
        ));
    }
    let attempt_json = serde_json::to_value(attempt).map_err(corrupt)?;
    let attempt_checksum = Digest::canonical(attempt).map_err(corrupt)?;
    let receipt_json = receipt
        .map(serde_json::to_value)
        .transpose()
        .map_err(corrupt)?;
    let state = match persistence_state {
        CognitionAttemptPersistenceState::Completed => "completed",
        CognitionAttemptPersistenceState::Abandoned => "abandoned",
        _ => {
            return Err(StoreError::Conflict(
                "network attempt can finish only as completed or abandoned".to_owned(),
            ));
        }
    };
    let mut transaction = store.pool().begin().await.map_err(operation_error)?;
    ensure_external_resolution_claim(&mut transaction, worker_id, entry.selection.request_id)
        .await?;
    let updated = sqlx::query(
        r#"
        UPDATE cognition_route_attempts
        SET
            dispatch_state = $6,
            normalized_status = $7,
            attempt_payload = $8,
            attempt_checksum = $9,
            receipt_payload = $10,
            completed_at = NOW()
        WHERE request_id = $1
          AND route_index = $2
          AND provider_slug = $3
          AND requested_model = $4
          AND billing_class = $5
          AND dispatch_state = 'dispatched'
        "#,
    )
    .bind(entry.selection.request_id)
    .bind(i32::from(attempt.route_index))
    .bind(route.provider.as_str())
    .bind(&route.requested_model)
    .bind(billing_class_text(route.billing_class))
    .bind(state)
    .bind(attempt_status_text(attempt.status))
    .bind(attempt_json)
    .bind(attempt_checksum.as_bytes().as_slice())
    .bind(receipt_json)
    .execute(&mut *transaction)
    .await
    .map_err(operation_error)?;
    if updated.rows_affected() != 1 {
        return Err(StoreError::Conflict(format!(
            "cognition route attempt {} is not an in-flight matching dispatch",
            attempt.route_index
        )));
    }
    transaction.commit().await.map_err(operation_error)
}

async fn ensure_claim(
    transaction: &mut Transaction<'_, Postgres>,
    worker_id: &str,
    request_id: Uuid,
) -> Result<(), StoreError> {
    let held = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT request.request_id
        FROM cognition_requests AS request
        WHERE request.request_id = $1
          AND request.claimed_by = $2
          AND NOT EXISTS (
              SELECT 1 FROM cognition_results WHERE request_id = request.request_id
          )
          AND NOT EXISTS (
              SELECT 1 FROM cognition_deadline_latches WHERE request_id = request.request_id
          )
        FOR UPDATE
        "#,
    )
    .bind(request_id)
    .bind(worker_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(operation_error)?;
    if held.is_some() {
        Ok(())
    } else {
        Err(StoreError::Conflict(format!(
            "cognition request {request_id} is not held by this worker"
        )))
    }
}

/// External calls are recorded as dispatched before leaving the process. Their
/// eventual response and billing resolution remain auditable even when the
/// immutable simulated-time deadline latch wins the race. This lock deliberately
/// checks worker ownership but does not make the late response canonical.
async fn ensure_external_resolution_claim(
    transaction: &mut Transaction<'_, Postgres>,
    worker_id: &str,
    request_id: Uuid,
) -> Result<(), StoreError> {
    let held = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT request_id
        FROM cognition_requests
        WHERE request_id = $1
          AND claimed_by = $2
        FOR UPDATE
        "#,
    )
    .bind(request_id)
    .bind(worker_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(operation_error)?;
    if held.is_some() {
        Ok(())
    } else {
        Err(StoreError::Conflict(format!(
            "cognition request {request_id} is not held by this worker"
        )))
    }
}

async fn require_recorded_recall(
    transaction: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
) -> Result<(), StoreError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM cognition_recall_outcomes WHERE request_id = $1)",
    )
    .bind(request_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(operation_error)?;
    if exists {
        Ok(())
    } else {
        Err(StoreError::Conflict(
            "cognition route attempt requires a recorded recall outcome".to_owned(),
        ))
    }
}

fn billing_class_text(billing_class: CognitionBillingClass) -> &'static str {
    match billing_class {
        CognitionBillingClass::FreeAllocation => "free_allocation",
        CognitionBillingClass::TrialCredit => "trial_credit",
        CognitionBillingClass::DevelopmentOnly => "development_only",
        CognitionBillingClass::PaidApproved => "paid_approved",
    }
}

fn parse_billing_class(value: &str) -> Result<CognitionBillingClass, StoreError> {
    match value {
        "free_allocation" => Ok(CognitionBillingClass::FreeAllocation),
        "trial_credit" => Ok(CognitionBillingClass::TrialCredit),
        "development_only" => Ok(CognitionBillingClass::DevelopmentOnly),
        "paid_approved" => Ok(CognitionBillingClass::PaidApproved),
        other => Err(StoreError::Corrupt(format!(
            "unknown cognition billing class {other}"
        ))),
    }
}

fn attempt_status_text(status: CognitionRouteAttemptStatus) -> &'static str {
    match status {
        CognitionRouteAttemptStatus::Succeeded => "succeeded",
        CognitionRouteAttemptStatus::Unavailable => "unavailable",
        CognitionRouteAttemptStatus::Rejected => "rejected",
        CognitionRouteAttemptStatus::InvalidResponse => "invalid_response",
        CognitionRouteAttemptStatus::SkippedUnconfigured => "skipped_unconfigured",
        CognitionRouteAttemptStatus::SkippedCooldown => "skipped_cooldown",
        CognitionRouteAttemptStatus::SkippedQuotaExhausted => "skipped_quota_exhausted",
        CognitionRouteAttemptStatus::SkippedDisabled => "skipped_disabled",
        CognitionRouteAttemptStatus::SkippedPaidUnauthorized => "skipped_paid_unauthorized",
        CognitionRouteAttemptStatus::StoppedAttemptLimit => "stopped_attempt_limit",
    }
}

fn parse_attempt_status(value: &str) -> Result<CognitionRouteAttemptStatus, StoreError> {
    match value {
        "succeeded" => Ok(CognitionRouteAttemptStatus::Succeeded),
        "unavailable" => Ok(CognitionRouteAttemptStatus::Unavailable),
        "rejected" => Ok(CognitionRouteAttemptStatus::Rejected),
        "invalid_response" => Ok(CognitionRouteAttemptStatus::InvalidResponse),
        "skipped_unconfigured" => Ok(CognitionRouteAttemptStatus::SkippedUnconfigured),
        "skipped_cooldown" => Ok(CognitionRouteAttemptStatus::SkippedCooldown),
        "skipped_quota_exhausted" => Ok(CognitionRouteAttemptStatus::SkippedQuotaExhausted),
        "skipped_disabled" => Ok(CognitionRouteAttemptStatus::SkippedDisabled),
        "skipped_paid_unauthorized" => Ok(CognitionRouteAttemptStatus::SkippedPaidUnauthorized),
        "stopped_attempt_limit" => Ok(CognitionRouteAttemptStatus::StoppedAttemptLimit),
        other => Err(StoreError::Corrupt(format!(
            "unknown cognition attempt status {other}"
        ))),
    }
}

async fn fetch_cost_reservation(
    transaction: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
    for_update: bool,
) -> Result<Option<CostReservationRow>, StoreError> {
    let query = if for_update {
        r#"
        SELECT billing_month, reserved_micro_usd, status, actual_micro_usd
        FROM cognition_cost_reservations
        WHERE request_id = $1
        FOR UPDATE
        "#
    } else {
        r#"
        SELECT billing_month, reserved_micro_usd, status, actual_micro_usd
        FROM cognition_cost_reservations
        WHERE request_id = $1
        "#
    };
    sqlx::query_as::<_, CostReservationRow>(query)
        .bind(request_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(operation_error)
}

enum PaidReservationResolution {
    Settled(u64),
    Released,
    Indeterminate,
}

async fn resolve_paid_reservation(
    transaction: &mut Transaction<'_, Postgres>,
    authorization: &PaidCognitionAuthorization,
    resolution: PaidReservationResolution,
) -> Result<(), StoreError> {
    let existing = fetch_cost_reservation(transaction, authorization.request_id, true)
        .await?
        .ok_or_else(|| StoreError::Conflict("paid cognition reservation is missing".to_owned()))?;
    let reserved = to_i64(
        authorization.reserved_micro_usd,
        "paid cognition reservation",
    )?;
    if existing.billing_month != authorization.billing_month
        || existing.reserved_micro_usd != reserved
        || existing.status != "reserved"
        || existing.actual_micro_usd.is_some()
    {
        return Err(StoreError::Conflict(
            "paid cognition reservation is not the active authorization".to_owned(),
        ));
    }
    let (status, actual, release_reserved, add_spent) = match resolution {
        PaidReservationResolution::Settled(actual) => {
            let actual = to_i64(actual, "paid cognition actual cost")?;
            if actual > reserved {
                return Err(StoreError::Conflict(
                    "paid cognition cost exceeds its reservation".to_owned(),
                ));
            }
            ("settled", Some(actual), reserved, actual)
        }
        PaidReservationResolution::Released => ("released", None, reserved, 0),
        PaidReservationResolution::Indeterminate => ("indeterminate", None, 0, 0),
    };
    if release_reserved != 0 || add_spent != 0 {
        let updated = sqlx::query(
            r#"
            UPDATE cognition_cost_accounts
            SET
                reserved_micro_usd = reserved_micro_usd - $2,
                spent_micro_usd = spent_micro_usd + $3,
                updated_at = NOW()
            WHERE billing_month = $1
              AND reserved_micro_usd >= $2
            "#,
        )
        .bind(authorization.billing_month)
        .bind(release_reserved)
        .bind(add_spent)
        .execute(&mut **transaction)
        .await
        .map_err(operation_error)?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::Conflict(
                "paid cognition cost account cannot resolve reservation".to_owned(),
            ));
        }
    }
    let updated = sqlx::query(
        r#"
        UPDATE cognition_cost_reservations
        SET status = $2, actual_micro_usd = $3, resolved_at = NOW()
        WHERE request_id = $1 AND status = 'reserved'
        "#,
    )
    .bind(authorization.request_id)
    .bind(status)
    .bind(actual)
    .execute(&mut **transaction)
    .await
    .map_err(operation_error)?;
    if updated.rows_affected() == 1 {
        Ok(())
    } else {
        Err(StoreError::Conflict(
            "paid cognition reservation lost its resolution race".to_owned(),
        ))
    }
}

async fn paid_attempt_was_dispatched(
    transaction: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM cognition_route_attempts
            WHERE request_id = $1
              AND billing_class = 'paid_approved'
              AND network_dispatched
        )
        "#,
    )
    .bind(request_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(operation_error)
}

fn to_i64(value: u64, field: &str) -> Result<i64, StoreError> {
    i64::try_from(value)
        .map_err(|_| StoreError::Conflict(format!("{field} does not fit in PostgreSQL BIGINT")))
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
