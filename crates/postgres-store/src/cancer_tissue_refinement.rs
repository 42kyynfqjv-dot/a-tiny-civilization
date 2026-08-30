use application::{
    CancerResearchCampaignDirective, CancerResearchCampaignOutcome, CancerResearchLadderResult,
    CancerResearchModelRequest, CancerTissueRefinementCampaignExperiment,
    CancerTissueRefinementCandidate, CancerTissueRefinementJob, CancerTissueRefinementJobStore,
    CancerTissueRefinementSurvivalEvidence, CancerVirtualExperimentCandidate,
    CognitionRouteRegistry, StoreError, cancer_research_campaign_directive,
    prepare_cancer_tissue_refinement_protocol,
};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;
use world_domain::{
    CANCER_TISSUE_REFINEMENT_METHOD_VERSION, CANCER_VIRTUAL_LAB_METHOD_VERSION,
    CancerResearchStage, CancerResearchTask, CancerTissueRefinementProtocol,
    CancerTissueRefinementResult, CancerVirtualExperimentResult, Digest, WorldId,
};

use crate::PostgresStore;

const TISSUE_SINGLETON_ADVISORY_LOCK: i64 = 0x4154_5449_5353_5545;
const MAX_ELIGIBLE_SYNTHESIS_SCAN: i64 = 64;

#[derive(FromRow)]
struct DurableResearchRow {
    request_payload: Value,
    request_checksum: Vec<u8>,
    result_payload: Value,
    result_checksum: Vec<u8>,
}

#[derive(FromRow)]
struct RootResearchRow {
    completed: bool,
    request_payload: Value,
    request_checksum: Vec<u8>,
    result_payload: Value,
    result_checksum: Vec<u8>,
    experiment_payload: Value,
    experiment_checksum: Vec<u8>,
}

#[derive(FromRow)]
struct CampaignChildRow {
    completed: bool,
    request_payload: Value,
    request_checksum: Vec<u8>,
    result_payload: Option<Value>,
    result_checksum: Option<Vec<u8>>,
    experiment_payload: Option<Value>,
    experiment_checksum: Option<Vec<u8>>,
}

#[derive(FromRow)]
struct TissueJobRow {
    refinement_id: Uuid,
    method_version: i32,
    survival_synthesis_request_id: Uuid,
    protocol_payload: Value,
    protocol_checksum: Vec<u8>,
    claim_token: Uuid,
    claim_count: i64,
}

#[derive(FromRow)]
struct TissueResultRow {
    result_payload: Value,
    result_checksum: Vec<u8>,
}

#[async_trait]
impl CancerTissueRefinementJobStore for PostgresStore {
    async fn claim_next_cancer_tissue_refinement(
        &self,
        world_id: WorldId,
        worker_id: &str,
        lease_seconds: u32,
    ) -> Result<Option<CancerTissueRefinementJob>, StoreError> {
        application::validate_tissue_worker_id(worker_id).map_err(corrupt)?;
        if !(application::MIN_CANCER_TISSUE_REFINEMENT_LEASE_SECONDS
            ..=application::MAX_CANCER_TISSUE_REFINEMENT_LEASE_SECONDS)
            .contains(&lease_seconds)
        {
            return Err(StoreError::Conflict(
                "tissue-refinement lease is outside the approved range".to_owned(),
            ));
        }

        self.admit_eligible_tissue_refinement_protocols(world_id)
            .await?;

        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        let singleton: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
            .bind(TISSUE_SINGLETON_ADVISORY_LOCK)
            .fetch_one(&mut *transaction)
            .await
            .map_err(operation_error)?;
        if !singleton {
            transaction.commit().await.map_err(operation_error)?;
            return Ok(None);
        }
        sqlx::query(
            r#"
            UPDATE cancer_tissue_refinement_jobs
            SET claimed_by=NULL,claimed_at=NULL,lease_until=NULL,claim_token=NULL
            WHERE completed_at IS NULL AND lease_until <= NOW()
            "#,
        )
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        let live_claim: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM cancer_tissue_refinement_jobs WHERE completed_at IS NULL AND claimed_by IS NOT NULL)",
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(operation_error)?;
        if live_claim {
            transaction.commit().await.map_err(operation_error)?;
            return Ok(None);
        }

        let claim_token = Uuid::new_v4();
        let row = sqlx::query_as::<_, TissueJobRow>(
            r#"
            WITH candidate AS (
                SELECT refinement_id
                FROM cancer_tissue_refinement_jobs
                WHERE world_id=$1 AND method_version=$2
                  AND completed_at IS NULL AND available_at <= NOW()
                  AND claimed_by IS NULL
                  AND NOT EXISTS (
                      SELECT 1 FROM cancer_tissue_refinement_results AS result
                      WHERE result.refinement_id=cancer_tissue_refinement_jobs.refinement_id
                  )
                ORDER BY created_at,refinement_id
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE cancer_tissue_refinement_jobs AS job
            SET claimed_by=$3,claimed_at=NOW(),
                lease_until=NOW()+($4::BIGINT*INTERVAL '1 second'),
                claim_token=$5,claim_count=job.claim_count+1,last_error=NULL
            FROM candidate
            WHERE job.refinement_id=candidate.refinement_id
            RETURNING job.refinement_id,job.method_version,
                      job.survival_synthesis_request_id,job.protocol_payload,
                      job.protocol_checksum,job.claim_token,job.claim_count
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i32::from(CANCER_TISSUE_REFINEMENT_METHOD_VERSION))
        .bind(worker_id)
        .bind(i64::from(lease_seconds))
        .bind(claim_token)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.method_version != i32::from(CANCER_TISSUE_REFINEMENT_METHOD_VERSION)
            || row.refinement_id.is_nil()
            || row.claim_token != claim_token
        {
            return Err(corrupt("claimed tissue job identity is invalid"));
        }
        let protocol: CancerTissueRefinementProtocol =
            serde_json::from_value(row.protocol_payload).map_err(corrupt)?;
        protocol.validate().map_err(corrupt)?;
        let protocol_checksum = digest_from_db(&row.protocol_checksum, "tissue protocol checksum")?;
        if protocol.refinement_id != row.refinement_id
            || protocol.canonical_hash().map_err(corrupt)? != protocol_checksum
        {
            return Err(corrupt("claimed tissue protocol failed durable provenance"));
        }
        let candidate = self
            .load_tissue_candidate(world_id, row.survival_synthesis_request_id)
            .await?
            .ok_or_else(|| corrupt("claimed tissue job lost its eligible campaign"))?;
        let expected = prepare_cancer_tissue_refinement_protocol(&candidate).map_err(corrupt)?;
        if expected != protocol {
            return Err(corrupt(
                "claimed tissue protocol differs from immutable campaign evidence",
            ));
        }
        let job = CancerTissueRefinementJob {
            worker_id: worker_id.to_owned(),
            claim_token,
            claim_count: u32::try_from(row.claim_count)
                .map_err(|_| corrupt("tissue claim count overflow"))?,
            candidate,
            protocol,
        };
        job.validate().map_err(corrupt)?;
        Ok(Some(job))
    }

    async fn complete_cancer_tissue_refinement(
        &self,
        job: &CancerTissueRefinementJob,
        result: &CancerTissueRefinementResult,
    ) -> Result<(), StoreError> {
        job.validate().map_err(corrupt)?;
        result.validate_against(&job.protocol).map_err(corrupt)?;
        let payload = serde_json::to_value(result).map_err(corrupt)?;
        let checksum = result.canonical_hash(&job.protocol).map_err(corrupt)?;
        let protocol_checksum = job.protocol.canonical_hash().map_err(corrupt)?;
        let mut transaction = self.pool().begin().await.map_err(operation_error)?;

        if let Some(existing) = sqlx::query_as::<_, TissueResultRow>(
            "SELECT result_payload,result_checksum FROM cancer_tissue_refinement_results WHERE refinement_id=$1",
        )
        .bind(job.protocol.refinement_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(operation_error)?
        {
            if existing.result_payload == payload
                && digest_from_db(&existing.result_checksum, "tissue result checksum")? == checksum
            {
                transaction.commit().await.map_err(operation_error)?;
                return Ok(());
            }
            return Err(corrupt("tissue result conflicts with immutable result bytes"));
        }

        lock_exact_tissue_claim(&mut transaction, job).await?;
        sqlx::query(
            r#"
            INSERT INTO cancer_tissue_refinement_results (
                refinement_id,world_id,method_version,protocol_checksum,
                result_payload,result_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6)
            "#,
        )
        .bind(result.refinement_id)
        .bind(result.world_id.as_uuid())
        .bind(i32::from(result.method_version))
        .bind(protocol_checksum.as_bytes().as_slice())
        .bind(&payload)
        .bind(checksum.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        let updated = sqlx::query(
            r#"
            UPDATE cancer_tissue_refinement_jobs
            SET completed_at=NOW(),claimed_by=NULL,claimed_at=NULL,
                lease_until=NULL,claim_token=NULL,last_error=NULL
            WHERE refinement_id=$1 AND claimed_by=$2 AND claim_token=$3
              AND completed_at IS NULL
            "#,
        )
        .bind(job.protocol.refinement_id)
        .bind(&job.worker_id)
        .bind(job.claim_token)
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        require_one(
            updated.rows_affected(),
            "tissue claim disappeared during completion",
        )?;
        transaction.commit().await.map_err(operation_error)
    }

    async fn fail_cancer_tissue_refinement(
        &self,
        job: &CancerTissueRefinementJob,
        error: &str,
        retry_after_seconds: u32,
    ) -> Result<(), StoreError> {
        job.validate().map_err(corrupt)?;
        let error = error.chars().take(2_048).collect::<String>();
        let retry = i64::from(retry_after_seconds.clamp(1, 300));
        let updated = sqlx::query(
            r#"
            UPDATE cancer_tissue_refinement_jobs
            SET available_at=NOW()+($4::BIGINT*INTERVAL '1 second'),
                claimed_by=NULL,claimed_at=NULL,lease_until=NULL,claim_token=NULL,
                last_error=$5
            WHERE refinement_id=$1 AND claimed_by=$2 AND claim_token=$3
              AND lease_until > NOW() AND completed_at IS NULL
            "#,
        )
        .bind(job.protocol.refinement_id)
        .bind(&job.worker_id)
        .bind(job.claim_token)
        .bind(retry)
        .bind(error)
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        require_one(
            updated.rows_affected(),
            "tissue job is not held by this live claim",
        )
    }
}

impl PostgresStore {
    async fn admit_eligible_tissue_refinement_protocols(
        &self,
        world_id: WorldId,
    ) -> Result<(), StoreError> {
        let rows = sqlx::query_as::<_, DurableResearchRow>(
            r#"
            SELECT request.request_payload,request.request_checksum,
                   result.result_payload,result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN cancer_tissue_refinement_jobs AS job
              ON job.survival_synthesis_request_id=request.request_id
             AND job.method_version=$2
            WHERE request.world_id=$1
              AND request.stage='independent_replication'
              AND request.completed_at IS NOT NULL
              AND request.request_payload->'selection'->>'task'='interpret_replication_result'
              AND result.result_payload->'receipt' <> 'null'::JSONB
              AND job.refinement_id IS NULL
              -- Ineligible synthesis rows are terminal history, not pending
              -- admission work. Filtering before LIMIT prevents the oldest 64
              -- falsified/inconclusive campaigns from starving a later survivor.
              AND EXISTS (
                  SELECT 1
                  FROM JSONB_ARRAY_ELEMENTS(
                      request.request_payload->'evidence_documents'
                  ) AS document
                  WHERE document->'reference'->>'source_id'
                            LIKE 'cancer-world://campaign-directive/%'
                    AND ((document->>'content')::JSONB)->>'phase'='synthesis'
                    AND ((document->>'content')::JSONB)->>'outcome'
                            ='survived_replication_round'
              )
            ORDER BY request.ordinal,request.request_id
            LIMIT $3
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i32::from(CANCER_TISSUE_REFINEMENT_METHOD_VERSION))
        .bind(MAX_ELIGIBLE_SYNTHESIS_SCAN)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        for row in rows {
            let (request, _) = parse_research_row(row, world_id, "tissue synthesis")?;
            let Some(candidate) = self
                .load_tissue_candidate(world_id, request.request_id)
                .await?
            else {
                continue;
            };
            let protocol =
                prepare_cancer_tissue_refinement_protocol(&candidate).map_err(corrupt)?;
            self.store_tissue_protocol(&protocol).await?;
        }
        Ok(())
    }

    async fn store_tissue_protocol(
        &self,
        protocol: &CancerTissueRefinementProtocol,
    ) -> Result<(), StoreError> {
        protocol.validate().map_err(corrupt)?;
        let payload = serde_json::to_value(protocol).map_err(corrupt)?;
        let checksum = protocol.canonical_hash().map_err(corrupt)?;
        let campaign_hashes = protocol
            .campaign_result_hashes
            .iter()
            .map(|hash| hash.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let inserted = sqlx::query(
            r#"
            INSERT INTO cancer_tissue_refinement_jobs (
                refinement_id,world_id,campaign_id,root_request_id,method_version,
                root_artifact_hash,root_plan_hash,root_result_hash,
                survival_synthesis_request_id,survival_synthesis_request_hash,
                survival_synthesis_result_hash,campaign_result_hashes,
                protocol_payload,protocol_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (campaign_id,method_version) DO NOTHING
            "#,
        )
        .bind(protocol.refinement_id)
        .bind(protocol.world_id.as_uuid())
        .bind(protocol.campaign_id)
        .bind(protocol.root_request_id)
        .bind(i32::from(protocol.method_version))
        .bind(protocol.root_artifact_hash.as_bytes().as_slice())
        .bind(protocol.root_plan_hash.as_bytes().as_slice())
        .bind(protocol.root_result_hash.as_bytes().as_slice())
        .bind(protocol.survival_synthesis_request_id)
        .bind(
            protocol
                .survival_synthesis_request_hash
                .as_bytes()
                .as_slice(),
        )
        .bind(
            protocol
                .survival_synthesis_result_hash
                .as_bytes()
                .as_slice(),
        )
        .bind(campaign_hashes)
        .bind(&payload)
        .bind(checksum.as_bytes().as_slice())
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        if inserted.rows_affected() == 1 {
            return Ok(());
        }
        let existing = sqlx::query_as::<_, (Uuid, Value, Vec<u8>)>(
            "SELECT refinement_id,protocol_payload,protocol_checksum FROM cancer_tissue_refinement_jobs WHERE campaign_id=$1 AND method_version=$2",
        )
        .bind(protocol.campaign_id)
        .bind(i32::from(protocol.method_version))
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        match existing {
            Some((refinement_id, existing_payload, existing_checksum))
                if refinement_id == protocol.refinement_id
                    && existing_payload == payload
                    && digest_from_db(&existing_checksum, "stored tissue protocol checksum")?
                        == checksum =>
            {
                Ok(())
            }
            Some(_) => Err(corrupt(
                "tissue campaign conflicts with its frozen protocol",
            )),
            None => Err(StoreError::Conflict(
                "tissue protocol disappeared during idempotency check".to_owned(),
            )),
        }
    }

    async fn load_tissue_candidate(
        &self,
        world_id: WorldId,
        synthesis_request_id: Uuid,
    ) -> Result<Option<CancerTissueRefinementCandidate>, StoreError> {
        let synthesis_row = sqlx::query_as::<_, DurableResearchRow>(
            r#"
            SELECT request.request_payload,request.request_checksum,
                   result.result_payload,result.result_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            WHERE request.request_id=$1 AND request.world_id=$2
            "#,
        )
        .bind(synthesis_request_id)
        .bind(world_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        let Some(synthesis_row) = synthesis_row else {
            return Err(corrupt("tissue synthesis request does not exist"));
        };
        let synthesis_request_hash = digest_from_db(
            &synthesis_row.request_checksum,
            "synthesis request checksum",
        )?;
        let synthesis_result_hash =
            digest_from_db(&synthesis_row.result_checksum, "synthesis result checksum")?;
        let (synthesis_request, synthesis_result) =
            parse_research_row(synthesis_row, world_id, "tissue synthesis")?;
        if synthesis_request.selection.stage != CancerResearchStage::IndependentReplication
            || synthesis_request.selection.task != CancerResearchTask::InterpretReplicationResult
            || synthesis_result.receipt.is_none()
        {
            return Err(corrupt(
                "tissue synthesis row is incomplete or has the wrong task",
            ));
        }
        let directive = cancer_research_campaign_directive(&synthesis_request).map_err(corrupt)?;
        let Some(CancerResearchCampaignDirective::Synthesis {
            campaign_id,
            root_artifact_hash,
            outcome,
            supporting_tests,
            falsifying_tests,
            inconclusive_tests,
            ..
        }) = directive
        else {
            return Err(corrupt(
                "tissue synthesis request omitted its exact directive",
            ));
        };
        if outcome != CancerResearchCampaignOutcome::SurvivedReplicationRound {
            return Ok(None);
        }
        if synthesis_request.selection.frozen_candidate_hash != Some(root_artifact_hash) {
            return Err(corrupt("tissue synthesis crossed its frozen campaign root"));
        }

        let root_rows = sqlx::query_as::<_, RootResearchRow>(
            r#"
            SELECT request.request_payload,request.request_checksum,
                   request.completed_at IS NOT NULL AS completed,
                   result.result_payload,result.result_checksum,
                   experiment.result_payload AS experiment_payload,
                   experiment.result_checksum AS experiment_checksum
            FROM cancer_research_requests AS request
            JOIN cancer_research_results AS result USING (request_id)
            JOIN cancer_virtual_experiment_results AS experiment
              ON experiment.request_id=request.request_id
             AND experiment.method_version=$3
            WHERE request.world_id=$1 AND request.stage='blind_discovery'
              AND experiment.artifact_hash=$2
            ORDER BY request.request_id
            LIMIT 2
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(root_artifact_hash.as_bytes().as_slice())
        .bind(i32::from(CANCER_VIRTUAL_LAB_METHOD_VERSION))
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        let [root_row] = root_rows.as_slice() else {
            return Err(corrupt(
                "tissue campaign does not have exactly one current root",
            ));
        };
        if !root_row.completed {
            return Err(corrupt("tissue campaign root is incomplete"));
        }
        let (root_request, root_result) = parse_research_row(
            DurableResearchRow {
                request_payload: root_row.request_payload.clone(),
                request_checksum: root_row.request_checksum.clone(),
                result_payload: root_row.result_payload.clone(),
                result_checksum: root_row.result_checksum.clone(),
            },
            world_id,
            "tissue root",
        )?;
        let root_contribution = root_result
            .receipt
            .ok_or_else(|| corrupt("tissue root omitted its contribution"))?
            .contribution;
        let root_candidate = CancerVirtualExperimentCandidate {
            world_id,
            request_id: root_request.request_id,
            ordinal: root_request.selection.ordinal,
            artifact_hash: root_contribution.canonical_hash().map_err(corrupt)?,
            contribution: root_contribution,
        };
        let root_experiment: CancerVirtualExperimentResult =
            serde_json::from_value(root_row.experiment_payload.clone()).map_err(corrupt)?;
        root_experiment
            .validate_against(&root_candidate.contribution)
            .map_err(corrupt)?;
        if root_candidate.artifact_hash != root_artifact_hash
            || root_experiment
                .canonical_hash(&root_candidate.contribution)
                .map_err(corrupt)?
                != digest_from_db(&root_row.experiment_checksum, "tissue root result checksum")?
        {
            return Err(corrupt(
                "tissue root failed immutable experiment provenance",
            ));
        }

        let child_rows = sqlx::query_as::<_, CampaignChildRow>(
            r#"
            SELECT child.completed_at IS NOT NULL AS completed,
                   child.request_payload,child.request_checksum,
                   result.result_payload,result.result_checksum,
                   experiment.result_payload AS experiment_payload,
                   experiment.result_checksum AS experiment_checksum
            FROM cancer_research_requests AS child
            LEFT JOIN cancer_research_results AS result USING (request_id)
            LEFT JOIN cancer_virtual_experiment_results AS experiment
              ON experiment.request_id=child.request_id
             AND experiment.method_version=$3
            WHERE child.world_id=$1 AND child.stage='independent_replication'
              AND child.request_payload->'selection'->>'frozen_candidate_hash'=$2
            ORDER BY child.ordinal,child.request_id
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(root_artifact_hash.to_string())
        .bind(i32::from(CANCER_VIRTUAL_LAB_METHOD_VERSION))
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        let mut campaign_experiments = Vec::new();
        let mut synthesis_seen = 0_usize;
        for row in child_rows {
            if !row.completed {
                return Err(corrupt("tissue campaign contains an incomplete follow-up"));
            }
            let (result_payload, result_checksum) = match (row.result_payload, row.result_checksum)
            {
                (Some(payload), Some(checksum)) => (payload, checksum),
                _ => {
                    return Err(corrupt(
                        "tissue campaign follow-up omitted its model result",
                    ));
                }
            };
            let (request, result) = parse_research_row(
                DurableResearchRow {
                    request_payload: row.request_payload,
                    request_checksum: row.request_checksum,
                    result_payload,
                    result_checksum,
                },
                world_id,
                "tissue follow-up",
            )?;
            if request.selection.frozen_candidate_hash != Some(root_artifact_hash) {
                return Err(corrupt("tissue follow-up crossed its frozen root"));
            }
            match request.selection.task {
                CancerResearchTask::DesignIndependentReplication => {
                    let contribution = result
                        .receipt
                        .ok_or_else(|| corrupt("tissue campaign test omitted its contribution"))?
                        .contribution;
                    let experiment = match (row.experiment_payload, row.experiment_checksum) {
                        (Some(payload), Some(checksum)) => {
                            let experiment: CancerVirtualExperimentResult =
                                serde_json::from_value(payload).map_err(corrupt)?;
                            experiment
                                .validate_against(&contribution)
                                .map_err(corrupt)?;
                            if experiment.canonical_hash(&contribution).map_err(corrupt)?
                                != digest_from_db(&checksum, "campaign experiment checksum")?
                            {
                                return Err(corrupt(
                                    "tissue campaign experiment failed its durable checksum",
                                ));
                            }
                            experiment
                        }
                        _ => {
                            return Err(corrupt(
                                "tissue campaign test omitted its current virtual result",
                            ));
                        }
                    };
                    let candidate = CancerVirtualExperimentCandidate {
                        world_id,
                        request_id: request.request_id,
                        ordinal: request.selection.ordinal,
                        artifact_hash: contribution.canonical_hash().map_err(corrupt)?,
                        contribution,
                    };
                    campaign_experiments.push(CancerTissueRefinementCampaignExperiment {
                        frozen_root_artifact_hash: root_artifact_hash,
                        candidate,
                        result: experiment,
                    });
                }
                CancerResearchTask::InterpretReplicationResult => {
                    synthesis_seen += 1;
                    if request.request_id != synthesis_request_id
                        || row.experiment_payload.is_some()
                        || row.experiment_checksum.is_some()
                    {
                        return Err(corrupt(
                            "tissue campaign has multiple or malformed synthesis rows",
                        ));
                    }
                }
                _ => return Err(corrupt("tissue campaign contains an unexpected task")),
            }
        }
        if synthesis_seen != 1
            || campaign_experiments.len()
                != usize::from(supporting_tests) + usize::from(inconclusive_tests)
        {
            return Err(corrupt(
                "tissue campaign durable rows disagree with synthesis test counts",
            ));
        }
        let candidate = CancerTissueRefinementCandidate {
            campaign_id,
            root: root_candidate,
            root_result: root_experiment,
            campaign_experiments,
            survival_evidence: CancerTissueRefinementSurvivalEvidence {
                synthesis_request_id,
                synthesis_request_hash,
                synthesis_result_hash,
                campaign_id,
                root_artifact_hash,
                supporting_tests,
                falsifying_tests,
                inconclusive_tests,
            },
        };
        candidate.validate_survivor().map_err(corrupt)?;
        Ok(Some(candidate))
    }
}

fn parse_research_row(
    row: DurableResearchRow,
    world_id: WorldId,
    label: &str,
) -> Result<(CancerResearchModelRequest, CancerResearchLadderResult), StoreError> {
    let request: CancerResearchModelRequest =
        serde_json::from_value(row.request_payload).map_err(corrupt)?;
    let result: CancerResearchLadderResult =
        serde_json::from_value(row.result_payload).map_err(corrupt)?;
    request.validate().map_err(corrupt)?;
    let registry = CognitionRouteRegistry::cancer_research_for_policy(
        request.route_purpose(),
        result.route_policy_version,
    )
    .map_err(corrupt)?;
    result
        .validate_against(&registry, &request)
        .map_err(corrupt)?;
    if request.selection.world_id != world_id
        || request.canonical_hash().map_err(corrupt)?
            != digest_from_db(&row.request_checksum, &format!("{label} request checksum"))?
        || Digest::canonical(&result).map_err(corrupt)?
            != digest_from_db(&row.result_checksum, &format!("{label} result checksum"))?
    {
        return Err(corrupt(format!("{label} failed immutable provenance")));
    }
    Ok((request, result))
}

async fn lock_exact_tissue_claim(
    transaction: &mut Transaction<'_, Postgres>,
    job: &CancerTissueRefinementJob,
) -> Result<(), StoreError> {
    let held: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM cancer_tissue_refinement_jobs
            WHERE refinement_id=$1 AND claimed_by=$2 AND claim_token=$3
              AND lease_until > NOW() AND completed_at IS NULL
            FOR UPDATE
        )
        "#,
    )
    .bind(job.protocol.refinement_id)
    .bind(&job.worker_id)
    .bind(job.claim_token)
    .fetch_one(&mut **transaction)
    .await
    .map_err(operation_error)?;
    if held {
        Ok(())
    } else {
        Err(StoreError::Conflict(
            "tissue result does not hold the exact live claim".to_owned(),
        ))
    }
}

fn digest_from_db(bytes: &[u8], field: &str) -> Result<Digest, StoreError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
        StoreError::Corrupt(format!("{field} has {} bytes instead of 32", bytes.len()))
    })?;
    Ok(Digest::from_bytes(bytes))
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
    if let sqlx::Error::Database(database) = &error
        && matches!(
            database.code().as_deref(),
            Some("23503" | "23505" | "23514" | "40001" | "P0001")
        )
    {
        return StoreError::Conflict(database.message().to_owned());
    }
    StoreError::Unavailable(error.to_string())
}

#[cfg(test)]
mod tests {
    use application::{
        CANCER_RESEARCH_CAMPAIGN_MAX_TESTS, CANCER_RESEARCH_MODEL_CONTRACT_VERSION,
        CancerResearchCampaignDirective, CancerResearchEvidenceDocument,
        CancerResearchLadderResult, CancerResearchModelReceipt, CancerResearchModelRequest,
        CancerTissueRefinementJobStore, CancerTissueRefinementWorkerStep, CognitionRouteAttempt,
        CognitionRouteAttemptStatus, ModelTokenUsage, execute_cancer_virtual_experiment,
        process_next_cancer_tissue_refinement,
    };
    use world_domain::{
        CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION, CancerResearchArtifactKind,
        CancerResearchClaim, CancerResearchContribution, CancerResearchInferenceTier,
        CancerResearchProfile, CancerResearchTarget, CancerResearchTask,
        CancerResearchTurnSelection, CancerVirtualEndpoint, CancerVirtualExperimentPlan,
        CancerVirtualInterventionModality, CancerVirtualMechanismTarget, CancerVirtualSubjectModel,
        EntityId, SimTick, WorldSeed,
    };

    use super::*;

    struct DurableCampaignFixture {
        world_id: WorldId,
        synthesis_request_id: Uuid,
        root_artifact_hash: Digest,
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing at an isolated PostgreSQL database"]
    async fn durable_survivor_is_singleton_idempotent_and_never_enters_memory() {
        let store = test_store().await;
        let first = insert_survived_campaign(&store, 0x4401).await;
        let second = insert_survived_campaign(&store, 0x4402).await;

        let first_job = store
            .claim_next_cancer_tissue_refinement(first.world_id, "tissue-worker-one", 900)
            .await
            .expect("claim first")
            .expect("eligible first job");
        assert_eq!(
            first_job.candidate.survival_evidence.synthesis_request_id,
            first.synthesis_request_id
        );
        assert_eq!(
            first_job.candidate.root.artifact_hash,
            first.root_artifact_hash
        );
        assert!(
            store
                .claim_next_cancer_tissue_refinement(second.world_id, "tissue-worker-two", 900)
                .await
                .expect("second claim attempt")
                .is_none(),
            "singleton claim spans worlds"
        );

        let result = application::execute_cancer_tissue_refinement(
            &first_job.candidate,
            &first_job.protocol,
        )
        .expect("execute bounded tissue projection");
        store
            .complete_cancer_tissue_refinement(&first_job, &result)
            .await
            .expect("complete");
        store
            .complete_cancer_tissue_refinement(&first_job, &result)
            .await
            .expect("exact completion retry");
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cancer_tissue_refinement_results WHERE refinement_id=$1",
        )
        .bind(first_job.protocol.refinement_id)
        .fetch_one(store.pool())
        .await
        .expect("result count");
        assert_eq!(count, 1);
        let memory_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_outbox WHERE payload::TEXT LIKE '%tissue_refinement%'",
        )
        .fetch_one(store.pool())
        .await
        .expect("memory count");
        assert_eq!(memory_count, 0);

        let next = store
            .claim_next_cancer_tissue_refinement(second.world_id, "tissue-worker-two", 900)
            .await
            .expect("claim after release")
            .expect("second world now claimable");
        assert_eq!(
            next.candidate.survival_evidence.synthesis_request_id,
            second.synthesis_request_id
        );
        let next_result =
            application::execute_cancer_tissue_refinement(&next.candidate, &next.protocol)
                .expect("execute second bounded tissue projection");
        store
            .complete_cancer_tissue_refinement(&next, &next_result)
            .await
            .expect("release singleton after second completion");
        let stale = store
            .complete_cancer_tissue_refinement(&first_job, &result)
            .await;
        assert!(stale.is_ok(), "an already committed exact retry stays safe");

        let mutation = sqlx::query(
            "UPDATE cancer_tissue_refinement_results SET result_payload=result_payload WHERE refinement_id=$1",
        )
        .bind(first_job.protocol.refinement_id)
        .execute(store.pool())
        .await;
        assert!(mutation.is_err(), "tissue results are append-only");
        let protocol_mutation = sqlx::query(
            "UPDATE cancer_tissue_refinement_jobs SET protocol_payload=protocol_payload WHERE refinement_id=$1",
        )
        .bind(first_job.protocol.refinement_id)
        .execute(store.pool())
        .await;
        assert!(
            protocol_mutation.is_err(),
            "completed protocols are immutable"
        );
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing at an isolated PostgreSQL database"]
    async fn incomplete_and_falsified_campaign_rows_fail_closed() {
        let store = test_store().await;
        let incomplete = insert_campaign_with_outcome(
            &store,
            0x4410,
            CancerResearchCampaignOutcome::SurvivedReplicationRound,
            3,
            0,
            0,
            false,
        )
        .await;
        assert!(
            store
                .claim_next_cancer_tissue_refinement(
                    incomplete.world_id,
                    "tissue-worker-incomplete",
                    900,
                )
                .await
                .is_err(),
            "a synthesis cannot conceal an incomplete campaign row"
        );
        let incomplete_jobs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cancer_tissue_refinement_jobs WHERE world_id=$1",
        )
        .bind(incomplete.world_id.as_uuid())
        .fetch_one(store.pool())
        .await
        .expect("incomplete job count");
        assert_eq!(incomplete_jobs, 0);

        // Build a second fixture whose synthesis accurately declares an
        // inconclusive outcome. It is valid durable research, but never eligible.
        let non_survivor = insert_campaign_with_outcome(
            &store,
            0x4411,
            CancerResearchCampaignOutcome::Inconclusive,
            2,
            0,
            8,
            true,
        )
        .await;
        assert!(
            store
                .claim_next_cancer_tissue_refinement(
                    non_survivor.world_id,
                    "tissue-worker-ineligible",
                    900,
                )
                .await
                .expect("ineligible scan")
                .is_none()
        );
        let jobs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cancer_tissue_refinement_jobs WHERE world_id=$1",
        )
        .bind(non_survivor.world_id.as_uuid())
        .fetch_one(store.pool())
        .await
        .expect("job count");
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing at an isolated PostgreSQL database"]
    async fn more_than_one_admission_page_of_non_survivors_cannot_starve_a_later_survivor() {
        let store = test_store().await;
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x4412));
        insert_test_world(&store, world_id).await;
        let non_survivor_count =
            usize::try_from(MAX_ELIGIBLE_SYNTHESIS_SCAN).expect("positive scan limit") + 1;
        let inconclusive_tests = u8::try_from(CANCER_RESEARCH_CAMPAIGN_MAX_TESTS)
            .expect("campaign test ceiling fits the durable directive");
        let campaign_ordinal_width = u32::try_from(CANCER_RESEARCH_CAMPAIGN_MAX_TESTS + 2)
            .expect("bounded campaign ordinal width");
        let mut first_ordinal = 1_u32;
        for _ in 0..non_survivor_count {
            insert_campaign_in_world(
                &store,
                world_id,
                CampaignFixtureSpec {
                    first_ordinal,
                    outcome: CancerResearchCampaignOutcome::Inconclusive,
                    supporting_tests: 0,
                    falsifying_tests: 0,
                    inconclusive_tests,
                    complete_followups: true,
                    root_intensity_parts_per_million: 900_000,
                    supporting_tests_last: false,
                },
            )
            .await;
            first_ordinal = first_ordinal
                .checked_add(campaign_ordinal_width)
                .expect("fixture ordinal");
        }
        let survivor = insert_campaign_in_world(
            &store,
            world_id,
            CampaignFixtureSpec {
                first_ordinal,
                outcome: CancerResearchCampaignOutcome::SurvivedReplicationRound,
                supporting_tests: 3,
                falsifying_tests: 0,
                inconclusive_tests: 0,
                complete_followups: true,
                root_intensity_parts_per_million: 900_000,
                supporting_tests_last: false,
            },
        )
        .await;

        let claimed = store
            .claim_next_cancer_tissue_refinement(world_id, "tissue-worker-after-page", 900)
            .await
            .expect("admission scan")
            .expect("later survivor is not starved");
        assert_eq!(
            claimed.candidate.survival_evidence.synthesis_request_id,
            survivor.synthesis_request_id
        );
        assert_eq!(
            claimed.candidate.root.artifact_hash,
            survivor.root_artifact_hash
        );
        let job_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cancer_tissue_refinement_jobs WHERE world_id=$1",
        )
        .bind(world_id.as_uuid())
        .fetch_one(store.pool())
        .await
        .expect("job count");
        assert_eq!(job_count, 1, "only the actual survivor is admitted");
        let result =
            application::execute_cancer_tissue_refinement(&claimed.candidate, &claimed.protocol)
                .expect("execute survivor after bounded admission scan");
        store
            .complete_cancer_tissue_refinement(&claimed, &result)
            .await
            .expect("complete survivor after bounded admission scan");
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing at an isolated PostgreSQL database"]
    async fn inconclusive_root_with_three_late_supports_is_durably_admitted() {
        let store = test_store().await;
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x4413));
        insert_test_world(&store, world_id).await;
        let survivor = insert_campaign_in_world(
            &store,
            world_id,
            CampaignFixtureSpec {
                first_ordinal: 1,
                outcome: CancerResearchCampaignOutcome::SurvivedReplicationRound,
                supporting_tests: 3,
                falsifying_tests: 0,
                inconclusive_tests: 3,
                complete_followups: true,
                root_intensity_parts_per_million: 300_000,
                supporting_tests_last: true,
            },
        )
        .await;

        let claimed = store
            .claim_next_cancer_tissue_refinement(world_id, "tissue-worker-late-survivor", 900)
            .await
            .expect("admission scan")
            .expect("six-test survivor is admitted");
        assert_eq!(
            claimed.candidate.survival_evidence.synthesis_request_id,
            survivor.synthesis_request_id
        );
        assert_eq!(
            claimed.candidate.root_result.interpretation,
            world_domain::CancerVirtualExperimentInterpretation::ModelInconclusive
        );
        assert_eq!(claimed.candidate.campaign_experiments.len(), 6);
        assert_eq!(claimed.candidate.survival_evidence.supporting_tests, 3);
        assert_eq!(claimed.candidate.survival_evidence.inconclusive_tests, 3);
        assert_eq!(claimed.protocol.campaign_result_hashes.len(), 6);
        let result =
            application::execute_cancer_tissue_refinement(&claimed.candidate, &claimed.protocol)
                .expect("execute late-survivor refinement");
        store
            .complete_cancer_tissue_refinement(&claimed, &result)
            .await
            .expect("complete late-survivor refinement");
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing at an isolated PostgreSQL database"]
    async fn expired_claim_reclaims_and_stale_token_cannot_write() {
        let store = test_store().await;
        let fixture = insert_survived_campaign(&store, 0x4420).await;
        let stale = store
            .claim_next_cancer_tissue_refinement(fixture.world_id, "stale-worker", 30)
            .await
            .expect("first claim")
            .expect("job");
        sqlx::query(
            "UPDATE cancer_tissue_refinement_jobs SET claimed_at=NOW()-INTERVAL '2 minutes',lease_until=NOW()-INTERVAL '1 minute' WHERE refinement_id=$1",
        )
        .bind(stale.protocol.refinement_id)
        .execute(store.pool())
        .await
        .expect("expire operational lease");
        let reclaimed = store
            .claim_next_cancer_tissue_refinement(fixture.world_id, "replacement-worker", 900)
            .await
            .expect("reclaim")
            .expect("reclaimed job");
        assert_ne!(stale.claim_token, reclaimed.claim_token);
        let result = application::execute_cancer_tissue_refinement(
            &reclaimed.candidate,
            &reclaimed.protocol,
        )
        .expect("execute");
        assert!(
            store
                .complete_cancer_tissue_refinement(&stale, &result)
                .await
                .is_err(),
            "stale claim token cannot complete"
        );
        store
            .complete_cancer_tissue_refinement(&reclaimed, &result)
            .await
            .expect("current token completes");
        assert_eq!(
            process_next_cancer_tissue_refinement(
                &store,
                fixture.world_id,
                "replacement-worker",
                900,
            )
            .await
            .expect("worker idle"),
            CancerTissueRefinementWorkerStep::Idle
        );
    }

    async fn test_store() -> PostgresStore {
        let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        let store = PostgresStore::connect(&database_url, 8)
            .await
            .expect("connect isolated test database");
        store.migrate().await.expect("migrate test database");
        store
    }

    async fn insert_survived_campaign(store: &PostgresStore, seed: u128) -> DurableCampaignFixture {
        insert_campaign_with_outcome(
            store,
            seed,
            CancerResearchCampaignOutcome::SurvivedReplicationRound,
            3,
            0,
            0,
            true,
        )
        .await
    }

    #[derive(Clone, Copy)]
    struct CampaignFixtureSpec {
        first_ordinal: u32,
        outcome: CancerResearchCampaignOutcome,
        supporting_tests: u8,
        falsifying_tests: u8,
        inconclusive_tests: u8,
        complete_followups: bool,
        root_intensity_parts_per_million: u32,
        supporting_tests_last: bool,
    }

    async fn insert_campaign_with_outcome(
        store: &PostgresStore,
        seed: u128,
        outcome: CancerResearchCampaignOutcome,
        supporting_tests: u8,
        falsifying_tests: u8,
        inconclusive_tests: u8,
        complete_followups: bool,
    ) -> DurableCampaignFixture {
        let world_id = WorldId::from_uuid(Uuid::from_u128(seed));
        insert_test_world(store, world_id).await;
        insert_campaign_in_world(
            store,
            world_id,
            CampaignFixtureSpec {
                first_ordinal: 1,
                outcome,
                supporting_tests,
                falsifying_tests,
                inconclusive_tests,
                complete_followups,
                root_intensity_parts_per_million: 900_000,
                supporting_tests_last: false,
            },
        )
        .await
    }

    async fn insert_campaign_in_world(
        store: &PostgresStore,
        world_id: WorldId,
        spec: CampaignFixtureSpec,
    ) -> DurableCampaignFixture {
        let root_selection = selection(
            world_id,
            spec.first_ordinal,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::ProposeDiscriminatingExperiment,
            CancerResearchInferenceTier::Exploration,
            None,
            Vec::new(),
        );
        let root_plan = plan(168, spec.root_intensity_parts_per_million);
        let (root_request, root_result, root_contribution) = research_row(
            root_selection,
            Vec::new(),
            CancerResearchArtifactKind::ExperimentProposal,
            Some(root_plan),
        );
        insert_research_row(store, &root_request, &root_result, true).await;
        let root_candidate = CancerVirtualExperimentCandidate {
            world_id,
            request_id: root_request.request_id,
            ordinal: spec.first_ordinal,
            artifact_hash: root_contribution.canonical_hash().expect("root hash"),
            contribution: root_contribution,
        };
        let root_experiment = execute_cancer_virtual_experiment(&root_candidate).expect("root lab");
        insert_virtual_result(store, &root_experiment, &root_candidate.contribution).await;
        let root_hash = root_candidate.artifact_hash;
        let campaign_id = CancerResearchCampaignDirective::campaign_id(root_request.request_id);

        let test_count = usize::from(spec.supporting_tests)
            + usize::from(spec.falsifying_tests)
            + usize::from(spec.inconclusive_tests);
        for index in 0..test_count {
            let ordinal = spec
                .first_ordinal
                .checked_add(u32::try_from(index).expect("index"))
                .and_then(|ordinal| ordinal.checked_add(1))
                .expect("fixture ordinal");
            let followup_selection = selection(
                world_id,
                ordinal,
                CancerResearchStage::IndependentReplication,
                CancerResearchTask::DesignIndependentReplication,
                CancerResearchInferenceTier::Exploration,
                Some(root_hash),
                Vec::new(),
            );
            let expected_assessment = if spec.supporting_tests_last {
                if index < usize::from(spec.inconclusive_tests) {
                    application::CancerResearchCampaignTestAssessment::Inconclusive
                } else if index < usize::from(spec.inconclusive_tests + spec.falsifying_tests) {
                    application::CancerResearchCampaignTestAssessment::Falsifies
                } else {
                    application::CancerResearchCampaignTestAssessment::Supports
                }
            } else if index < usize::from(spec.supporting_tests) {
                application::CancerResearchCampaignTestAssessment::Supports
            } else if index < usize::from(spec.supporting_tests + spec.falsifying_tests) {
                application::CancerResearchCampaignTestAssessment::Falsifies
            } else {
                application::CancerResearchCampaignTestAssessment::Inconclusive
            };
            let intensity = match expected_assessment {
                application::CancerResearchCampaignTestAssessment::Supports => 900_000,
                application::CancerResearchCampaignTestAssessment::Falsifies => 50_000,
                application::CancerResearchCampaignTestAssessment::Inconclusive => 300_000,
            };
            let (request, result, contribution) = research_row(
                followup_selection,
                Vec::new(),
                CancerResearchArtifactKind::ExperimentProposal,
                Some(plan(
                    96 + u16::try_from(index).expect("index") * 12,
                    intensity,
                )),
            );
            insert_research_row(
                store,
                &request,
                &result,
                spec.complete_followups || index > 0,
            )
            .await;
            let candidate = CancerVirtualExperimentCandidate {
                world_id,
                request_id: request.request_id,
                ordinal,
                artifact_hash: contribution.canonical_hash().expect("followup hash"),
                contribution,
            };
            let experiment = execute_cancer_virtual_experiment(&candidate).expect("followup lab");
            assert_eq!(
                application::cancer_research_campaign_test_assessment(&experiment),
                expected_assessment,
                "fixture test {index} produced the wrong campaign assessment"
            );
            insert_virtual_result(store, &experiment, &candidate.contribution).await;
        }

        let synthesis_ordinal = spec
            .first_ordinal
            .checked_add(u32::try_from(test_count).expect("count"))
            .and_then(|ordinal| ordinal.checked_add(1))
            .expect("fixture ordinal");
        let directive = CancerResearchCampaignDirective::Synthesis {
            schema_version: application::CANCER_RESEARCH_CAMPAIGN_DIRECTIVE_SCHEMA_VERSION,
            campaign_id,
            root_artifact_hash: root_hash,
            outcome: spec.outcome,
            supporting_tests: spec.supporting_tests,
            falsifying_tests: spec.falsifying_tests,
            inconclusive_tests: spec.inconclusive_tests,
        };
        let document = directive
            .evidence_document(world_id)
            .expect("directive evidence");
        let synthesis_selection = selection(
            world_id,
            synthesis_ordinal,
            CancerResearchStage::IndependentReplication,
            CancerResearchTask::InterpretReplicationResult,
            CancerResearchInferenceTier::Escalation,
            Some(root_hash),
            vec![document.reference.clone()],
        );
        let (synthesis_request, synthesis_result, _) = research_row(
            synthesis_selection,
            vec![document],
            CancerResearchArtifactKind::Paper,
            None,
        );
        insert_research_row(store, &synthesis_request, &synthesis_result, true).await;
        DurableCampaignFixture {
            world_id,
            synthesis_request_id: synthesis_request.request_id,
            root_artifact_hash: root_hash,
        }
    }

    fn selection(
        world_id: WorldId,
        ordinal: u32,
        stage: CancerResearchStage,
        task: CancerResearchTask,
        tier: CancerResearchInferenceTier,
        frozen: Option<Digest>,
        evidence: Vec<world_domain::CancerResearchEvidenceReference>,
    ) -> CancerResearchTurnSelection {
        let resident_id = EntityId::deterministic(world_id, &ordinal.to_be_bytes());
        CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(u64::from(ordinal) * 2 + 1),
            SimTick::new(u64::from(ordinal) * 2 + 2),
            ordinal,
            CancerResearchTarget::AdultGlioblastoma,
            stage,
            task,
            tier,
            CancerResearchProfile::seeded(WorldSeed::new(44), resident_id).expect("profile"),
            evidence,
            frozen,
            512,
        )
        .expect("selection")
    }

    fn plan(exposure_hours: u16, intensity: u32) -> CancerVirtualExperimentPlan {
        CancerVirtualExperimentPlan {
            schema_version: CANCER_VIRTUAL_EXPERIMENT_PLAN_SCHEMA_VERSION,
            subject_model: CancerVirtualSubjectModel::TumorOrganoid,
            intervention_modality: CancerVirtualInterventionModality::MolecularInhibition,
            primary_target: CancerVirtualMechanismTarget::CellDivision,
            secondary_target: None,
            primary_endpoint: CancerVirtualEndpoint::ViableTumorFraction,
            intensity_parts_per_million: intensity,
            exposure_hours,
            cohort_size: 128,
        }
    }

    fn research_row(
        selection: CancerResearchTurnSelection,
        evidence_documents: Vec<CancerResearchEvidenceDocument>,
        artifact_kind: CancerResearchArtifactKind,
        plan: Option<CancerVirtualExperimentPlan>,
    ) -> (
        CancerResearchModelRequest,
        CancerResearchLadderResult,
        CancerResearchContribution,
    ) {
        let request = CancerResearchModelRequest::new(selection, evidence_documents, Vec::new())
            .expect("request");
        let contribution = CancerResearchContribution::new_with_virtual_experiment(
            &request.selection,
            artifact_kind,
            "A preregistered durable campaign fixture",
            "A deterministic fixture used only to verify durable observer-side tissue execution.",
            vec![CancerResearchClaim {
                statement: "The bounded intervention changes the modeled endpoint.".to_owned(),
                testable_prediction: "The intervention projection differs from control.".to_owned(),
                falsification_test: "The bounded interval fails the preregistered direction."
                    .to_owned(),
                citation_hashes: Vec::new(),
            }],
            plan,
        )
        .expect("contribution");
        let registry = CognitionRouteRegistry::cancer_research_for_policy(
            request.route_purpose(),
            if request.selection.inference_tier == CancerResearchInferenceTier::Exploration {
                CognitionRouteRegistry::cancer_research_exploration().policy_version
            } else {
                CognitionRouteRegistry::cancer_research_escalation().policy_version
            },
        )
        .expect("registry");
        let route = &registry.routes[0];
        let attempt = CognitionRouteAttempt {
            route_index: 0,
            provider: route.provider.clone(),
            requested_model: route.requested_model.clone(),
            billing_class: route.billing_class,
            status: CognitionRouteAttemptStatus::Succeeded,
        };
        let receipt = CancerResearchModelReceipt {
            contract_version: CANCER_RESEARCH_MODEL_CONTRACT_VERSION,
            request_id: request.request_id,
            request_hash: request.canonical_hash().expect("request hash"),
            provider: route.provider.clone(),
            requested_model: route.requested_model.clone(),
            resolved_model: route.requested_model.clone(),
            provider_response_id: format!("fixture-{}", request.request_id),
            usage: ModelTokenUsage {
                prompt_tokens: 100,
                completion_tokens: 100,
            },
            billed_micro_usd: 0,
            contribution: contribution.clone(),
            provider_response_hash: Digest::sha256(request.request_id.as_bytes()),
            adapter_version: "durable-tissue-fixture-v1".to_owned(),
        };
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
        result
            .validate_against(&registry, &request)
            .expect("result");
        (request, result, contribution)
    }

    async fn insert_test_world(store: &PostgresStore, world_id: WorldId) {
        let zero = vec![0_u8; 32];
        sqlx::query(
            r#"
            INSERT INTO worlds (
                id,seed,status,ruleset_version,manifest,manifest_checksum,
                last_event_checksum,current_state_checksum
            ) VALUES ($1,$2,'retired',39,'{}'::JSONB,$3,$3,$3)
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(world_id.as_uuid().as_u128().to_string())
        .bind(zero)
        .execute(store.pool())
        .await
        .expect("world");
    }

    async fn insert_research_row(
        store: &PostgresStore,
        request: &CancerResearchModelRequest,
        result: &CancerResearchLadderResult,
        completed: bool,
    ) {
        sqlx::query(
            r#"
            INSERT INTO cancer_research_requests (
                request_id,world_id,resident_id,selected_tick,deadline_tick,ordinal,
                stage,inference_tier,request_payload,request_checksum,completed_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,
                CASE WHEN $11 THEN NOW() ELSE NULL END)
            "#,
        )
        .bind(request.request_id)
        .bind(request.selection.world_id.as_uuid())
        .bind(request.selection.resident_id.as_uuid())
        .bind(i64::try_from(request.selection.selected_at_tick.get()).expect("tick"))
        .bind(i64::try_from(request.selection.deadline_tick.get()).expect("tick"))
        .bind(i64::from(request.selection.ordinal))
        .bind(match request.selection.stage {
            CancerResearchStage::BlindDiscovery => "blind_discovery",
            CancerResearchStage::LiteratureAudit => "literature_audit",
            CancerResearchStage::IndependentReplication => "independent_replication",
        })
        .bind(match request.selection.inference_tier {
            CancerResearchInferenceTier::Exploration => "exploration",
            CancerResearchInferenceTier::Escalation => "escalation",
        })
        .bind(serde_json::to_value(request).expect("request JSON"))
        .bind(
            request
                .canonical_hash()
                .expect("request checksum")
                .as_bytes()
                .as_slice(),
        )
        .bind(completed)
        .execute(store.pool())
        .await
        .expect("request insert");
        let registry = CognitionRouteRegistry::cancer_research_for_policy(
            request.route_purpose(),
            result.route_policy_version,
        )
        .expect("registry");
        result.validate_against(&registry, request).expect("result");
        sqlx::query(
            r#"
            INSERT INTO cancer_research_results (
                request_id,route_policy_version,route_registry_checksum,
                result_payload,result_checksum
            ) VALUES ($1,$2,$3,$4,$5)
            "#,
        )
        .bind(request.request_id)
        .bind(i32::from(result.route_policy_version))
        .bind(result.route_registry_hash.as_bytes().as_slice())
        .bind(serde_json::to_value(result).expect("result JSON"))
        .bind(
            Digest::canonical(result)
                .expect("result checksum")
                .as_bytes()
                .as_slice(),
        )
        .execute(store.pool())
        .await
        .expect("result insert");
    }

    async fn insert_virtual_result(
        store: &PostgresStore,
        result: &CancerVirtualExperimentResult,
        contribution: &CancerResearchContribution,
    ) {
        result
            .validate_against(contribution)
            .expect("virtual result");
        sqlx::query(
            r#"
            INSERT INTO cancer_virtual_experiment_results (
                experiment_id,world_id,request_id,method_version,artifact_hash,
                plan_hash,result_payload,result_checksum
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            "#,
        )
        .bind(result.experiment_id)
        .bind(result.world_id.as_uuid())
        .bind(result.request_id)
        .bind(i32::from(result.method_version))
        .bind(result.artifact_hash.as_bytes().as_slice())
        .bind(result.plan_hash.as_bytes().as_slice())
        .bind(serde_json::to_value(result).expect("experiment JSON"))
        .bind(
            result
                .canonical_hash(contribution)
                .expect("experiment hash")
                .as_bytes()
                .as_slice(),
        )
        .execute(store.pool())
        .await
        .expect("experiment insert");
    }
}
