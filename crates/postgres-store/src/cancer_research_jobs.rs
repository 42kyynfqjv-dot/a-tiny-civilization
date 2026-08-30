use application::{
    CancerNci60QualificationCandidate, CancerPatientDerivedMolecularCandidate,
    CancerResearchAttemptPersistenceState, CancerResearchCampaignCandidate,
    CancerResearchCampaignFollowup, CancerResearchCatalogItem,
    CancerResearchFireworksCostReconciliation, CancerResearchFireworksDispatchCandidate,
    CancerResearchJobEntry, CancerResearchJobStore, CancerResearchLadderResult,
    CancerResearchLiteratureSnapshot, CancerResearchMemoryInput, CancerResearchModelReceipt,
    CancerResearchModelRequest, CancerResearchNoveltyCandidate, CancerResearchPaidAuthorization,
    CancerResearchPaidReservationDecision, CancerResearchPriorResult,
    CancerResearchRouteAttemptRecord, CancerResearchTerminalFailureClass,
    CancerTcgaGbmTargetContextCandidate, CancerVirtualExperimentCandidate,
    CancerVirtualExperimentCatalogSummary, CognitionBillingClass, CognitionBillingScope,
    CognitionModelRoute, CognitionRouteAttempt, CognitionRouteAttemptStatus,
    CognitionRouteRegistry, MAX_CANCER_RESEARCH_MEMORY_INPUTS,
    MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD, MemoryRetain, StoreError,
    cancer_research_collective_id, cancer_research_contributions_duplicate,
    validate_fireworks_reconciliation_batch,
};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde_json::Value;
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;
use world_domain::{
    CancerNci60ResponseQualification, CancerPatientDerivedMolecularQualification,
    CancerResearchInferenceTier, CancerResearchNoveltyAudit, CancerResearchNoveltyStatus,
    CancerResearchStage, CancerTcgaGbmTargetContextQualification, CancerVirtualExperimentResult,
    Digest, EventSequence,
};

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

#[derive(FromRow)]
struct PriorResearchResultRow {
    request_payload: Value,
    request_checksum: Vec<u8>,
    result_payload: Value,
    result_checksum: Vec<u8>,
}

#[derive(FromRow)]
struct ResearchMemoryMirrorRow {
    operation_id: Uuid,
    document_id: Uuid,
    world_id: Uuid,
    agent_id: Uuid,
    source_sequence: i64,
    bank_id: String,
    payload_version: i32,
    payload: Value,
}

#[derive(FromRow)]
struct CampaignRootRow {
    request_payload: Value,
    request_checksum: Vec<u8>,
    result_payload: Value,
    result_checksum: Vec<u8>,
    experiment_payload: Value,
    experiment_checksum: Vec<u8>,
    audit_payload: Value,
    audit_checksum: Vec<u8>,
}

#[derive(FromRow)]
struct CampaignFollowupRow {
    request_payload: Value,
    request_checksum: Vec<u8>,
    request_completed: bool,
    result_payload: Option<Value>,
    result_checksum: Option<Vec<u8>>,
    experiment_payload: Option<Value>,
    experiment_checksum: Option<Vec<u8>>,
}

#[derive(FromRow)]
struct ResearchCostReservationRow {
    billing_month: NaiveDate,
    reserved_micro_usd: i64,
    status: String,
    actual_micro_usd: Option<i64>,
}

#[derive(FromRow)]
struct ResearchFireworksReconciliationRow {
    schema_version: i32,
    reconciliation_id: Uuid,
    route_index: i32,
    billing_month: NaiveDate,
    source_format: String,
    export_sha256: Vec<u8>,
    export_byte_length: i64,
    row_sha256: Vec<u8>,
    row_start_offset: i64,
    row_byte_length: i64,
    provider_started_at: DateTime<Utc>,
    matched_dispatch_at: DateTime<Utc>,
    requested_model: String,
    prompt_tokens: i64,
    completion_tokens: i64,
    actual_micro_usd: i64,
    reserved_micro_usd: i64,
    released_micro_usd: i64,
}

#[derive(FromRow)]
struct ResearchLiteratureRow {
    evidence_id: Uuid,
    world_id: Uuid,
    source_id: String,
    title: String,
    license: String,
    published_at: Option<NaiveDate>,
    content: String,
    content_hash: Vec<u8>,
    source_payload: Value,
    retrieved_at: DateTime<Utc>,
}

#[async_trait]
impl CancerResearchJobStore for PostgresStore {
    async fn store_cancer_research_literature(
        &self,
        snapshot: &CancerResearchLiteratureSnapshot,
    ) -> Result<(), StoreError> {
        snapshot.validate().map_err(corrupt)?;
        let inserted = sqlx::query(
            r#"
            INSERT INTO cancer_research_literature (
                evidence_id, world_id, source_id, title, license, published_at,
                content, content_hash, source_payload, retrieved_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            ON CONFLICT (world_id, source_id, content_hash) DO NOTHING
            "#,
        )
        .bind(snapshot.evidence_id)
        .bind(snapshot.world_id.as_uuid())
        .bind(&snapshot.source_id)
        .bind(&snapshot.title)
        .bind(&snapshot.license)
        .bind(snapshot.published_at)
        .bind(&snapshot.document.content)
        .bind(
            snapshot
                .document
                .reference
                .content_hash
                .as_bytes()
                .as_slice(),
        )
        .bind(&snapshot.source_payload)
        .bind(snapshot.retrieved_at)
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        if inserted.rows_affected() == 1 {
            return Ok(());
        }
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT evidence_id FROM cancer_research_literature WHERE world_id=$1 AND source_id=$2 AND content_hash=$3",
        )
        .bind(snapshot.world_id.as_uuid())
        .bind(&snapshot.source_id)
        .bind(snapshot.document.reference.content_hash.as_bytes().as_slice())
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        if existing == Some(snapshot.evidence_id) {
            Ok(())
        } else {
            Err(StoreError::Corrupt(
                "literature snapshot identity conflicts with durable evidence".to_owned(),
            ))
        }
    }

    async fn load_cancer_research_literature(
        &self,
        world_id: world_domain::WorldId,
        limit: usize,
    ) -> Result<Vec<CancerResearchLiteratureSnapshot>, StoreError> {
        let limit =
            i64::try_from(limit.clamp(1, application::MAX_CANCER_RESEARCH_LITERATURE_DOCUMENTS))
                .map_err(corrupt)?;
        let rows = sqlx::query_as::<_, ResearchLiteratureRow>(
            r#"
            SELECT evidence_id, world_id, source_id, title, license, published_at,
                   content, content_hash, source_payload, retrieved_at
            FROM (
                SELECT DISTINCT ON (source_id)
                       evidence_id, world_id, source_id, title, license, published_at,
                       content, content_hash, source_payload, retrieved_at
                FROM cancer_research_literature
                WHERE world_id=$1
                ORDER BY source_id, retrieved_at DESC, evidence_id DESC
            ) AS latest_source_snapshot
            ORDER BY published_at DESC NULLS LAST, evidence_id
            LIMIT $2
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        rows.into_iter()
            .map(|row| {
                let content_hash = digest_from_db(&row.content_hash, "literature content hash")?;
                let snapshot = CancerResearchLiteratureSnapshot {
                    evidence_id: row.evidence_id,
                    world_id: world_domain::WorldId::from_uuid(row.world_id),
                    source_id: row.source_id.clone(),
                    title: row.title,
                    license: row.license,
                    published_at: row.published_at,
                    document: application::CancerResearchEvidenceDocument {
                        reference: world_domain::CancerResearchEvidenceReference {
                            kind: world_domain::CancerResearchEvidenceKind::Literature,
                            source_id: row.source_id,
                            content_hash,
                        },
                        content: row.content,
                    },
                    source_payload: row.source_payload,
                    retrieved_at: row.retrieved_at,
                };
                snapshot.validate().map_err(corrupt)?;
                Ok(snapshot)
            })
            .collect()
    }

    async fn load_unaudited_cancer_research(
        &self,
        world_id: world_domain::WorldId,
        method_version: u16,
        limit: usize,
    ) -> Result<Vec<CancerResearchNoveltyCandidate>, StoreError> {
        if method_version == 0 {
            return Err(StoreError::Conflict(
                "novelty method version must be nonzero".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, PriorResearchResultRow>(
            r#"
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN cancer_research_novelty_audits AS audit
              ON audit.request_id=request.request_id
             AND audit.method_version=$2
            WHERE request.world_id=$1
              AND result.result_payload->'receipt' <> 'null'::JSONB
              AND audit.audit_id IS NULL
            ORDER BY request.ordinal DESC, request.request_id
            LIMIT $3
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i32::from(method_version))
        .bind(i64::try_from(limit.clamp(1, 32)).map_err(corrupt)?)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let (request, result) = parse_historical_research_result(row, world_id)?;
            let contribution = result
                .receipt
                .as_ref()
                .ok_or_else(|| corrupt("successful novelty candidate omitted its receipt"))?
                .contribution
                .clone();
            let prior_rows = sqlx::query_as::<_, PriorResearchResultRow>(
                r#"
                SELECT prior.request_payload, prior.request_checksum,
                       result.result_payload, result.result_checksum
                FROM cancer_research_requests AS prior
                JOIN cancer_research_results AS result USING (request_id)
                WHERE prior.world_id=$1
                  AND prior.ordinal < $2
                  AND result.result_payload->'receipt' <> 'null'::JSONB
                ORDER BY prior.ordinal DESC, prior.request_id
                LIMIT 128
                "#,
            )
            .bind(world_id.as_uuid())
            .bind(i64::from(request.selection.ordinal))
            .fetch_all(self.pool())
            .await
            .map_err(operation_error)?;
            let mut prior_contributions = Vec::with_capacity(prior_rows.len());
            for prior_row in prior_rows {
                let (_, prior_result) = parse_historical_research_result(prior_row, world_id)?;
                prior_contributions.push(
                    prior_result
                        .receipt
                        .ok_or_else(|| {
                            corrupt("successful prior novelty artifact omitted its receipt")
                        })?
                        .contribution,
                );
            }
            candidates.push(CancerResearchNoveltyCandidate {
                world_id,
                request_id: request.request_id,
                ordinal: request.selection.ordinal,
                artifact_hash: contribution.canonical_hash().map_err(corrupt)?,
                contribution,
                prior_contributions,
            });
        }
        Ok(candidates)
    }

    async fn store_cancer_research_novelty_audit(
        &self,
        audit: &CancerResearchNoveltyAudit,
    ) -> Result<(), StoreError> {
        audit.validate().map_err(corrupt)?;
        let payload = serde_json::to_value(audit).map_err(corrupt)?;
        let checksum = audit.canonical_hash().map_err(corrupt)?;
        let inserted = sqlx::query(
            r#"
            INSERT INTO cancer_research_novelty_audits (
                audit_id,world_id,request_id,method_version,artifact_hash,
                normalized_status,audit_payload,audit_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            ON CONFLICT (request_id,method_version) DO NOTHING
            "#,
        )
        .bind(audit.audit_id)
        .bind(audit.world_id.as_uuid())
        .bind(audit.request_id)
        .bind(i32::from(audit.method_version))
        .bind(audit.artifact_hash.as_bytes().as_slice())
        .bind(novelty_status_text(audit.status))
        .bind(&payload)
        .bind(checksum.as_bytes().as_slice())
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        if inserted.rows_affected() == 1 {
            return Ok(());
        }
        let existing = sqlx::query_as::<_, (Uuid, Value, Vec<u8>)>(
            "SELECT audit_id,audit_payload,audit_checksum FROM cancer_research_novelty_audits WHERE request_id=$1 AND method_version=$2",
        )
        .bind(audit.request_id)
        .bind(i32::from(audit.method_version))
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        match existing {
            Some((audit_id, stored_payload, stored_checksum))
                if audit_id == audit.audit_id
                    && stored_payload == payload
                    && digest_from_db(&stored_checksum, "novelty audit checksum")? == checksum =>
            {
                Ok(())
            }
            Some(_) => Err(StoreError::Corrupt(format!(
                "novelty audit {} conflicts with its durable assessment",
                audit.audit_id
            ))),
            None => Err(StoreError::Conflict(
                "novelty audit disappeared during idempotency check".to_owned(),
            )),
        }
    }

    async fn load_unexecuted_cancer_virtual_experiments(
        &self,
        world_id: world_domain::WorldId,
        method_version: u16,
        limit: usize,
    ) -> Result<Vec<CancerVirtualExperimentCandidate>, StoreError> {
        if method_version == 0 {
            return Err(StoreError::Conflict(
                "virtual lab method version must be nonzero".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, PriorResearchResultRow>(
            r#"
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN cancer_virtual_experiment_results AS experiment
              ON experiment.request_id=request.request_id
             AND experiment.method_version=$2
            WHERE request.world_id=$1
              AND result.result_payload->'receipt' <> 'null'::JSONB
              AND result.result_payload->'receipt'->'contribution'->'virtual_experiment_plan' IS NOT NULL
              AND experiment.experiment_id IS NULL
            ORDER BY request.ordinal DESC, request.request_id
            LIMIT $3
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i32::from(method_version))
        .bind(i64::try_from(limit.clamp(1, 64)).map_err(corrupt)?)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let (request, result) = parse_historical_research_result(row, world_id)?;
            let contribution = result
                .receipt
                .ok_or_else(|| corrupt("planned virtual experiment omitted its receipt"))?
                .contribution;
            let artifact_hash = contribution.canonical_hash().map_err(corrupt)?;
            candidates.push(CancerVirtualExperimentCandidate {
                world_id,
                request_id: request.request_id,
                ordinal: request.selection.ordinal,
                artifact_hash,
                contribution,
            });
        }
        Ok(candidates)
    }

    async fn store_cancer_virtual_experiment_result(
        &self,
        result: &CancerVirtualExperimentResult,
        contribution: &world_domain::CancerResearchContribution,
        ordinal: u32,
    ) -> Result<(), StoreError> {
        result.validate_against(contribution).map_err(corrupt)?;
        let payload = serde_json::to_value(result).map_err(corrupt)?;
        let checksum = result.canonical_hash(contribution).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        let stored_result_payload: Value = sqlx::query_scalar(
            "SELECT result_payload FROM cancer_research_results WHERE request_id=$1",
        )
        .bind(result.request_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(operation_error)?;
        let stored_result: CancerResearchLadderResult =
            serde_json::from_value(stored_result_payload).map_err(corrupt)?;
        let stored_contribution = &stored_result
            .receipt
            .ok_or_else(|| corrupt("virtual experiment source omitted its receipt"))?
            .contribution;
        if stored_contribution != contribution {
            return Err(StoreError::Corrupt(
                "virtual experiment contribution differs from immutable research result".to_owned(),
            ));
        }
        let inserted = sqlx::query(
            r#"
            INSERT INTO cancer_virtual_experiment_results (
                experiment_id,world_id,request_id,method_version,artifact_hash,
                plan_hash,result_payload,result_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            ON CONFLICT (request_id,method_version) DO NOTHING
            "#,
        )
        .bind(result.experiment_id)
        .bind(result.world_id.as_uuid())
        .bind(result.request_id)
        .bind(i32::from(result.method_version))
        .bind(result.artifact_hash.as_bytes().as_slice())
        .bind(result.plan_hash.as_bytes().as_slice())
        .bind(&payload)
        .bind(checksum.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        if inserted.rows_affected() == 0 {
            let existing = sqlx::query_as::<_, (Uuid, Value, Vec<u8>)>(
                "SELECT experiment_id,result_payload,result_checksum FROM cancer_virtual_experiment_results WHERE request_id=$1 AND method_version=$2",
            )
            .bind(result.request_id)
            .bind(i32::from(result.method_version))
            .fetch_optional(&mut *transaction)
            .await
            .map_err(operation_error)?;
            match existing {
                Some((experiment_id, stored_payload, stored_checksum))
                    if experiment_id == result.experiment_id
                        && stored_payload == payload
                        && digest_from_db(&stored_checksum, "virtual experiment checksum")?
                            == checksum => {}
                Some(_) => {
                    return Err(StoreError::Corrupt(format!(
                        "virtual experiment {} conflicts with its durable result",
                        result.experiment_id
                    )));
                }
                None => {
                    return Err(StoreError::Conflict(
                        "virtual experiment disappeared during idempotency check".to_owned(),
                    ));
                }
            }
        }

        let (selected_tick, source_sequence): (i64, i64) = sqlx::query_as(
            r#"
            SELECT request.selected_tick,
                   (SELECT sequence FROM event_batches
                    WHERE world_id=request.world_id AND tick <= request.selected_tick
                    ORDER BY tick DESC,sequence DESC LIMIT 1) AS source_sequence
            FROM cancer_research_requests AS request
            WHERE request.request_id=$1
            "#,
        )
        .bind(result.request_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(operation_error)?;
        let retain = MemoryRetain::new(
            result.world_id,
            cancer_research_collective_id(result.world_id),
            EventSequence::new(
                u64::try_from(source_sequence)
                    .map_err(|_| corrupt("virtual experiment source sequence is negative"))?,
            ),
            world_domain::SimTick::new(
                u64::try_from(selected_tick)
                    .map_err(|_| corrupt("virtual experiment selected tick is negative"))?,
            ),
            ordinal,
            serde_json::to_string(result).map_err(corrupt)?,
            "Cancer World virtual experiment result",
        )
        .map_err(corrupt)?;
        sqlx::query(
            r#"
            INSERT INTO memory_outbox (
                operation_id,document_id,world_id,agent_id,source_sequence,
                bank_id,payload_version,payload,available_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'epoch'::TIMESTAMPTZ)
            ON CONFLICT (operation_id) DO NOTHING
            "#,
        )
        .bind(retain.operation_id)
        .bind(retain.document_id)
        .bind(retain.world_id.as_uuid())
        .bind(retain.agent_id.as_uuid())
        .bind(source_sequence)
        .bind(&retain.bank_id)
        .bind(i32::from(retain.payload_version))
        .bind(serde_json::to_value(&retain).map_err(corrupt)?)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn load_unqualified_cancer_nci60_predictions(
        &self,
        world_id: world_domain::WorldId,
        method_version: u16,
        limit: usize,
    ) -> Result<Vec<CancerNci60QualificationCandidate>, StoreError> {
        if method_version == 0 {
            return Err(StoreError::Conflict(
                "NCI-60 qualification method version must be nonzero".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, PriorResearchResultRow>(
            r#"
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN cancer_nci60_response_qualifications AS qualification
              ON qualification.request_id=request.request_id
             AND qualification.method_version=$2
            WHERE request.world_id=$1
              AND result.result_payload->'receipt' <> 'null'::JSONB
              AND result.result_payload->'receipt'->'contribution'->'nci60_response_prediction' IS NOT NULL
              AND qualification.qualification_id IS NULL
            ORDER BY request.ordinal, request.request_id
            LIMIT $3
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i32::from(method_version))
        .bind(i64::try_from(limit.clamp(1, 64)).map_err(corrupt)?)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let (request, result) = parse_historical_research_result(row, world_id)?;
            let mut challenge_documents = request.evidence_documents.iter().filter(|document| {
                document.reference.kind
                    == world_domain::CancerResearchEvidenceKind::ResponseChallenge
            });
            let challenge_document = challenge_documents.next().cloned().ok_or_else(|| {
                corrupt("NCI-60 prediction request omitted its challenge document")
            })?;
            if challenge_documents.next().is_some() {
                return Err(corrupt(
                    "NCI-60 prediction request contains multiple challenge documents",
                ));
            }
            let contribution = result
                .receipt
                .ok_or_else(|| corrupt("NCI-60 prediction omitted its receipt"))?
                .contribution;
            if contribution.nci60_response_prediction.is_none() {
                return Err(corrupt(
                    "NCI-60 qualification candidate omitted its prediction",
                ));
            }
            let artifact_hash = contribution.canonical_hash().map_err(corrupt)?;
            candidates.push(CancerNci60QualificationCandidate {
                world_id,
                request_id: request.request_id,
                ordinal: request.selection.ordinal,
                artifact_hash,
                contribution,
                challenge_document,
            });
        }
        Ok(candidates)
    }

    async fn store_cancer_nci60_qualification(
        &self,
        qualification: &CancerNci60ResponseQualification,
        contribution: &world_domain::CancerResearchContribution,
    ) -> Result<(), StoreError> {
        qualification
            .validate_against(contribution)
            .map_err(corrupt)?;
        let payload = serde_json::to_value(qualification).map_err(corrupt)?;
        let checksum = qualification
            .canonical_hash(contribution)
            .map_err(corrupt)?;
        let stored_result_payload: Value = sqlx::query_scalar(
            "SELECT result_payload FROM cancer_research_results WHERE request_id=$1",
        )
        .bind(qualification.request_id)
        .fetch_one(self.pool())
        .await
        .map_err(operation_error)?;
        let stored_result: CancerResearchLadderResult =
            serde_json::from_value(stored_result_payload).map_err(corrupt)?;
        let stored_contribution = &stored_result
            .receipt
            .ok_or_else(|| corrupt("NCI-60 qualification source omitted its receipt"))?
            .contribution;
        if stored_contribution != contribution {
            return Err(StoreError::Corrupt(
                "NCI-60 qualification contribution differs from immutable research result"
                    .to_owned(),
            ));
        }
        let inserted = sqlx::query(
            r#"
            INSERT INTO cancer_nci60_response_qualifications (
                qualification_id,world_id,request_id,method_version,artifact_hash,
                prediction_hash,challenge_id,answer_key_hash,result_payload,result_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            ON CONFLICT (request_id,method_version) DO NOTHING
            "#,
        )
        .bind(qualification.qualification_id)
        .bind(qualification.world_id.as_uuid())
        .bind(qualification.request_id)
        .bind(i32::from(qualification.method_version))
        .bind(qualification.artifact_hash.as_bytes().as_slice())
        .bind(qualification.prediction_hash.as_bytes().as_slice())
        .bind(qualification.challenge_id)
        .bind(qualification.answer_key.content_hash.as_bytes().as_slice())
        .bind(&payload)
        .bind(checksum.as_bytes().as_slice())
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        if inserted.rows_affected() == 1 {
            return Ok(());
        }
        let existing = sqlx::query_as::<_, (Uuid, Value, Vec<u8>)>(
            "SELECT qualification_id,result_payload,result_checksum FROM cancer_nci60_response_qualifications WHERE request_id=$1 AND method_version=$2",
        )
        .bind(qualification.request_id)
        .bind(i32::from(qualification.method_version))
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        match existing {
            Some((qualification_id, stored_payload, stored_checksum))
                if qualification_id == qualification.qualification_id
                    && stored_payload == payload
                    && digest_from_db(&stored_checksum, "NCI-60 qualification checksum")?
                        == checksum =>
            {
                Ok(())
            }
            Some(_) => Err(StoreError::Corrupt(format!(
                "NCI-60 qualification {} conflicts with its durable result",
                qualification.qualification_id
            ))),
            None => Err(StoreError::Conflict(
                "NCI-60 qualification disappeared during idempotency check".to_owned(),
            )),
        }
    }

    async fn load_unqualified_cancer_patient_derived_molecular_targets(
        &self,
        world_id: world_domain::WorldId,
        method_version: u16,
        limit: usize,
    ) -> Result<Vec<CancerPatientDerivedMolecularCandidate>, StoreError> {
        if method_version == 0 {
            return Err(StoreError::Conflict(
                "patient-derived molecular qualification method version must be nonzero".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, PriorResearchResultRow>(
            r#"
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN cancer_patient_derived_molecular_qualifications AS qualification
              ON qualification.request_id=request.request_id
             AND qualification.method_version=$2
            WHERE request.world_id=$1
              AND result.result_payload->'receipt' <> 'null'::JSONB
              AND JSONB_TYPEOF(
                  result.result_payload->'receipt'->'contribution'->'molecular_targets'
              )='array'
              AND JSONB_ARRAY_LENGTH(
                  result.result_payload->'receipt'->'contribution'->'molecular_targets'
              ) > 0
              AND qualification.qualification_id IS NULL
            ORDER BY request.ordinal,request.request_id
            LIMIT $3
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i32::from(method_version))
        .bind(i64::try_from(limit.clamp(1, 64)).map_err(corrupt)?)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let (request, result) = parse_historical_research_result(row, world_id)?;
            let contribution = result
                .receipt
                .ok_or_else(|| corrupt("molecular qualification candidate omitted its receipt"))?
                .contribution;
            if contribution.molecular_targets.is_empty() {
                return Err(corrupt(
                    "molecular qualification candidate omitted structured targets",
                ));
            }
            let artifact_hash = contribution.canonical_hash().map_err(corrupt)?;
            candidates.push(CancerPatientDerivedMolecularCandidate {
                world_id,
                request_id: request.request_id,
                ordinal: request.selection.ordinal,
                artifact_hash,
                contribution,
            });
        }
        Ok(candidates)
    }

    async fn store_cancer_patient_derived_molecular_qualification(
        &self,
        qualification: &CancerPatientDerivedMolecularQualification,
        contribution: &world_domain::CancerResearchContribution,
    ) -> Result<(), StoreError> {
        qualification
            .validate_against(contribution)
            .map_err(corrupt)?;
        let payload = serde_json::to_value(qualification).map_err(corrupt)?;
        let checksum = qualification
            .canonical_hash(contribution)
            .map_err(corrupt)?;
        let stored_result_payload: Value = sqlx::query_scalar(
            "SELECT result_payload FROM cancer_research_results WHERE request_id=$1",
        )
        .bind(qualification.request_id)
        .fetch_one(self.pool())
        .await
        .map_err(operation_error)?;
        let stored_result: CancerResearchLadderResult =
            serde_json::from_value(stored_result_payload).map_err(corrupt)?;
        let stored_contribution = &stored_result
            .receipt
            .ok_or_else(|| corrupt("molecular qualification source omitted its receipt"))?
            .contribution;
        if stored_contribution != contribution {
            return Err(StoreError::Corrupt(
                "patient-derived qualification contribution differs from immutable research result"
                    .to_owned(),
            ));
        }
        let inserted = sqlx::query(
            r#"
            INSERT INTO cancer_patient_derived_molecular_qualifications (
                qualification_id,world_id,request_id,method_version,artifact_hash,
                source_artifact_hash,result_payload,result_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            ON CONFLICT (request_id,method_version) DO NOTHING
            "#,
        )
        .bind(qualification.qualification_id)
        .bind(qualification.world_id.as_uuid())
        .bind(qualification.request_id)
        .bind(i32::from(qualification.method_version))
        .bind(qualification.artifact_hash.as_bytes().as_slice())
        .bind(qualification.source.content_hash.as_bytes().as_slice())
        .bind(&payload)
        .bind(checksum.as_bytes().as_slice())
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        if inserted.rows_affected() == 1 {
            return Ok(());
        }
        let existing = sqlx::query_as::<_, (Uuid, Value, Vec<u8>)>(
            "SELECT qualification_id,result_payload,result_checksum FROM cancer_patient_derived_molecular_qualifications WHERE request_id=$1 AND method_version=$2",
        )
        .bind(qualification.request_id)
        .bind(i32::from(qualification.method_version))
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        match existing {
            Some((qualification_id, stored_payload, stored_checksum))
                if qualification_id == qualification.qualification_id
                    && stored_payload == payload
                    && digest_from_db(
                        &stored_checksum,
                        "patient-derived molecular qualification checksum",
                    )? == checksum =>
            {
                Ok(())
            }
            Some(_) => Err(StoreError::Corrupt(format!(
                "patient-derived molecular qualification {} conflicts with its durable result",
                qualification.qualification_id
            ))),
            None => Err(StoreError::Conflict(
                "patient-derived molecular qualification disappeared during idempotency check"
                    .to_owned(),
            )),
        }
    }

    async fn load_unqualified_cancer_tcga_gbm_target_context(
        &self,
        world_id: world_domain::WorldId,
        method_version: u16,
        limit: usize,
    ) -> Result<Vec<CancerTcgaGbmTargetContextCandidate>, StoreError> {
        if method_version == 0 {
            return Err(StoreError::Conflict(
                "TCGA-GBM target-context method version must be nonzero".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, PriorResearchResultRow>(
            r#"
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN cancer_tcga_gbm_target_context_qualifications AS qualification
              ON qualification.request_id=request.request_id
             AND qualification.method_version=$2
            WHERE request.world_id=$1
              AND result.result_payload->'receipt' <> 'null'::JSONB
              AND JSONB_TYPEOF(
                  result.result_payload->'receipt'->'contribution'->'molecular_targets'
              )='array'
              AND JSONB_ARRAY_LENGTH(
                  result.result_payload->'receipt'->'contribution'->'molecular_targets'
              ) > 0
              AND qualification.qualification_id IS NULL
            ORDER BY request.ordinal,request.request_id
            LIMIT $3
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i32::from(method_version))
        .bind(i64::try_from(limit.clamp(1, 64)).map_err(corrupt)?)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let (request, result) = parse_historical_research_result(row, world_id)?;
            let contribution = result
                .receipt
                .ok_or_else(|| corrupt("TCGA target-context candidate omitted its receipt"))?
                .contribution;
            if contribution.molecular_targets.is_empty() {
                return Err(corrupt(
                    "TCGA target-context candidate omitted structured targets",
                ));
            }
            let artifact_hash = contribution.canonical_hash().map_err(corrupt)?;
            candidates.push(CancerTcgaGbmTargetContextCandidate {
                world_id,
                request_id: request.request_id,
                artifact_hash,
                contribution,
            });
        }
        Ok(candidates)
    }

    async fn store_cancer_tcga_gbm_target_context_qualification(
        &self,
        qualification: &CancerTcgaGbmTargetContextQualification,
        contribution: &world_domain::CancerResearchContribution,
    ) -> Result<(), StoreError> {
        qualification
            .validate_against(contribution)
            .map_err(corrupt)?;
        let payload = serde_json::to_value(qualification).map_err(corrupt)?;
        let checksum = qualification
            .canonical_hash(contribution)
            .map_err(corrupt)?;
        let stored_result_payload: Value = sqlx::query_scalar(
            "SELECT result_payload FROM cancer_research_results WHERE request_id=$1",
        )
        .bind(qualification.request_id)
        .fetch_one(self.pool())
        .await
        .map_err(operation_error)?;
        let stored_result: CancerResearchLadderResult =
            serde_json::from_value(stored_result_payload).map_err(corrupt)?;
        let stored_contribution = &stored_result
            .receipt
            .ok_or_else(|| corrupt("TCGA target-context source omitted its receipt"))?
            .contribution;
        if stored_contribution != contribution {
            return Err(StoreError::Corrupt(
                "TCGA target-context contribution differs from immutable research result"
                    .to_owned(),
            ));
        }
        let inserted = sqlx::query(
            r#"
            INSERT INTO cancer_tcga_gbm_target_context_qualifications (
                qualification_id,world_id,request_id,method_version,artifact_hash,
                source_artifact_hash,result_payload,result_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            ON CONFLICT (request_id,method_version) DO NOTHING
            "#,
        )
        .bind(qualification.qualification_id)
        .bind(qualification.world_id.as_uuid())
        .bind(qualification.request_id)
        .bind(i32::from(qualification.method_version))
        .bind(qualification.artifact_hash.as_bytes().as_slice())
        .bind(qualification.source.content_hash.as_bytes().as_slice())
        .bind(&payload)
        .bind(checksum.as_bytes().as_slice())
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        if inserted.rows_affected() == 1 {
            return Ok(());
        }
        let existing = sqlx::query_as::<_, (Uuid, Value, Vec<u8>)>(
            "SELECT qualification_id,result_payload,result_checksum FROM cancer_tcga_gbm_target_context_qualifications WHERE request_id=$1 AND method_version=$2",
        )
        .bind(qualification.request_id)
        .bind(i32::from(qualification.method_version))
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        match existing {
            Some((qualification_id, stored_payload, stored_checksum))
                if qualification_id == qualification.qualification_id
                    && stored_payload == payload
                    && digest_from_db(&stored_checksum, "TCGA target-context checksum")?
                        == checksum =>
            {
                Ok(())
            }
            Some(_) => Err(StoreError::Corrupt(format!(
                "TCGA target-context qualification {} conflicts with its durable result",
                qualification.qualification_id
            ))),
            None => Err(StoreError::Conflict(
                "TCGA target-context qualification disappeared during idempotency check".to_owned(),
            )),
        }
    }

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

    async fn load_existing_cancer_research_request(
        &self,
        world_id: world_domain::WorldId,
        ordinal: u32,
    ) -> Result<Option<CancerResearchModelRequest>, StoreError> {
        let rows = sqlx::query_as::<_, (Value, Vec<u8>)>(
            r#"
            SELECT request_payload,request_checksum
            FROM cancer_research_requests
            WHERE world_id=$1 AND ordinal=$2
            ORDER BY request_id
            LIMIT 2
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i64::from(ordinal))
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        match rows.as_slice() {
            [] => Ok(None),
            [(payload, checksum)] => {
                let request: CancerResearchModelRequest =
                    serde_json::from_value(payload.clone()).map_err(corrupt)?;
                request.validate().map_err(corrupt)?;
                if request.selection.world_id != world_id
                    || request.selection.ordinal != ordinal
                    || request.canonical_hash().map_err(corrupt)?
                        != digest_from_db(checksum, "existing research request checksum")?
                {
                    return Err(corrupt(
                        "existing cancer research request failed its durable provenance",
                    ));
                }
                Ok(Some(request))
            }
            _ => Err(corrupt(
                "multiple cancer research requests share one deterministic world ordinal",
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

    async fn terminally_fail_cancer_research_request(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        failure_class: CancerResearchTerminalFailureClass,
        error: &str,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        let error = error.chars().take(2_048).collect::<String>();
        if error.trim().is_empty() {
            return Err(StoreError::Conflict(
                "terminal research failure text cannot be empty".to_owned(),
            ));
        }
        let request_checksum = entry.request.canonical_hash().map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.request.request_id).await?;
        sqlx::query(
            r#"
            INSERT INTO cancer_research_terminal_failures (
                request_id,request_checksum,failure_class,failure_text,claim_count
            ) VALUES ($1,$2,$3,$4,$5)
            "#,
        )
        .bind(entry.request.request_id)
        .bind(request_checksum.as_bytes().as_slice())
        .bind(failure_class.as_str())
        .bind(&error)
        .bind(i64::from(entry.claim_count))
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        let updated = sqlx::query(
            r#"
            UPDATE cancer_research_requests
            SET completed_at=NOW(),claimed_by=NULL,claimed_at=NULL,last_error=$3
            WHERE request_id=$1 AND claimed_by=$2 AND completed_at IS NULL
            "#,
        )
        .bind(entry.request.request_id)
        .bind(worker_id)
        .bind(&error)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        require_one(
            updated.rows_affected(),
            "terminal research request is not held by this worker",
        )?;
        // Any unresolved paid authorization remains conservative and visible
        // for provider reconciliation; dead-lettering never guesses that an
        // external dispatch was free or releases potentially incurred spend.
        transaction.commit().await.map_err(operation_error)
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
        if let Some(receipt) = result.receipt.as_ref() {
            enqueue_cancer_research_memory(&mut transaction, &entry.request, receipt).await?;
        }
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

    async fn load_latest_cancer_research_hypothesis(
        &self,
        world_id: world_domain::WorldId,
        before_ordinal: u32,
        program: world_domain::CancerResearchProgram,
    ) -> Result<Option<CancerResearchPriorResult>, StoreError> {
        let row = sqlx::query_as::<_, PriorResearchResultRow>(
            r#"
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            WHERE request.world_id=$1
              AND request.ordinal < $2
              AND MOD(request.ordinal, 2) = $3
              AND request.stage='blind_discovery'
              AND result.result_payload->'receipt' IS NOT NULL
              AND result.result_payload->'receipt'->'contribution'->>'artifact_kind'='hypothesis'
            ORDER BY request.ordinal DESC, request.request_id
            LIMIT 1
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i64::from(before_ordinal))
        .bind(i64::from(program.ordinal_remainder()))
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let request: CancerResearchModelRequest =
            serde_json::from_value(row.request_payload).map_err(corrupt)?;
        let result: CancerResearchLadderResult =
            serde_json::from_value(row.result_payload).map_err(corrupt)?;
        if request.selection.world_id != world_id
            || request.selection.ordinal >= before_ordinal
            || world_domain::CancerResearchProgram::for_ordinal(request.selection.ordinal)
                != program
            || request.canonical_hash().map_err(corrupt)?
                != digest_from_db(&row.request_checksum, "prior research request checksum")?
            || Digest::canonical(&result).map_err(corrupt)?
                != digest_from_db(&row.result_checksum, "prior research result checksum")?
        {
            return Err(StoreError::Corrupt(
                "promoted cancer research hypothesis failed its durable checksum".to_owned(),
            ));
        }
        let prior = CancerResearchPriorResult { request, result };
        prior.validate().map_err(corrupt)?;
        if prior.request.selection.stage != CancerResearchStage::BlindDiscovery
            || prior.contribution().artifact_kind
                != world_domain::CancerResearchArtifactKind::Hypothesis
        {
            return Err(StoreError::Corrupt(
                "promoted cancer research result is not a blind hypothesis".to_owned(),
            ));
        }
        Ok(Some(prior))
    }

    async fn load_cancer_research_campaign_candidate(
        &self,
        world_id: world_domain::WorldId,
        before_ordinal: u32,
        program: world_domain::CancerResearchProgram,
    ) -> Result<Option<CancerResearchCampaignCandidate>, StoreError> {
        let row = sqlx::query_as::<_, CampaignRootRow>(
            r#"
            WITH child_stats AS MATERIALIZED (
                SELECT
                    child.request_payload->'selection'->>'frozen_candidate_hash'
                        AS frozen_candidate_hash,
                    COUNT(*) AS total_children,
                    COUNT(*) FILTER (
                        WHERE child_result.result_payload->'receipt' <> 'null'::JSONB
                    ) AS successful_children,
                    COUNT(*) FILTER (
                        WHERE child_result.result_payload->'receipt' = 'null'::JSONB
                          AND (child_result.result_payload->>'route_policy_version')::INTEGER
                              IN ($6, $8)
                    ) AS current_policy_failures,
                    BOOL_OR(
                        child.request_payload->'selection'->>'task'
                            = 'interpret_replication_result'
                        AND child_result.result_payload->'receipt' <> 'null'::JSONB
                    ) AS synthesis_complete
                FROM cancer_research_requests AS child
                JOIN cancer_research_results AS child_result USING (request_id)
                WHERE child.world_id=$1
                  AND child.ordinal < $2
                  AND child.stage='independent_replication'
                GROUP BY child.request_payload->'selection'->>'frozen_candidate_hash'
            ),
            selected_candidate AS MATERIALIZED (
                SELECT request.request_id
                FROM cancer_research_requests AS request
                JOIN cancer_research_results AS result USING (request_id)
                JOIN cancer_virtual_experiment_results AS experiment
                  ON experiment.request_id=request.request_id
                 AND experiment.method_version=$4
                JOIN cancer_research_novelty_audits AS audit
                  ON audit.request_id=request.request_id
                 AND audit.method_version=$5
                LEFT JOIN child_stats AS children
                  ON children.frozen_candidate_hash=ENCODE(experiment.artifact_hash, 'hex')
                WHERE request.world_id=$1
                  AND request.ordinal < $2
                  AND MOD(request.ordinal, 2) = $3
                  AND request.stage='blind_discovery'
                  AND result.result_payload->'receipt' <> 'null'::JSONB
                  AND result.result_payload->'receipt'->'contribution'->'virtual_experiment_plan' IS NOT NULL
                  AND experiment.result_payload->>'interpretation' IN (
                      'model_supports_prediction',
                      'model_inconclusive'
                  )
                  AND audit.normalized_status IN ('new_combination','no_close_match_found')
                  AND COALESCE(children.current_policy_failures, 0) < $7
                  AND NOT COALESCE(children.synthesis_complete, FALSE)
                ORDER BY
                    COALESCE(children.successful_children, 0) DESC,
                    COALESCE(children.total_children, 0),
                    (experiment.result_payload->>'interpretation'='model_supports_prediction') DESC,
                    request.ordinal,
                    request.request_id
                LIMIT 1
            )
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum,
                   experiment.result_payload AS experiment_payload,
                   experiment.result_checksum AS experiment_checksum,
                   audit.audit_payload, audit.audit_checksum
            FROM selected_candidate AS selected
            JOIN cancer_research_requests AS request USING (request_id)
            JOIN cancer_research_results AS result USING (request_id)
            JOIN cancer_virtual_experiment_results AS experiment
              ON experiment.request_id=request.request_id
             AND experiment.method_version=$4
            JOIN cancer_research_novelty_audits AS audit
              ON audit.request_id=request.request_id
             AND audit.method_version=$5
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i64::from(before_ordinal))
        .bind(i64::from(program.ordinal_remainder()))
        .bind(i32::from(world_domain::CANCER_VIRTUAL_LAB_METHOD_VERSION))
        .bind(i32::from(
            world_domain::CANCER_RESEARCH_NOVELTY_METHOD_VERSION,
        ))
        .bind(i32::from(
            application::CANCER_RESEARCH_ESCALATION_ROUTE_POLICY_VERSION,
        ))
        .bind(
            i64::try_from(application::CANCER_RESEARCH_CAMPAIGN_MAX_DELIVERY_FAILURES_PER_POLICY)
                .map_err(|_| corrupt("campaign delivery failure limit overflow"))?,
        )
        .bind(i32::from(
            application::CANCER_RESEARCH_EXPLORATION_ROUTE_POLICY_VERSION,
        ))
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        let Some(row) = row else {
            return Ok(None);
        };

        let (root_request, root_result) = parse_historical_research_result(
            PriorResearchResultRow {
                request_payload: row.request_payload,
                request_checksum: row.request_checksum,
                result_payload: row.result_payload,
                result_checksum: row.result_checksum,
            },
            world_id,
        )?;
        if root_request.selection.ordinal >= before_ordinal
            || root_request.selection.stage != CancerResearchStage::BlindDiscovery
            || world_domain::CancerResearchProgram::for_ordinal(root_request.selection.ordinal)
                != program
        {
            return Err(corrupt("campaign root crossed its selection boundary"));
        }
        let root = CancerResearchPriorResult {
            request: root_request,
            result: root_result,
        };
        root.validate().map_err(corrupt)?;
        let root_artifact_hash = root.contribution().canonical_hash().map_err(corrupt)?;
        let root_experiment: CancerVirtualExperimentResult =
            serde_json::from_value(row.experiment_payload).map_err(corrupt)?;
        root_experiment
            .validate_against(root.contribution())
            .map_err(corrupt)?;
        if root_experiment.artifact_hash != root_artifact_hash
            || !matches!(
                root_experiment.interpretation,
                world_domain::CancerVirtualExperimentInterpretation::ModelSupportsPrediction
                    | world_domain::CancerVirtualExperimentInterpretation::ModelInconclusive
            )
            || root_experiment
                .canonical_hash(root.contribution())
                .map_err(corrupt)?
                != digest_from_db(
                    &row.experiment_checksum,
                    "campaign root experiment checksum",
                )?
        {
            return Err(corrupt(
                "campaign root virtual experiment failed its durable provenance",
            ));
        }
        let novelty: CancerResearchNoveltyAudit =
            serde_json::from_value(row.audit_payload).map_err(corrupt)?;
        novelty.validate().map_err(corrupt)?;
        if novelty.world_id != world_id
            || novelty.request_id != root.request.request_id
            || novelty.artifact_hash != root_artifact_hash
            || !matches!(
                novelty.status,
                CancerResearchNoveltyStatus::NewCombination
                    | CancerResearchNoveltyStatus::NoCloseMatchFound
            )
            || novelty.canonical_hash().map_err(corrupt)?
                != digest_from_db(&row.audit_checksum, "campaign root novelty checksum")?
        {
            return Err(corrupt(
                "campaign root novelty audit failed its durable provenance",
            ));
        }

        let followup_rows = sqlx::query_as::<_, CampaignFollowupRow>(
            r#"
            SELECT child.request_payload, child.request_checksum,
                   child.completed_at IS NOT NULL AS request_completed,
                   result.result_payload, result.result_checksum,
                   experiment.result_payload AS experiment_payload,
                   experiment.result_checksum AS experiment_checksum
            FROM cancer_research_requests AS child
            LEFT JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN cancer_virtual_experiment_results AS experiment
              ON experiment.request_id=child.request_id
             AND experiment.method_version=$4
            WHERE child.world_id=$1
              AND child.ordinal < $2
              AND child.stage='independent_replication'
              AND child.request_payload->'selection'->>'frozen_candidate_hash'=$3
            ORDER BY child.ordinal, child.request_id
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i64::from(before_ordinal))
        .bind(root_artifact_hash.to_string())
        .bind(i32::from(world_domain::CANCER_VIRTUAL_LAB_METHOD_VERSION))
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;

        let mut followups = Vec::with_capacity(followup_rows.len());
        for row in followup_rows {
            let request: CancerResearchModelRequest =
                serde_json::from_value(row.request_payload).map_err(corrupt)?;
            request.validate().map_err(corrupt)?;
            if request.selection.world_id != world_id
                || request.selection.ordinal >= before_ordinal
                || request.selection.stage != CancerResearchStage::IndependentReplication
                || request.selection.frozen_candidate_hash != Some(root_artifact_hash)
                || request.canonical_hash().map_err(corrupt)?
                    != digest_from_db(&row.request_checksum, "campaign follow-up request checksum")?
            {
                return Err(corrupt(
                    "campaign follow-up request failed its durable provenance",
                ));
            }
            let result = match (row.result_payload, row.result_checksum) {
                (Some(payload), Some(checksum)) => {
                    let result: CancerResearchLadderResult =
                        serde_json::from_value(payload).map_err(corrupt)?;
                    let registry = CognitionRouteRegistry::cancer_research_for_policy(
                        request.route_purpose(),
                        result.route_policy_version,
                    )
                    .map_err(corrupt)?;
                    result
                        .validate_against(&registry, &request)
                        .map_err(corrupt)?;
                    if Digest::canonical(&result).map_err(corrupt)?
                        != digest_from_db(&checksum, "campaign follow-up result checksum")?
                    {
                        return Err(corrupt(
                            "campaign follow-up result failed its durable checksum",
                        ));
                    }
                    Some(result)
                }
                (None, None) => None,
                _ => return Err(corrupt("campaign follow-up result is incomplete")),
            };
            let virtual_experiment = match (row.experiment_payload, row.experiment_checksum) {
                (Some(payload), Some(checksum)) => {
                    let experiment: CancerVirtualExperimentResult =
                        serde_json::from_value(payload).map_err(corrupt)?;
                    let contribution = result
                        .as_ref()
                        .and_then(|result| result.receipt.as_ref())
                        .map(|receipt| &receipt.contribution)
                        .ok_or_else(|| {
                            corrupt("campaign experiment omitted its successful contribution")
                        })?;
                    experiment.validate_against(contribution).map_err(corrupt)?;
                    if experiment.canonical_hash(contribution).map_err(corrupt)?
                        != digest_from_db(&checksum, "campaign follow-up experiment checksum")?
                    {
                        return Err(corrupt(
                            "campaign follow-up experiment failed its durable checksum",
                        ));
                    }
                    Some(experiment)
                }
                (None, None) => None,
                _ => return Err(corrupt("campaign follow-up experiment is incomplete")),
            };
            followups.push(CancerResearchCampaignFollowup {
                request,
                result,
                virtual_experiment,
                request_completed: row.request_completed,
            });
        }
        Ok(Some(CancerResearchCampaignCandidate {
            root,
            root_experiment,
            followups,
        }))
    }

    async fn load_cancer_research_catalog(
        &self,
        world_id: world_domain::WorldId,
        before_ordinal: u32,
        limit: usize,
    ) -> Result<Vec<CancerResearchMemoryInput>, StoreError> {
        let limit = limit.clamp(1, application::MAX_CANCER_RESEARCH_CATALOG_ENTRIES);
        let scan_limit = limit.saturating_mul(16).clamp(limit, 4_096);
        let rows = sqlx::query_as::<_, PriorResearchResultRow>(
            r#"
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            WHERE request.world_id=$1
              AND request.ordinal < $2
              AND request.stage='blind_discovery'
              AND result.result_payload->'receipt' <> 'null'::JSONB
            ORDER BY request.ordinal DESC, request.request_id
            LIMIT $3
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i64::from(before_ordinal))
        .bind(i64::try_from(scan_limit).map_err(|_| corrupt("catalog scan limit overflow"))?)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;

        let mut entries: Vec<CancerResearchCatalogItem> = Vec::with_capacity(limit);
        let mut distinct_contributions: Vec<world_domain::CancerResearchContribution> =
            Vec::with_capacity(limit);
        for row in rows {
            let (request, result) = parse_historical_research_result(row, world_id)?;
            let receipt = result
                .receipt
                .ok_or_else(|| corrupt("successful catalog research result omitted its receipt"))?;
            let contribution = receipt.contribution;
            if distinct_contributions
                .iter()
                .any(|existing| cancer_research_contributions_duplicate(existing, &contribution))
            {
                continue;
            }
            let virtual_experiment = sqlx::query_as::<_, (Value, Vec<u8>)>(
                r#"
                SELECT result_payload,result_checksum
                FROM cancer_virtual_experiment_results
                WHERE request_id=$1 AND method_version=$2
                "#,
            )
            .bind(request.request_id)
            .bind(i32::from(world_domain::CANCER_VIRTUAL_LAB_METHOD_VERSION))
            .fetch_optional(self.pool())
            .await
            .map_err(operation_error)?
            .map(|(payload, checksum)| {
                let experiment: CancerVirtualExperimentResult =
                    serde_json::from_value(payload).map_err(corrupt)?;
                experiment
                    .validate_against(&contribution)
                    .map_err(corrupt)?;
                if experiment.canonical_hash(&contribution).map_err(corrupt)?
                    != digest_from_db(&checksum, "catalog virtual experiment checksum")?
                {
                    return Err(corrupt(
                        "catalog virtual experiment failed its durable checksum",
                    ));
                }
                CancerVirtualExperimentCatalogSummary::from_result(&experiment, &contribution)
                    .map_err(corrupt)
            })
            .transpose()?;
            entries.push(CancerResearchCatalogItem {
                ordinal: request.selection.ordinal,
                contribution_id: contribution.contribution_id,
                artifact_hash: contribution.canonical_hash().map_err(corrupt)?,
                artifact_kind: contribution.artifact_kind,
                title: contribution.title.clone(),
                virtual_experiment,
            });
            distinct_contributions.push(contribution);
            if entries.len() == limit {
                break;
            }
        }
        entries.sort_by_key(|entry| entry.ordinal);
        // Reserve one request-level slot for the program's latest hypothesis,
        // which the scheduler appends after loading this cross-program wiki.
        CancerResearchMemoryInput::from_internal_catalog_pages(
            world_id,
            before_ordinal,
            &entries,
            MAX_CANCER_RESEARCH_MEMORY_INPUTS.saturating_sub(1),
        )
        .map_err(corrupt)
    }

    async fn reserve_paid_cancer_research(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        route: &CognitionModelRoute,
        reserved_micro_usd: u64,
    ) -> Result<CancerResearchPaidReservationDecision, StoreError> {
        validate_worker_id(worker_id)?;
        entry.validate().map_err(corrupt)?;
        entry.request.validate_route(route).map_err(corrupt)?;
        if route.billing_class != CognitionBillingClass::PaidApproved
            || reserved_micro_usd == 0
            || reserved_micro_usd > MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD
        {
            return Err(StoreError::Conflict(
                "paid research reservation is outside the approved route or per-call cap"
                    .to_owned(),
            ));
        }
        let reserved = to_i64(reserved_micro_usd, "paid research reservation")?;
        let scope = CognitionBillingScope::CancerResearch;
        let (target, hard_stop) = scope.monthly_limits_micro_usd();
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.request.request_id).await?;
        let billing_month: NaiveDate =
            sqlx::query_scalar("SELECT date_trunc('month', CURRENT_DATE)::DATE")
                .fetch_one(&mut *transaction)
                .await
                .map_err(operation_error)?;
        sqlx::query(
            r#"
            INSERT INTO cognition_cost_accounts (
                billing_scope,billing_month,target_micro_usd,hard_stop_micro_usd
            ) VALUES ($1,$2,$3,$4)
            ON CONFLICT (billing_scope,billing_month) DO NOTHING
            "#,
        )
        .bind(scope.as_str())
        .bind(billing_month)
        .bind(to_i64(target, "research monthly target")?)
        .bind(to_i64(hard_stop, "research monthly hard stop")?)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        if let Some(existing) =
            fetch_research_reservation(&mut transaction, entry.request.request_id, false).await?
        {
            if existing.billing_month == billing_month
                && existing.reserved_micro_usd == reserved
                && existing.status == "reserved"
                && existing.actual_micro_usd.is_none()
            {
                transaction.commit().await.map_err(operation_error)?;
                return Ok(CancerResearchPaidReservationDecision::Authorized(
                    CancerResearchPaidAuthorization {
                        request_id: entry.request.request_id,
                        billing_month,
                        reserved_micro_usd,
                    },
                ));
            }
            return Err(StoreError::Conflict(
                "research request already has a different paid reservation".to_owned(),
            ));
        }
        let account = sqlx::query_as::<_, (i64, i64, i64)>(
            r#"
            SELECT reserved_micro_usd,spent_micro_usd,hard_stop_micro_usd
            FROM cognition_cost_accounts
            WHERE billing_scope=$1 AND billing_month=$2
            FOR UPDATE
            "#,
        )
        .bind(scope.as_str())
        .bind(billing_month)
        .fetch_one(&mut *transaction)
        .await
        .map_err(operation_error)?;
        let would_use = account
            .0
            .checked_add(account.1)
            .and_then(|used| used.checked_add(reserved))
            .ok_or_else(|| {
                StoreError::Conflict("research cost arithmetic overflowed".to_owned())
            })?;
        if would_use > account.2 {
            transaction.commit().await.map_err(operation_error)?;
            return Ok(CancerResearchPaidReservationDecision::DeniedHardStop);
        }
        sqlx::query(
            r#"
            INSERT INTO cancer_research_cost_reservations (
                request_id,billing_scope,billing_month,reserved_micro_usd,status
            ) VALUES ($1,$2,$3,$4,'reserved')
            "#,
        )
        .bind(entry.request.request_id)
        .bind(scope.as_str())
        .bind(billing_month)
        .bind(reserved)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        sqlx::query(
            r#"
            UPDATE cognition_cost_accounts
            SET reserved_micro_usd=reserved_micro_usd+$3,updated_at=NOW()
            WHERE billing_scope=$1 AND billing_month=$2
            "#,
        )
        .bind(scope.as_str())
        .bind(billing_month)
        .bind(reserved)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)?;
        Ok(CancerResearchPaidReservationDecision::Authorized(
            CancerResearchPaidAuthorization {
                request_id: entry.request.request_id,
                billing_month,
                reserved_micro_usd,
            },
        ))
    }

    async fn load_paid_cancer_research_authorization(
        &self,
        entry: &CancerResearchJobEntry,
    ) -> Result<Option<CancerResearchPaidAuthorization>, StoreError> {
        entry.validate().map_err(corrupt)?;
        let row = fetch_research_reservation_from_pool(self, entry.request.request_id).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.status != "reserved" {
            return Ok(None);
        }
        if row.actual_micro_usd.is_some() {
            return Err(StoreError::Corrupt(
                "active research reservation unexpectedly has an actual cost".to_owned(),
            ));
        }
        let authorization = CancerResearchPaidAuthorization {
            request_id: entry.request.request_id,
            billing_month: row.billing_month,
            reserved_micro_usd: u64::try_from(row.reserved_micro_usd).map_err(|_| {
                StoreError::Corrupt("negative research reservation amount".to_owned())
            })?,
        };
        authorization.validate_against(entry).map_err(corrupt)?;
        Ok(Some(authorization))
    }

    async fn settle_paid_cancer_research(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        authorization: &CancerResearchPaidAuthorization,
        receipt: &CancerResearchModelReceipt,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        authorization.validate_against(entry).map_err(corrupt)?;
        if receipt.request_id != entry.request.request_id
            || receipt.billed_micro_usd > MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD
        {
            return Err(StoreError::Conflict(
                "paid research receipt exceeds the per-call cap or differs from its request"
                    .to_owned(),
            ));
        }
        let receipt_payload = serde_json::to_value(receipt).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.request.request_id).await?;
        let durable_receipt = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT outcome.receipt_payload
            FROM cancer_research_route_outcomes AS outcome
            JOIN cancer_research_route_dispatches AS dispatch
              ON dispatch.request_id=outcome.request_id AND dispatch.route_index=outcome.route_index
            WHERE outcome.request_id=$1 AND outcome.normalized_status='succeeded'
              AND dispatch.billing_class='paid_approved'
            "#,
        )
        .bind(entry.request.request_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(operation_error)?;
        if durable_receipt.as_ref() != Some(&receipt_payload) {
            return Err(StoreError::Conflict(
                "paid research receipt is not the durable successful outcome".to_owned(),
            ));
        }
        resolve_research_reservation(
            &mut transaction,
            authorization,
            ResearchReservationResolution::Settled(receipt.billed_micro_usd),
        )
        .await?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn release_paid_cancer_research(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        authorization: &CancerResearchPaidAuthorization,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        authorization.validate_against(entry).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.request.request_id).await?;
        if !paid_research_release_is_safe(&mut transaction, entry.request.request_id).await? {
            return Err(StoreError::Conflict(
                "a paid research call without an explicit rejected outcome cannot release its reservation"
                    .to_owned(),
            ));
        }
        resolve_research_reservation(
            &mut transaction,
            authorization,
            ResearchReservationResolution::Released,
        )
        .await?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn mark_paid_cancer_research_indeterminate(
        &self,
        worker_id: &str,
        entry: &CancerResearchJobEntry,
        authorization: &CancerResearchPaidAuthorization,
    ) -> Result<(), StoreError> {
        validate_worker_id(worker_id)?;
        authorization.validate_against(entry).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        ensure_claim(&mut transaction, worker_id, entry.request.request_id).await?;
        if !paid_research_was_dispatched(&mut transaction, entry.request.request_id).await? {
            return Err(StoreError::Conflict(
                "an undispatched paid research call cannot become billing-indeterminate".to_owned(),
            ));
        }
        resolve_research_reservation(
            &mut transaction,
            authorization,
            ResearchReservationResolution::Indeterminate,
        )
        .await?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn list_indeterminate_cancer_fireworks_dispatches(
        &self,
        billing_month: NaiveDate,
    ) -> Result<Vec<CancerResearchFireworksDispatchCandidate>, StoreError> {
        if billing_month.day() != 1 {
            return Err(StoreError::Conflict(
                "Fireworks reconciliation billing month must be its first day".to_owned(),
            ));
        }
        let rows = sqlx::query_as::<_, (Uuid, i32, String, DateTime<Utc>, NaiveDate, i64)>(
            r#"
            SELECT dispatch.request_id,dispatch.route_index,dispatch.requested_model,
                   dispatch.dispatched_at,reservation.billing_month,
                   reservation.reserved_micro_usd
            FROM cancer_research_route_dispatches AS dispatch
            JOIN cancer_research_cost_reservations AS reservation
              ON reservation.request_id=dispatch.request_id
            LEFT JOIN cancer_research_fireworks_cost_reconciliations AS reconciliation
              ON reconciliation.request_id=dispatch.request_id
            WHERE reservation.billing_month=$1
              AND reservation.status='indeterminate'
              AND reservation.actual_micro_usd IS NULL
              AND dispatch.provider_slug='fireworks_cancer'
              AND dispatch.billing_class='paid_approved'
              AND dispatch.requested_model IN (
                    'accounts/fireworks/models/gpt-oss-20b',
                    'accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b'
              )
              AND reconciliation.request_id IS NULL
            ORDER BY dispatch.dispatched_at,dispatch.request_id,dispatch.route_index
            "#,
        )
        .bind(billing_month)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        rows.into_iter()
            .map(
                |(
                    request_id,
                    route_index,
                    requested_model,
                    dispatched_at,
                    billing_month,
                    reserved_micro_usd,
                )| {
                    let candidate = CancerResearchFireworksDispatchCandidate {
                        request_id,
                        route_index: u16::try_from(route_index).map_err(|_| {
                            StoreError::Corrupt(
                                "negative Fireworks reconciliation route index".to_owned(),
                            )
                        })?,
                        requested_model,
                        dispatched_at,
                        billing_month,
                        reserved_micro_usd: u64::try_from(reserved_micro_usd).map_err(|_| {
                            StoreError::Corrupt(
                                "negative Fireworks reconciliation reservation".to_owned(),
                            )
                        })?,
                    };
                    candidate.validate().map_err(corrupt)?;
                    Ok(candidate)
                },
            )
            .collect()
    }

    async fn record_cancer_fireworks_cost_reconciliations(
        &self,
        reconciliations: &[CancerResearchFireworksCostReconciliation],
    ) -> Result<(), StoreError> {
        if reconciliations.is_empty() {
            return Err(StoreError::Conflict(
                "Fireworks reconciliation batch is empty".to_owned(),
            ));
        }
        validate_fireworks_reconciliation_batch(reconciliations).map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        // Serialize every importer, including batches whose timestamp windows
        // overlap but whose request rows differ. This keeps the trigger's
        // exactly-one unresolved match stable from pre-read through append.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(0x4154_4657_4249_4C4Ci64)
            .execute(&mut *transaction)
            .await
            .map_err(operation_error)?;
        for reconciliation in reconciliations {
            let candidate = fetch_indeterminate_fireworks_candidate(
                &mut transaction,
                reconciliation.request_id,
                true,
            )
            .await?;
            let Some(candidate) = candidate else {
                let existing = sqlx::query_as::<_, ResearchFireworksReconciliationRow>(
                    r#"
                    SELECT schema_version,reconciliation_id,route_index,billing_month,
                           source_format,export_sha256,export_byte_length,row_sha256,row_start_offset,row_byte_length,
                           provider_started_at,
                           matched_dispatch_at,requested_model,prompt_tokens,completion_tokens,
                           actual_micro_usd,reserved_micro_usd,released_micro_usd
                    FROM cancer_research_fireworks_cost_reconciliations
                    WHERE request_id=$1
                    FOR SHARE
                    "#,
                )
                .bind(reconciliation.request_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(operation_error)?;
                if existing.as_ref().is_some_and(|row| {
                    row.schema_version == i32::from(reconciliation.schema_version)
                        && row.reconciliation_id == reconciliation.reconciliation_id
                        && row.route_index == i32::from(reconciliation.route_index)
                        && row.billing_month == reconciliation.billing_month
                        && row.source_format == reconciliation.source_format
                        && row.export_sha256.as_slice() == reconciliation.export_hash.as_bytes()
                        && u64::try_from(row.export_byte_length).ok()
                            == Some(reconciliation.export_byte_length)
                        && row.row_sha256.as_slice() == reconciliation.row_hash.as_bytes()
                        && u64::try_from(row.row_start_offset).ok()
                            == Some(reconciliation.row_start_offset)
                        && u64::try_from(row.row_byte_length).ok()
                            == Some(reconciliation.row_byte_length)
                        && row.provider_started_at == reconciliation.provider_started_at
                        && row.matched_dispatch_at == reconciliation.matched_dispatch_at
                        && row.requested_model == reconciliation.requested_model
                        && row.prompt_tokens == i64::from(reconciliation.prompt_tokens)
                        && row.completion_tokens == i64::from(reconciliation.completion_tokens)
                        && u64::try_from(row.actual_micro_usd).ok()
                            == Some(reconciliation.actual_micro_usd)
                        && u64::try_from(row.reserved_micro_usd).ok()
                            == Some(reconciliation.reserved_micro_usd)
                        && u64::try_from(row.released_micro_usd).ok()
                            == Some(reconciliation.released_micro_usd)
                }) {
                    continue;
                }
                return Err(StoreError::Conflict(
                    "Fireworks reconciliation has no unique indeterminate dispatch or differs from its prior append"
                        .to_owned(),
                ));
            };
            reconciliation
                .validate_against(&candidate)
                .map_err(corrupt)?;
            sqlx::query(
                r#"
                INSERT INTO cancer_research_fireworks_reconciliation_exports (
                    export_sha256,export_byte_length,source_format
                ) VALUES ($1,$2,$3)
                ON CONFLICT (export_sha256) DO NOTHING
                "#,
            )
            .bind(reconciliation.export_hash.as_bytes().as_slice())
            .bind(to_i64(
                reconciliation.export_byte_length,
                "Fireworks export byte length",
            )?)
            .bind(&reconciliation.source_format)
            .execute(&mut *transaction)
            .await
            .map_err(operation_error)?;
            let export_matches: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM cancer_research_fireworks_reconciliation_exports
                    WHERE export_sha256=$1 AND export_byte_length=$2 AND source_format=$3
                )
                "#,
            )
            .bind(reconciliation.export_hash.as_bytes().as_slice())
            .bind(to_i64(
                reconciliation.export_byte_length,
                "Fireworks export byte length",
            )?)
            .bind(&reconciliation.source_format)
            .fetch_one(&mut *transaction)
            .await
            .map_err(operation_error)?;
            if !export_matches {
                return Err(StoreError::Conflict(
                    "Fireworks export hash already has different provenance".to_owned(),
                ));
            }
            let inserted = sqlx::query(
                r#"
                INSERT INTO cancer_research_fireworks_cost_reconciliations (
                    reconciliation_id,schema_version,request_id,route_index,billing_month,
                    source_format,export_sha256,export_byte_length,row_sha256,row_start_offset,row_byte_length,
                    provider_started_at,matched_dispatch_at,requested_model,prompt_tokens,completion_tokens,
                    actual_micro_usd,reserved_micro_usd,released_micro_usd
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
                "#,
            )
            .bind(reconciliation.reconciliation_id)
            .bind(i32::from(reconciliation.schema_version))
            .bind(reconciliation.request_id)
            .bind(i32::from(reconciliation.route_index))
            .bind(reconciliation.billing_month)
            .bind(&reconciliation.source_format)
            .bind(reconciliation.export_hash.as_bytes().as_slice())
            .bind(to_i64(
                reconciliation.export_byte_length,
                "Fireworks export byte length",
            )?)
            .bind(reconciliation.row_hash.as_bytes().as_slice())
            .bind(to_i64(
                reconciliation.row_start_offset,
                "Fireworks export row offset",
            )?)
            .bind(to_i64(
                reconciliation.row_byte_length,
                "Fireworks export row length",
            )?)
            .bind(reconciliation.provider_started_at)
            .bind(reconciliation.matched_dispatch_at)
            .bind(&reconciliation.requested_model)
            .bind(i64::from(reconciliation.prompt_tokens))
            .bind(i64::from(reconciliation.completion_tokens))
            .bind(to_i64(
                reconciliation.actual_micro_usd,
                "reconciled Fireworks cost",
            )?)
            .bind(to_i64(
                reconciliation.reserved_micro_usd,
                "reconciled Fireworks reservation",
            )?)
            .bind(to_i64(
                reconciliation.released_micro_usd,
                "released Fireworks reservation",
            )?)
            .execute(&mut *transaction)
            .await
            .map_err(operation_error)?;
            require_one(
                inserted.rows_affected(),
                "Fireworks reconciliation lost its append race",
            )?;
        }
        transaction.commit().await.map_err(operation_error)
    }
}

fn parse_historical_research_result(
    row: PriorResearchResultRow,
    world_id: world_domain::WorldId,
) -> Result<(CancerResearchModelRequest, CancerResearchLadderResult), StoreError> {
    let request: CancerResearchModelRequest =
        serde_json::from_value(row.request_payload).map_err(corrupt)?;
    let result: CancerResearchLadderResult =
        serde_json::from_value(row.result_payload).map_err(corrupt)?;
    request.validate().map_err(corrupt)?;
    if request.selection.world_id != world_id
        || request.canonical_hash().map_err(corrupt)?
            != digest_from_db(&row.request_checksum, "catalog request checksum")?
        || Digest::canonical(&result).map_err(corrupt)?
            != digest_from_db(&row.result_checksum, "catalog result checksum")?
        || result.request_id != request.request_id
    {
        return Err(StoreError::Corrupt(
            "catalog research result failed its durable provenance".to_owned(),
        ));
    }
    let receipt = result
        .receipt
        .as_ref()
        .ok_or_else(|| corrupt("successful catalog result omitted its receipt"))?;
    if receipt.request_id != request.request_id
        || receipt.request_hash != request.canonical_hash().map_err(corrupt)?
    {
        return Err(StoreError::Corrupt(
            "catalog research result crossed its immutable request provenance".to_owned(),
        ));
    }
    receipt
        .contribution
        .validate_against(&request.selection)
        .map_err(corrupt)?;
    Ok((request, result))
}

impl PostgresStore {
    /// Idempotently mirrors successful historical Cancer World contributions into
    /// the isolated research Hindsight bank. New results are enqueued in the same
    /// transaction that stores them; this closes the gap for results created
    /// before that invariant existed.
    pub async fn backfill_cancer_research_memories(&self) -> Result<u64, StoreError> {
        // Multiple memory-worker replicas start together. Only one may scan and
        // repair historical gaps; current research completion already mirrors
        // new receipts atomically and does not depend on this operational lock.
        const CANCER_RESEARCH_MEMORY_BACKFILL_LOCK: i64 = 0x4154_4352_4D45_4D31;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        let owns_backfill: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
            .bind(CANCER_RESEARCH_MEMORY_BACKFILL_LOCK)
            .fetch_one(&mut *transaction)
            .await
            .map_err(operation_error)?;
        if !owns_backfill {
            transaction.commit().await.map_err(operation_error)?;
            return Ok(0);
        }
        let rows = sqlx::query_as::<_, PriorResearchResultRow>(
            r#"
            SELECT request.request_payload, request.request_checksum,
                   result.result_payload, result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            WHERE result.result_payload->'receipt' <> 'null'::JSONB
              AND NOT EXISTS (
                  SELECT 1
                  FROM memory_outbox AS memory
                  WHERE memory.world_id=request.world_id
                    AND memory.payload->>'context'='Cancer World research artifact'
                    AND (memory.payload->>'ordinal')::BIGINT=request.ordinal
              )
            ORDER BY request.world_id,request.ordinal,request.request_id
            "#,
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(operation_error)?;
        let mut inserted = 0_u64;
        for row in rows {
            let request: CancerResearchModelRequest =
                serde_json::from_value(row.request_payload).map_err(corrupt)?;
            let result: CancerResearchLadderResult =
                serde_json::from_value(row.result_payload).map_err(corrupt)?;
            if request.canonical_hash().map_err(corrupt)?
                != digest_from_db(&row.request_checksum, "research request checksum")?
                || Digest::canonical(&result).map_err(corrupt)?
                    != digest_from_db(&row.result_checksum, "research result checksum")?
            {
                return Err(StoreError::Corrupt(
                    "historical cancer research failed its durable checksum".to_owned(),
                ));
            }
            let receipt = result.receipt.as_ref().ok_or_else(|| {
                StoreError::Corrupt(
                    "successful historical research result omitted its receipt".to_owned(),
                )
            })?;
            if result.request_id != request.request_id
                || receipt.request_id != request.request_id
                || receipt.request_hash != request.canonical_hash().map_err(corrupt)?
            {
                return Err(StoreError::Corrupt(
                    "historical research result crossed its immutable request provenance"
                        .to_owned(),
                ));
            }
            receipt
                .contribution
                .validate_against(&request.selection)
                .map_err(corrupt)?;
            inserted += u64::from(
                enqueue_cancer_research_memory(&mut transaction, &request, receipt).await?,
            );
        }
        transaction.commit().await.map_err(operation_error)?;
        Ok(inserted)
    }
}

async fn enqueue_cancer_research_memory(
    transaction: &mut Transaction<'_, Postgres>,
    request: &CancerResearchModelRequest,
    receipt: &CancerResearchModelReceipt,
) -> Result<bool, StoreError> {
    receipt
        .contribution
        .validate_against(&request.selection)
        .map_err(corrupt)?;
    let source_sequence: i64 = sqlx::query_scalar(
        r#"
        SELECT sequence
        FROM event_batches
        WHERE world_id=$1 AND tick <= $2
        ORDER BY tick DESC, sequence DESC
        LIMIT 1
        "#,
    )
    .bind(request.selection.world_id.as_uuid())
    .bind(to_i64(
        request.selection.selected_at_tick.get(),
        "research memory selected tick",
    )?)
    .fetch_one(&mut **transaction)
    .await
    .map_err(operation_error)?;
    let retain = MemoryRetain::new(
        request.selection.world_id,
        cancer_research_collective_id(request.selection.world_id),
        EventSequence::new(
            u64::try_from(source_sequence)
                .map_err(|_| corrupt("research memory source sequence is negative"))?,
        ),
        request.selection.selected_at_tick,
        request.selection.ordinal,
        serde_json::to_string(&receipt.contribution).map_err(corrupt)?,
        "Cancer World research artifact",
    )
    .map_err(corrupt)?;
    let payload = serde_json::to_value(&retain).map_err(corrupt)?;
    let inserted = sqlx::query(
        r#"
        INSERT INTO memory_outbox (
            operation_id, document_id, world_id, agent_id, source_sequence,
            bank_id, payload_version, payload, available_at
        )
        -- Research is a tiny collective stream beside a very large perception
        -- stream. The existing available-at index provides bounded priority
        -- without a per-claim sort across the entire general-memory backlog.
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'epoch'::TIMESTAMPTZ)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(retain.operation_id)
    .bind(retain.document_id)
    .bind(retain.world_id.as_uuid())
    .bind(retain.agent_id.as_uuid())
    .bind(source_sequence)
    .bind(&retain.bank_id)
    .bind(i32::from(retain.payload_version))
    .bind(&payload)
    .execute(&mut **transaction)
    .await
    .map_err(operation_error)?;
    if inserted.rows_affected() == 1 {
        return Ok(true);
    }

    // `memory_outbox` has both an operation primary key and a document identity
    // uniqueness boundary. A concurrent speculative insert may meet either one
    // first, so catch both and then prove that the winner stored identical
    // immutable provenance rather than silently accepting a collision.
    let conflicts = sqlx::query_as::<_, ResearchMemoryMirrorRow>(
        r#"
        SELECT operation_id,document_id,world_id,agent_id,source_sequence,
               bank_id,payload_version,payload
        FROM memory_outbox
        WHERE operation_id=$1
           OR (bank_id=$2 AND document_id=$3 AND payload_version=$4)
        ORDER BY operation_id
        LIMIT 2
        "#,
    )
    .bind(retain.operation_id)
    .bind(&retain.bank_id)
    .bind(retain.document_id)
    .bind(i32::from(retain.payload_version))
    .fetch_all(&mut **transaction)
    .await
    .map_err(operation_error)?;
    if conflicts.len() != 1 {
        return Err(StoreError::Corrupt(format!(
            "research memory {} conflicts with multiple durable identities",
            retain.operation_id
        )));
    }
    let existing = &conflicts[0];
    if existing.operation_id != retain.operation_id
        || existing.document_id != retain.document_id
        || existing.world_id != retain.world_id.as_uuid()
        || existing.agent_id != retain.agent_id.as_uuid()
        || existing.source_sequence != source_sequence
        || existing.bank_id != retain.bank_id
        || existing.payload_version != i32::from(retain.payload_version)
        || existing.payload != payload
    {
        return Err(StoreError::Corrupt(format!(
            "research memory {} conflicts with its immutable mirror",
            retain.operation_id
        )));
    }
    Ok(false)
}

async fn fetch_research_reservation_from_pool(
    store: &PostgresStore,
    request_id: Uuid,
) -> Result<Option<ResearchCostReservationRow>, StoreError> {
    sqlx::query_as::<_, ResearchCostReservationRow>(
        r#"
        SELECT billing_month,reserved_micro_usd,status,actual_micro_usd
        FROM cancer_research_cost_reservations WHERE request_id=$1
        "#,
    )
    .bind(request_id)
    .fetch_optional(store.pool())
    .await
    .map_err(operation_error)
}

async fn fetch_research_reservation(
    transaction: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
    for_update: bool,
) -> Result<Option<ResearchCostReservationRow>, StoreError> {
    let query = if for_update {
        r#"
        SELECT billing_month,reserved_micro_usd,status,actual_micro_usd
        FROM cancer_research_cost_reservations WHERE request_id=$1 FOR UPDATE
        "#
    } else {
        r#"
        SELECT billing_month,reserved_micro_usd,status,actual_micro_usd
        FROM cancer_research_cost_reservations WHERE request_id=$1
        "#
    };
    sqlx::query_as::<_, ResearchCostReservationRow>(query)
        .bind(request_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(operation_error)
}

async fn fetch_indeterminate_fireworks_candidate(
    transaction: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
    for_update: bool,
) -> Result<Option<CancerResearchFireworksDispatchCandidate>, StoreError> {
    let query = if for_update {
        r#"
        SELECT dispatch.request_id,dispatch.route_index,dispatch.requested_model,
               dispatch.dispatched_at,reservation.billing_month,reservation.reserved_micro_usd
        FROM cancer_research_route_dispatches AS dispatch
        JOIN cancer_research_cost_reservations AS reservation
          ON reservation.request_id=dispatch.request_id
        LEFT JOIN cancer_research_fireworks_cost_reconciliations AS reconciliation
          ON reconciliation.request_id=dispatch.request_id
        WHERE dispatch.request_id=$1
          AND reservation.status='indeterminate'
          AND reservation.actual_micro_usd IS NULL
          AND dispatch.provider_slug='fireworks_cancer'
          AND dispatch.billing_class='paid_approved'
          AND dispatch.requested_model IN (
                'accounts/fireworks/models/gpt-oss-20b',
                'accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b'
          )
          AND reconciliation.request_id IS NULL
        FOR UPDATE OF reservation,dispatch
        "#
    } else {
        r#"
        SELECT dispatch.request_id,dispatch.route_index,dispatch.requested_model,
               dispatch.dispatched_at,reservation.billing_month,reservation.reserved_micro_usd
        FROM cancer_research_route_dispatches AS dispatch
        JOIN cancer_research_cost_reservations AS reservation
          ON reservation.request_id=dispatch.request_id
        LEFT JOIN cancer_research_fireworks_cost_reconciliations AS reconciliation
          ON reconciliation.request_id=dispatch.request_id
        WHERE dispatch.request_id=$1
          AND reservation.status='indeterminate'
          AND reservation.actual_micro_usd IS NULL
          AND dispatch.provider_slug='fireworks_cancer'
          AND dispatch.billing_class='paid_approved'
          AND dispatch.requested_model IN (
                'accounts/fireworks/models/gpt-oss-20b',
                'accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b'
          )
          AND reconciliation.request_id IS NULL
        "#
    };
    let row = sqlx::query_as::<_, (Uuid, i32, String, DateTime<Utc>, NaiveDate, i64)>(query)
        .bind(request_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(operation_error)?;
    row.map(
        |(
            request_id,
            route_index,
            requested_model,
            dispatched_at,
            billing_month,
            reserved_micro_usd,
        )| {
            let candidate = CancerResearchFireworksDispatchCandidate {
                request_id,
                route_index: u16::try_from(route_index).map_err(|_| {
                    StoreError::Corrupt("negative Fireworks reconciliation route index".to_owned())
                })?,
                requested_model,
                dispatched_at,
                billing_month,
                reserved_micro_usd: u64::try_from(reserved_micro_usd).map_err(|_| {
                    StoreError::Corrupt("negative Fireworks reconciliation reservation".to_owned())
                })?,
            };
            candidate.validate().map_err(corrupt)?;
            Ok(candidate)
        },
    )
    .transpose()
}

enum ResearchReservationResolution {
    Settled(u64),
    Released,
    Indeterminate,
}

async fn resolve_research_reservation(
    transaction: &mut Transaction<'_, Postgres>,
    authorization: &CancerResearchPaidAuthorization,
    resolution: ResearchReservationResolution,
) -> Result<(), StoreError> {
    let existing = fetch_research_reservation(transaction, authorization.request_id, true)
        .await?
        .ok_or_else(|| StoreError::Conflict("paid research reservation is missing".to_owned()))?;
    let reserved = to_i64(
        authorization.reserved_micro_usd,
        "paid research reservation",
    )?;
    if existing.billing_month != authorization.billing_month
        || existing.reserved_micro_usd != reserved
        || existing.status != "reserved"
        || existing.actual_micro_usd.is_some()
    {
        return Err(StoreError::Conflict(
            "paid research reservation is not the active authorization".to_owned(),
        ));
    }
    let (status, actual, release_reserved, add_spent) = match resolution {
        ResearchReservationResolution::Settled(actual) => {
            let actual = to_i64(actual, "paid research actual cost")?;
            if actual
                > to_i64(
                    MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD,
                    "paid research per-call cap",
                )?
            {
                return Err(StoreError::Conflict(
                    "paid research cost exceeds the per-call cap".to_owned(),
                ));
            }
            ("settled", Some(actual), reserved, actual)
        }
        ResearchReservationResolution::Released => ("released", None, reserved, 0),
        ResearchReservationResolution::Indeterminate => ("indeterminate", None, 0, 0),
    };
    if release_reserved != 0 || add_spent != 0 {
        let updated = sqlx::query(
            r#"
            UPDATE cognition_cost_accounts
            SET reserved_micro_usd=reserved_micro_usd-$2,
                spent_micro_usd=spent_micro_usd+$3,updated_at=NOW()
            WHERE billing_scope='cancer_research' AND billing_month=$1
              AND reserved_micro_usd >= $2
            "#,
        )
        .bind(authorization.billing_month)
        .bind(release_reserved)
        .bind(add_spent)
        .execute(&mut **transaction)
        .await
        .map_err(operation_error)?;
        require_one(
            updated.rows_affected(),
            "research cost account cannot resolve reservation",
        )?;
    }
    let updated = sqlx::query(
        r#"
        UPDATE cancer_research_cost_reservations
        SET status=$2,actual_micro_usd=$3,resolved_at=NOW()
        WHERE request_id=$1 AND status='reserved'
        "#,
    )
    .bind(authorization.request_id)
    .bind(status)
    .bind(actual)
    .execute(&mut **transaction)
    .await
    .map_err(operation_error)?;
    require_one(
        updated.rows_affected(),
        "research reservation lost its resolution race",
    )
}

async fn paid_research_was_dispatched(
    transaction: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM cancer_research_route_dispatches
            WHERE request_id=$1 AND billing_class='paid_approved'
        )
        "#,
    )
    .bind(request_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(operation_error)
}

async fn paid_research_release_is_safe(
    transaction: &mut Transaction<'_, Postgres>,
    request_id: Uuid,
) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        r#"
        SELECT NOT EXISTS (
            SELECT 1
            FROM cancer_research_route_dispatches AS dispatch
            LEFT JOIN cancer_research_route_outcomes AS outcome
              ON outcome.request_id=dispatch.request_id AND outcome.route_index=dispatch.route_index
            WHERE dispatch.request_id=$1
              AND dispatch.billing_class='paid_approved'
              AND (
                  outcome.request_id IS NULL
                  OR outcome.normalized_status <> 'rejected'
              )
        )
        "#,
    )
    .bind(request_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(operation_error)
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

const fn novelty_status_text(status: CancerResearchNoveltyStatus) -> &'static str {
    match status {
        CancerResearchNoveltyStatus::KnownOverlap => "known_overlap",
        CancerResearchNoveltyStatus::NewCombination => "new_combination",
        CancerResearchNoveltyStatus::NoCloseMatchFound => "no_close_match_found",
        CancerResearchNoveltyStatus::PossibleError => "possible_error",
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
        if code.as_deref() == Some("40001") {
            return StoreError::Unavailable(database.message().to_owned());
        }
        if matches!(code.as_deref(), Some("23503" | "23505" | "23514" | "P0001")) {
            return StoreError::Conflict(database.message().to_owned());
        }
    }
    StoreError::Unavailable(error.to_string())
}

#[cfg(test)]
mod tests {
    use application::{
        CANCER_RESEARCH_MODEL_CONTRACT_VERSION, CancerResearchEvidenceDocument,
        CancerResearchModel, CancerResearchModelAdapters, CancerResearchModelError,
        CancerResearchModelRequest, CancerResearchWorkerConfiguration, CancerResearchWorkerOutcome,
        CognitionProviderId, ModelTokenUsage, process_next_cancer_research_job,
    };
    use std::{collections::BTreeMap, sync::Arc};
    use uuid::Uuid;
    use world_domain::{
        CancerResearchArtifactKind, CancerResearchClaim, CancerResearchContribution,
        CancerResearchInferenceTier, CancerResearchProfile, CancerResearchTarget,
        CancerResearchTask, CancerResearchTurnSelection, EntityId, SimTick, WorldId, WorldSeed,
    };

    use super::*;

    struct SuccessfulResearchModel;

    #[async_trait::async_trait]
    impl CancerResearchModel for SuccessfulResearchModel {
        async fn infer_research(
            &self,
            route: &CognitionModelRoute,
            request: &CancerResearchModelRequest,
        ) -> Result<CancerResearchModelReceipt, CancerResearchModelError> {
            let contribution = CancerResearchContribution::new(
                &request.selection,
                CancerResearchArtifactKind::Hypothesis,
                "Worker-generated bounded hypothesis",
                "A deterministic fake provider response used to exercise the worker boundary.",
                vec![CancerResearchClaim {
                    statement: "A reversible state may affect the bounded phenotype.".to_owned(),
                    testable_prediction: "A perturbation changes the assay readout.".to_owned(),
                    falsification_test: "The preregistered perturbation has no effect.".to_owned(),
                    citation_hashes: Vec::new(),
                }],
            )
            .map_err(|error| CancerResearchModelError::InvalidResponse(error.to_string()))?;
            Ok(CancerResearchModelReceipt {
                contract_version: CANCER_RESEARCH_MODEL_CONTRACT_VERSION,
                request_id: request.request_id,
                request_hash: request
                    .canonical_hash()
                    .map_err(|error| CancerResearchModelError::Rejected(error.to_string()))?,
                provider: route.provider.clone(),
                requested_model: route.requested_model.clone(),
                resolved_model: route.requested_model.clone(),
                provider_response_id: "fake-worker-response".to_owned(),
                usage: ModelTokenUsage {
                    prompt_tokens: 100,
                    completion_tokens: 100,
                },
                billed_micro_usd: 0,
                contribution,
                provider_response_hash: Digest::sha256(b"fake worker provider response"),
                adapter_version: "fake-worker-v1".to_owned(),
            })
        }
    }

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
        sqlx::query(
            r#"
            INSERT INTO event_batches (
                world_id,sequence,tick,event_schema_version,ruleset_version,payload,
                checksum,previous_checksum,post_state_checksum
            ) VALUES ($1,1,0,1,37,'{}'::JSONB,$2,$2,$2)
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(&zero)
        .execute(store.pool())
        .await
        .expect("insert test event provenance");

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
        let insert_mirror = |store: PostgresStore,
                             request: CancerResearchModelRequest,
                             receipt: CancerResearchModelReceipt| async move {
            let mut transaction = store.pool().begin().await.map_err(operation_error)?;
            let inserted =
                enqueue_cancer_research_memory(&mut transaction, &request, &receipt).await?;
            transaction.commit().await.map_err(operation_error)?;
            Ok::<bool, StoreError>(inserted)
        };
        let (first_mirror, second_mirror) = tokio::join!(
            insert_mirror(store.clone(), request.clone(), receipt.clone()),
            insert_mirror(store.clone(), request.clone(), receipt.clone()),
        );
        let mut concurrent_outcomes = [
            first_mirror.expect("first concurrent research-memory mirror"),
            second_mirror.expect("second concurrent research-memory mirror"),
        ];
        concurrent_outcomes.sort_unstable();
        assert_eq!(concurrent_outcomes, [false, true]);
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
        let mirrored: (i64, String) = sqlx::query_as(
            "SELECT COUNT(*),MIN(bank_id) FROM memory_outbox WHERE world_id=$1 AND payload->>'context'='Cancer World research artifact'",
        )
        .bind(world_id.as_uuid())
        .fetch_one(store.pool())
        .await
        .expect("research memory mirror");
        assert_eq!(mirrored.0, 1);
        assert_eq!(
            mirrored.1,
            application::cancer_research_memory_bank_id(world_id)
        );
        assert_eq!(
            store
                .backfill_cancer_research_memories()
                .await
                .expect("idempotent research-memory backfill"),
            0
        );
        assert_eq!(
            store
                .load_cancer_research_result(request.request_id)
                .await
                .expect("load"),
            Some(result.clone())
        );
        assert_eq!(
            store
                .load_latest_cancer_research_hypothesis(
                    world_id,
                    0,
                    world_domain::CancerResearchProgram::Devices,
                )
                .await
                .expect("no prior hypothesis at ordinal zero"),
            None
        );
        let promoted = store
            .load_latest_cancer_research_hypothesis(
                world_id,
                1,
                world_domain::CancerResearchProgram::Devices,
            )
            .await
            .expect("load prior hypothesis")
            .expect("successful hypothesis exists");
        promoted.validate().expect("promoted result validates");
        assert_eq!(promoted.request, request);
        assert_eq!(promoted.result, result);
        let mutation = sqlx::query(
            "UPDATE cancer_research_results SET route_policy_version=99 WHERE request_id=$1",
        )
        .bind(request.request_id)
        .execute(store.pool())
        .await;
        assert!(mutation.is_err());

        let paid_selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(30),
            SimTick::new(40),
            1,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::LiteratureAudit,
            CancerResearchTask::ChallengeFrozenHypothesis,
            CancerResearchInferenceTier::Escalation,
            CancerResearchProfile::seeded(WorldSeed::new(37), resident_id).expect("profile"),
            Vec::new(),
            Some(Digest::sha256(b"frozen candidate")),
            2_048,
        )
        .expect("paid selection");
        let paid_request = CancerResearchModelRequest::new(
            paid_selection,
            Vec::<CancerResearchEvidenceDocument>::new(),
            Vec::new(),
        )
        .expect("paid request");
        store
            .enqueue_cancer_research_request(&paid_request)
            .await
            .expect("enqueue paid request");
        let paid_entry = store
            .claim_next_cancer_research_request("research-test-worker", 60)
            .await
            .expect("claim paid")
            .expect("paid job");
        let paid_registry = CognitionRouteRegistry::cancer_research_escalation();
        let (paid_route_index, paid_route) = paid_registry
            .routes
            .iter()
            .enumerate()
            .find(|(_, route)| route.billing_class == CognitionBillingClass::PaidApproved)
            .map(|(index, route)| {
                (
                    u16::try_from(index).expect("paid route index"),
                    route.clone(),
                )
            })
            .expect("escalation registry contains a paid route");
        let authorization = match store
            .reserve_paid_cancer_research(
                "research-test-worker",
                &paid_entry,
                &paid_route,
                MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD,
            )
            .await
            .expect("reserve")
        {
            CancerResearchPaidReservationDecision::Authorized(authorization) => authorization,
            CancerResearchPaidReservationDecision::DeniedHardStop => {
                panic!("fresh test treasury unexpectedly denied")
            }
        };
        store
            .begin_cancer_research_route_attempt(
                "research-test-worker",
                &paid_entry,
                paid_route_index,
                &paid_route,
            )
            .await
            .expect("paid dispatch");
        let paid_contribution = CancerResearchContribution::new(
            &paid_entry.request.selection,
            CancerResearchArtifactKind::LiteratureAudit,
            "A bounded candidate challenge",
            "The frozen candidate remains unverified after a focused challenge.",
            vec![CancerResearchClaim {
                statement: "The candidate may fail under an alternative mechanism.".to_owned(),
                testable_prediction: "The alternative predicts a different assay response."
                    .to_owned(),
                falsification_test:
                    "Both mechanisms produce indistinguishable preregistered responses.".to_owned(),
                citation_hashes: Vec::new(),
            }],
        )
        .expect("paid contribution");
        let paid_receipt = CancerResearchModelReceipt {
            contract_version: CANCER_RESEARCH_MODEL_CONTRACT_VERSION,
            request_id: paid_request.request_id,
            request_hash: paid_request.canonical_hash().expect("paid request hash"),
            provider: paid_route.provider.clone(),
            requested_model: paid_route.requested_model.clone(),
            resolved_model: paid_route.requested_model.clone(),
            provider_response_id: "paid-test-response".to_owned(),
            usage: ModelTokenUsage {
                prompt_tokens: 200,
                completion_tokens: 150,
            },
            billed_micro_usd: 1_000,
            contribution: paid_contribution,
            provider_response_hash: Digest::sha256(b"paid provider response"),
            adapter_version: "test-adapter-v1".to_owned(),
        };
        let paid_attempt = CognitionRouteAttempt {
            route_index: paid_route_index,
            provider: paid_route.provider.clone(),
            requested_model: paid_route.requested_model.clone(),
            billing_class: paid_route.billing_class,
            status: CognitionRouteAttemptStatus::Succeeded,
        };
        store
            .finish_cancer_research_route_attempt(
                "research-test-worker",
                &paid_entry,
                &paid_attempt,
                Some(&paid_receipt),
            )
            .await
            .expect("paid outcome");
        store
            .settle_paid_cancer_research(
                "research-test-worker",
                &paid_entry,
                &authorization,
                &paid_receipt,
            )
            .await
            .expect("settle paid receipt");
        let account = sqlx::query_as::<_, (i64, i64)>(
            "SELECT reserved_micro_usd,spent_micro_usd FROM cognition_cost_accounts WHERE billing_scope='cancer_research' AND billing_month=$1",
        )
        .bind(authorization.billing_month)
        .fetch_one(store.pool())
        .await
        .expect("research account");
        assert_eq!(account, (0, 1_000));

        let paid_result = CancerResearchLadderResult {
            contract_version: CANCER_RESEARCH_MODEL_CONTRACT_VERSION,
            request_id: paid_request.request_id,
            route_policy_version: paid_registry.policy_version,
            route_registry_hash: paid_registry
                .canonical_hash(paid_request.route_purpose())
                .expect("paid registry hash"),
            attempts: paid_registry
                .routes
                .iter()
                .enumerate()
                .take(usize::from(paid_route_index))
                .map(|(route_index, route)| CognitionRouteAttempt {
                    route_index: u16::try_from(route_index).expect("route index"),
                    provider: route.provider.clone(),
                    requested_model: route.requested_model.clone(),
                    billing_class: route.billing_class,
                    status: CognitionRouteAttemptStatus::SkippedUnconfigured,
                })
                .chain(std::iter::once(paid_attempt))
                .collect(),
            receipt: Some(paid_receipt),
        };
        store
            .complete_cancer_research_request(
                "research-test-worker",
                &paid_entry,
                &paid_registry,
                &paid_result,
            )
            .await
            .expect("complete paid request");

        let worker_selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(50),
            SimTick::new(60),
            2,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::GenerateMechanisticHypothesis,
            CancerResearchInferenceTier::Exploration,
            CancerResearchProfile::seeded(WorldSeed::new(37), resident_id).expect("profile"),
            Vec::new(),
            None,
            2_048,
        )
        .expect("worker selection");
        let worker_request = CancerResearchModelRequest::new(
            worker_selection,
            Vec::<CancerResearchEvidenceDocument>::new(),
            Vec::new(),
        )
        .expect("worker request");
        store
            .enqueue_cancer_research_request(&worker_request)
            .await
            .expect("enqueue worker request");
        let mut adapters: CancerResearchModelAdapters = BTreeMap::new();
        adapters.insert(
            CognitionProviderId::openrouter_cancer(),
            Arc::new(SuccessfulResearchModel),
        );
        let outcome = process_next_cancer_research_job(
            &store,
            &adapters,
            "worker-integration-test",
            &CancerResearchWorkerConfiguration::default(),
        )
        .await
        .expect("process worker request");
        assert_eq!(
            outcome,
            CancerResearchWorkerOutcome::Completed {
                request_id: worker_request.request_id,
                succeeded: true,
            }
        );
        assert!(
            store
                .load_cancer_research_result(worker_request.request_id)
                .await
                .expect("load worker result")
                .is_some()
        );
    }
}
