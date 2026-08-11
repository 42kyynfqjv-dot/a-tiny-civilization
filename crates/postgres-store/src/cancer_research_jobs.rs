use application::{
    CancerResearchAttemptPersistenceState, CancerResearchJobEntry, CancerResearchJobStore,
    CancerResearchLadderResult, CancerResearchModelReceipt, CancerResearchModelRequest,
    CancerResearchRouteAttemptRecord, CognitionBillingClass, CognitionModelRoute,
    CognitionRouteAttempt, CognitionRouteAttemptStatus, CognitionRouteRegistry, StoreError,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;
use world_domain::{CancerResearchInferenceTier, CancerResearchStage, Digest};

use crate::PostgresStore;

#[derive(FromRow)]
struct ResearchJobRow {
    request_id: Uuid,
    world_id: Uuid,
    resident_id: Uuid,
    selected_tick: i64,
    deadline_tick: i64,
    ordinal: i64,
    stage: String,
    inference_tier: String,
    request_payload: Value,
    request_checksum: Vec<u8>,
    claim_count: i64,
}

#[derive(FromRow)]
struct ResearchAttemptRow {
    route_index: i32,
    route_payload: Value,
    route_checksum: Vec<u8>,
    normalized_status: Option<String>,
    attempt_payload: Option<Value>,
    attempt_checksum: Option<Vec<u8>>,
    receipt_payload: Option<Value>,
    receipt_checksum: Option<Vec<u8>>,
}

#[derive(FromRow)]
struct ResearchResultRow {
    result_payload: Value,
    result_checksum: Vec<u8>,
}

#[async_trait]
impl CancerResearchJobStore for PostgresStore {
    async fn enqueue_cancer_research_request(
        &self,
        request: &CancerResearchModelRequest,
    ) -> Result<(), StoreError> {
        request.validate().map_err(corrupt)?;
        let payload = serde_json::to_value(request).map_err(corrupt)?;
        let checksum = request.canonical_hash().map_err(corrupt)?;
        let selected_tick = to_i64(request.selection.selected_at_tick.get(), "selected tick")?;
        let deadline_tick = to_i64(request.selection.deadline_tick.get(), "deadline tick")?;
        let ordinal = i64::from(request.selection.ordinal);
        let inserted = sqlx::query(
            r#"
            INSERT INTO cancer_research_requests (
                request_id, world_id, resident_id, selected_tick, deadline_tick,
                ordinal, stage, inference_tier, request_payload, request_checksum
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            ON CONFLICT (request_id) DO NOTHING
            "#,
        )
        .bind(request.request_id)
        .bind(request.selection.world_id.as_uuid())
        .bind(request.selection.resident_id.as_uuid())
        .bind(selected_tick)
        .bind(deadline_tick)
        .bind(ordinal)
        .bind(stage_text(request.selection.stage))
        .bind(tier_text(request.selection.inference_tier))
        .bind(&payload)
        .bind(checksum.as_bytes().as_slice())
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        if inserted.rows_affected() == 1 {
            return Ok(());
        }
        let existing = sqlx::query_as::<_, (Value, Vec<u8>)>(
            "SELECT request_payload, request_checksum FROM cancer_research_requests WHERE request_id=$1",
        )
        .bind(request.request_id)
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        match existing {
            Some((stored_payload, stored_checksum))
                if stored_payload == payload
                    && digest_from_db(&stored_checksum, "research request checksum")?
                        == checksum =>
            {
                Ok(())
            }
            Some(_) => Err(StoreError::Corrupt(format!(
                "cancer research request {} conflicts with its durable input",
                request.request_id
            ))),
            None => Err(StoreError::Conflict(
                "cancer research request disappeared during idempotency check".to_owned(),
            )),
        }
    }

    async fn claim_next_cancer_research_request(
        &self,
        worker_id: &str,
        claim_lease_seconds: u32,
    ) -> Result<Option<CancerResearchJobEntry>, StoreError> {
        validate_worker_id(worker_id)?;
        let lease_seconds = i64::from(claim_lease_seconds.clamp(1, 3_600));
        let row = sqlx::query_as::<_, ResearchJobRow>(
            r#"
            WITH candidate AS (
                SELECT request_id
                FROM cancer_research_requests
                WHERE completed_at IS NULL
                  AND available_at <= NOW()
                  AND (
                      claimed_at IS NULL
                      OR claimed_at < NOW() - ($2::BIGINT * INTERVAL '1 second')
                  )
                ORDER BY selected_tick, request_id
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE cancer_research_requests AS request
            SET claimed_by=$1, claimed_at=NOW(), claim_count=request.claim_count+1, last_error=NULL
            FROM candidate
            WHERE request.request_id=candidate.request_id
            RETURNING request.request_id, request.world_id, request.resident_id,
                request.selected_tick, request.deadline_tick, request.ordinal,
                request.stage, request.inference_tier, request.request_payload,
                request.request_checksum, request.claim_count
            "#,
        )
        .bind(worker_id)
        .bind(lease_seconds)
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        row.map(parse_job).transpose()
    }

    async fn reschedule_cancer_research_request(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        error: &str,
        retry_after_seconds: u32,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        let error = error.chars().take(2_048).collect::<String>();
        let retry = i64::from(retry_after_seconds.clamp(1, 3_600));
        let updated = sqlx::query(
            r#"
            UPDATE cancer_research_requests
            SET available_at=NOW()+($3::BIGINT*INTERVAL '1 second'),
                claimed_by=NULL, claimed_at=NULL, last_error=$4
            WHERE request_id=$1 AND claimed_by=$2 AND completed_at IS NULL
            "#,
        )
        .bind(entry.request.request_id)
        .bind(worker_id)
        .bind(retry)
        .bind(error)
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        require_one(
            updated.rows_affected(),
            "research request is not held by this worker",
        )
    }

    async fn begin_cancer_research_route_attempt(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        route_index: u16,
        route: &CognitionModelRoute,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        entry.request.validate_route(route).map_err(corrupt)?;
        if route_index >= application::MAX_CANCER_RESEARCH_NETWORK_ATTEMPTS {
            return Err(StoreError::Conflict(
                "cancer research network-attempt limit reached".to_owned(),
            ));
        }
        let route_payload = serde_json::to_value(route).map_err(corrupt)?;
        let route_checksum = Digest::canonical(route).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.request.request_id).await?;
        sqlx::query(
            r#"
            INSERT INTO cancer_research_route_dispatches (
                request_id, route_index, provider_slug, requested_model,
                billing_class, route_payload, route_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6,$7)
            "#,
        )
        .bind(entry.request.request_id)
        .bind(i32::from(route_index))
        .bind(route.provider.as_str())
        .bind(&route.requested_model)
        .bind(billing_class_text(route.billing_class))
        .bind(route_payload)
        .bind(route_checksum.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn finish_cancer_research_route_attempt(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        attempt: &CognitionRouteAttempt,
        receipt: Option<&CancerResearchModelReceipt>,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        if !matches!(
            attempt.status,
            CognitionRouteAttemptStatus::Succeeded
                | CognitionRouteAttemptStatus::Unavailable
                | CognitionRouteAttemptStatus::Rejected
                | CognitionRouteAttemptStatus::InvalidResponse
        ) {
            return Err(StoreError::Conflict(
                "research network attempt requires a terminal network status".to_owned(),
            ));
        }
        let route = route_from_attempt(attempt)?;
        entry.request.validate_route(&route).map_err(corrupt)?;
        match (attempt.status, receipt) {
            (CognitionRouteAttemptStatus::Succeeded, Some(receipt)) => receipt
                .validate_against(&route, &entry.request)
                .map_err(corrupt)?,
            (CognitionRouteAttemptStatus::Succeeded, None) => {
                return Err(StoreError::Conflict(
                    "successful research attempt omitted its receipt".to_owned(),
                ));
            }
            (_, Some(_)) => {
                return Err(StoreError::Conflict(
                    "failed research attempt cannot carry a receipt".to_owned(),
                ));
            }
            (_, None) => {}
        }
        let attempt_payload = serde_json::to_value(attempt).map_err(corrupt)?;
        let attempt_checksum = Digest::canonical(attempt).map_err(corrupt)?;
        let receipt_payload = receipt
            .map(serde_json::to_value)
            .transpose()
            .map_err(corrupt)?;
        let receipt_checksum = receipt
            .map(Digest::canonical)
            .transpose()
            .map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.request.request_id).await?;
        let dispatch = sqlx::query_as::<_, (Value, Vec<u8>)>(
            r#"SELECT route_payload, route_checksum
               FROM cancer_research_route_dispatches
               WHERE request_id=$1 AND route_index=$2 FOR UPDATE"#,
        )
        .bind(entry.request.request_id)
        .bind(i32::from(attempt.route_index))
        .fetch_optional(&mut *transaction)
        .await
        .map_err(operation_error)?
        .ok_or_else(|| StoreError::Conflict("research route was not dispatched".to_owned()))?;
        if dispatch.0 != serde_json::to_value(&route).map_err(corrupt)?
            || digest_from_db(&dispatch.1, "research route checksum")?
                != Digest::canonical(&route).map_err(corrupt)?
        {
            return Err(StoreError::Corrupt(
                "research route differs from its durable dispatch".to_owned(),
            ));
        }
        sqlx::query(
            r#"
            INSERT INTO cancer_research_route_outcomes (
                request_id, route_index, normalized_status, attempt_payload,
                attempt_checksum, receipt_payload, receipt_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6,$7)
            "#,
        )
        .bind(entry.request.request_id)
        .bind(i32::from(attempt.route_index))
        .bind(attempt_status_text(attempt.status))
        .bind(attempt_payload)
        .bind(attempt_checksum.as_bytes().as_slice())
        .bind(receipt_payload)
        .bind(receipt_checksum.map(|checksum| checksum.as_bytes().to_vec()))
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn list_cancer_research_route_attempts(
        &self,
        entry: &CancerResearchJobEntry,
    ) -> Result<Vec<CancerResearchRouteAttemptRecord>, StoreError> {
        entry.validate().map_err(corrupt)?;
        let rows = sqlx::query_as::<_, ResearchAttemptRow>(
            r#"
            SELECT dispatch.route_index, dispatch.route_payload, dispatch.route_checksum,
                outcome.normalized_status, outcome.attempt_payload, outcome.attempt_checksum,
                outcome.receipt_payload, outcome.receipt_checksum
            FROM cancer_research_route_dispatches AS dispatch
            LEFT JOIN cancer_research_route_outcomes AS outcome
              ON outcome.request_id=dispatch.request_id AND outcome.route_index=dispatch.route_index
            WHERE dispatch.request_id=$1
            ORDER BY dispatch.route_index
            "#,
        )
        .bind(entry.request.request_id)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        rows.into_iter().map(parse_attempt).collect()
    }

    async fn complete_cancer_research_request(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        registry: &CognitionRouteRegistry,
        result: &CancerResearchLadderResult,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        result
            .validate_against(registry, &entry.request)
            .map_err(corrupt)?;
        let durable = self.list_cancer_research_route_attempts(entry).await?;
        if durable.iter().any(|record| {
            record.persistence_state == CancerResearchAttemptPersistenceState::Dispatched
        }) {
            return Err(StoreError::Conflict(
                "cannot complete research while a network attempt is in flight".to_owned(),
            ));
        }
        let durable_attempts = durable
            .iter()
            .map(|record| {
                record.attempt.clone().ok_or_else(|| {
                    StoreError::Corrupt("terminal research attempt omitted payload".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let result_network_attempts = result
            .attempts
            .iter()
            .filter(|attempt| {
                matches!(
                    attempt.status,
                    CognitionRouteAttemptStatus::Succeeded
                        | CognitionRouteAttemptStatus::Unavailable
                        | CognitionRouteAttemptStatus::Rejected
                        | CognitionRouteAttemptStatus::InvalidResponse
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let durable_receipt = durable.iter().find_map(|record| record.receipt.clone());
        if durable_attempts != result_network_attempts || durable_receipt != result.receipt {
            return Err(StoreError::Conflict(
                "research result differs from its durable network-attempt history".to_owned(),
            ));
        }
        let result_payload = serde_json::to_value(result).map_err(corrupt)?;
        let result_checksum = Digest::canonical(result).map_err(corrupt)?;
        let registry_checksum = registry
            .canonical_hash(entry.request.route_purpose())
            .map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.request.request_id).await?;
        sqlx::query(
            r#"
            INSERT INTO cancer_research_results (
                request_id, route_policy_version, route_registry_checksum,
                result_payload, result_checksum
            ) VALUES ($1,$2,$3,$4,$5)
            "#,
        )
        .bind(entry.request.request_id)
        .bind(i32::from(registry.policy_version))
        .bind(registry_checksum.as_bytes().as_slice())
        .bind(result_payload)
        .bind(result_checksum.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        sqlx::query(
            "UPDATE cancer_research_requests SET completed_at=NOW() WHERE request_id=$1 AND claimed_by=$2 AND completed_at IS NULL",
        )
        .bind(entry.request.request_id)
        .bind(worker_id)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn load_cancer_research_result(
        &self,
        request_id: Uuid,
    ) -> Result<Option<CancerResearchLadderResult>, StoreError> {
        let row = sqlx::query_as::<_, ResearchResultRow>(
            "SELECT result_payload, result_checksum FROM cancer_research_results WHERE request_id=$1",
        )
        .bind(request_id)
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let result: CancerResearchLadderResult =
            serde_json::from_value(row.result_payload).map_err(corrupt)?;
        if result.request_id != request_id
            || Digest::canonical(&result).map_err(corrupt)?
                != digest_from_db(&row.result_checksum, "research result checksum")?
        {
            return Err(StoreError::Corrupt(
                "cancer research result failed its durable checksum".to_owned(),
            ));
        }
        Ok(Some(result))
    }
}

fn parse_job(row: ResearchJobRow) -> Result<CancerResearchJobEntry, StoreError> {
    let request: CancerResearchModelRequest =
        serde_json::from_value(row.request_payload).map_err(corrupt)?;
    request.validate().map_err(corrupt)?;
    let checksum = digest_from_db(&row.request_checksum, "research request checksum")?;
    let claim_count = u32::try_from(row.claim_count)
        .map_err(|_| StoreError::Corrupt("invalid research claim count".to_owned()))?;
    if request.request_id != row.request_id
        || request.selection.world_id.as_uuid() != row.world_id
        || request.selection.resident_id.as_uuid() != row.resident_id
        || to_i64(request.selection.selected_at_tick.get(), "selected tick")? != row.selected_tick
        || to_i64(request.selection.deadline_tick.get(), "deadline tick")? != row.deadline_tick
        || i64::from(request.selection.ordinal) != row.ordinal
        || stage_text(request.selection.stage) != row.stage
        || tier_text(request.selection.inference_tier) != row.inference_tier
        || request.canonical_hash().map_err(corrupt)? != checksum
    {
        return Err(StoreError::Corrupt(format!(
            "cancer research request {} indexed columns disagree with its payload",
            row.request_id
        )));
    }
    let entry = CancerResearchJobEntry {
        request,
        claim_count,
    };
    entry.validate().map_err(corrupt)?;
    Ok(entry)
}

fn parse_attempt(row: ResearchAttemptRow) -> Result<CancerResearchRouteAttemptRecord, StoreError> {
    let route: CognitionModelRoute = serde_json::from_value(row.route_payload).map_err(corrupt)?;
    route.validate().map_err(corrupt)?;
    if Digest::canonical(&route).map_err(corrupt)?
        != digest_from_db(&row.route_checksum, "research route checksum")?
    {
        return Err(StoreError::Corrupt(
            "research route failed its durable checksum".to_owned(),
        ));
    }
    let route_index = u16::try_from(row.route_index)
        .map_err(|_| StoreError::Corrupt("invalid research route index".to_owned()))?;
    let (persistence_state, attempt, receipt) = match row.normalized_status {
        None => {
            if row.attempt_payload.is_some()
                || row.attempt_checksum.is_some()
                || row.receipt_payload.is_some()
                || row.receipt_checksum.is_some()
            {
                return Err(StoreError::Corrupt(
                    "in-flight research dispatch contains a terminal payload".to_owned(),
                ));
            }
            (
                CancerResearchAttemptPersistenceState::Dispatched,
                None,
                None,
            )
        }
        Some(status) => {
            let attempt_payload = row.attempt_payload.ok_or_else(|| {
                StoreError::Corrupt("research outcome omitted its attempt".to_owned())
            })?;
            let attempt: CognitionRouteAttempt =
                serde_json::from_value(attempt_payload).map_err(corrupt)?;
            if attempt_status_text(attempt.status) != status
                || Digest::canonical(&attempt).map_err(corrupt)?
                    != digest_from_db(
                        row.attempt_checksum.as_deref().unwrap_or_default(),
                        "research attempt checksum",
                    )?
            {
                return Err(StoreError::Corrupt(
                    "research attempt failed its durable checksum".to_owned(),
                ));
            }
            let receipt = row
                .receipt_payload
                .map(serde_json::from_value::<CancerResearchModelReceipt>)
                .transpose()
                .map_err(corrupt)?;
            match (&receipt, row.receipt_checksum) {
                (Some(receipt), Some(checksum))
                    if Digest::canonical(receipt).map_err(corrupt)?
                        == digest_from_db(&checksum, "research receipt checksum")? => {}
                (None, None) => {}
                _ => {
                    return Err(StoreError::Corrupt(
                        "research receipt failed its durable checksum".to_owned(),
                    ));
                }
            }
            (
                CancerResearchAttemptPersistenceState::Completed,
                Some(attempt),
                receipt,
            )
        }
    };
    let record = CancerResearchRouteAttemptRecord {
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

async fn ensure_claim(
    transaction: &mut Transaction<'_, Postgres>,
    worker_id: &str,
    request_id: Uuid,
) -> Result<(), StoreError> {
    let held = sqlx::query_scalar::<_, Uuid>(
        "SELECT request_id FROM cancer_research_requests WHERE request_id=$1 AND claimed_by=$2 AND completed_at IS NULL FOR UPDATE",
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
            "cancer research request {request_id} is not held by this worker"
        )))
    }
}

fn validate_worker_id(worker_id: &str) -> Result<(), StoreError> {
    if worker_id.trim() != worker_id || worker_id.is_empty() || worker_id.len() > 128 {
        Err(StoreError::Conflict(
            "research worker identifier must contain 1 to 128 trimmed bytes".to_owned(),
        ))
    } else {
        Ok(())
    }
}

const fn stage_text(stage: CancerResearchStage) -> &'static str {
    match stage {
        CancerResearchStage::BlindDiscovery => "blind_discovery",
        CancerResearchStage::LiteratureAudit => "literature_audit",
        CancerResearchStage::IndependentReplication => "independent_replication",
    }
}

const fn tier_text(tier: CancerResearchInferenceTier) -> &'static str {
    match tier {
        CancerResearchInferenceTier::Exploration => "exploration",
        CancerResearchInferenceTier::Escalation => "escalation",
    }
}

const fn billing_class_text(class: CognitionBillingClass) -> &'static str {
    match class {
        CognitionBillingClass::FreeAllocation => "free_allocation",
        CognitionBillingClass::TrialCredit => "trial_credit",
        CognitionBillingClass::DevelopmentOnly => "development_only",
        CognitionBillingClass::PaidApproved => "paid_approved",
    }
}

const fn attempt_status_text(status: CognitionRouteAttemptStatus) -> &'static str {
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

fn digest_from_db(bytes: &[u8], field: &str) -> Result<Digest, StoreError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
        StoreError::Corrupt(format!("{field} has {} bytes instead of 32", bytes.len()))
    })?;
    Ok(Digest::from_bytes(bytes))
}

fn to_i64(value: u64, field: &str) -> Result<i64, StoreError> {
    i64::try_from(value)
        .map_err(|_| StoreError::Conflict(format!("{field} does not fit PostgreSQL BIGINT")))
}

fn require_one(rows: u64, message: &str) -> Result<(), StoreError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(StoreError::Conflict(message.to_owned()))
    }
}

fn corrupt(error: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt(error.to_string())
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

#[cfg(test)]
mod tests {
    use application::{
        CANCER_RESEARCH_MODEL_CONTRACT_VERSION, CancerResearchEvidenceDocument,
        CancerResearchModelRequest, ModelTokenUsage,
    };
    use uuid::Uuid;
    use world_domain::{
        CancerResearchArtifactKind, CancerResearchClaim, CancerResearchContribution,
        CancerResearchInferenceTier, CancerResearchProfile, CancerResearchTarget,
        CancerResearchTask, CancerResearchTurnSelection, EntityId, SimTick, WorldId, WorldSeed,
    };

    use super::*;

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing at an isolated PostgreSQL database"]
    async fn research_ledger_round_trips_and_rejects_mutation() {
        let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        let store = PostgresStore::connect(&database_url, 4)
            .await
            .expect("connect test database");
        store.migrate().await.expect("migrate test database");

        let world_id = WorldId::from_uuid(Uuid::new_v4());
        let world_uuid = world_id.as_uuid();
        let uuid_bytes = world_uuid.as_bytes();
        let seed = u64::from_be_bytes(
            uuid_bytes[..8]
                .try_into()
                .expect("UUID contains at least eight bytes"),
        )
        .to_string();
        let zero = vec![0_u8; 32];
        sqlx::query(
            r#"
            INSERT INTO worlds (
                id, seed, status, ruleset_version, manifest, manifest_checksum,
                last_event_checksum, current_state_checksum
            ) VALUES ($1,$2,'running',37,'{}'::JSONB,$3,$3,$3)
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(seed)
        .bind(&zero)
        .execute(store.pool())
        .await
        .expect("insert test world");

        let resident_id = EntityId::deterministic(world_id, b"research-ledger-resident");
        let selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(10),
            SimTick::new(20),
            0,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::GenerateMechanisticHypothesis,
            CancerResearchInferenceTier::Exploration,
            CancerResearchProfile::seeded(WorldSeed::new(37), resident_id).expect("profile"),
            Vec::new(),
            None,
            2_048,
        )
        .expect("selection");
        let request = CancerResearchModelRequest::new(
            selection,
            Vec::<CancerResearchEvidenceDocument>::new(),
            Vec::new(),
        )
        .expect("request");
        store
            .enqueue_cancer_research_request(&request)
            .await
            .expect("enqueue");
        store
            .enqueue_cancer_research_request(&request)
            .await
            .expect("idempotent enqueue");
        let entry = store
            .claim_next_cancer_research_request("research-test-worker", 60)
            .await
            .expect("claim")
            .expect("job");
        assert_eq!(entry.request, request);

        let registry = CognitionRouteRegistry::cancer_research_exploration();
        let route = registry.routes[0].clone();
        store
            .begin_cancer_research_route_attempt("research-test-worker", &entry, 0, &route)
            .await
            .expect("record dispatch");
        let contribution = CancerResearchContribution::new(
            &entry.request.selection,
            CancerResearchArtifactKind::Hypothesis,
            "A bounded hypothesis",
            "An unverified mechanism proposed from the supplied primitives.",
            vec![CancerResearchClaim {
                statement: "A reversible state may affect the target phenotype.".to_owned(),
                testable_prediction: "A state perturbation changes the assay readout.".to_owned(),
                falsification_test: "The preregistered perturbation has no effect.".to_owned(),
                citation_hashes: Vec::new(),
            }],
        )
        .expect("contribution");
        let receipt = CancerResearchModelReceipt {
            contract_version: CANCER_RESEARCH_MODEL_CONTRACT_VERSION,
            request_id: request.request_id,
            request_hash: request.canonical_hash().expect("request hash"),
            provider: route.provider.clone(),
            requested_model: route.requested_model.clone(),
            resolved_model: route.requested_model.clone(),
            provider_response_id: "test-response".to_owned(),
            usage: ModelTokenUsage {
                prompt_tokens: 100,
                completion_tokens: 100,
            },
            billed_micro_usd: 0,
            contribution,
            provider_response_hash: Digest::sha256(b"provider response"),
            adapter_version: "test-adapter-v1".to_owned(),
        };
        let attempt = CognitionRouteAttempt {
            route_index: 0,
            provider: route.provider.clone(),
            requested_model: route.requested_model.clone(),
            billing_class: route.billing_class,
            status: CognitionRouteAttemptStatus::Succeeded,
        };
        store
            .finish_cancer_research_route_attempt(
                "research-test-worker",
                &entry,
                &attempt,
                Some(&receipt),
            )
            .await
            .expect("record outcome");
        let result = CancerResearchLadderResult {
            contract_version: CANCER_RESEARCH_MODEL_CONTRACT_VERSION,
            request_id: request.request_id,
            route_policy_version: registry.policy_version,
            route_registry_hash: registry
                .canonical_hash(request.route_purpose())
                .expect("registry hash"),
            attempts: vec![attempt],
            receipt: Some(receipt),
        };
        store
            .complete_cancer_research_request("research-test-worker", &entry, &registry, &result)
            .await
            .expect("complete");
        assert_eq!(
            store
                .load_cancer_research_result(request.request_id)
                .await
                .expect("load"),
            Some(result)
        );
        let mutation = sqlx::query(
            "UPDATE cancer_research_results SET route_policy_version=99 WHERE request_id=$1",
        )
        .bind(request.request_id)
        .execute(store.pool())
        .await;
        assert!(mutation.is_err());
    }
}
