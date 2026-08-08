use application::{StoreError, StoredWorld, TransitionEffects, WorldCursor, WorldStore};
use async_trait::async_trait;
use serde_json::{Value, json};
use sim_engine::{EngineState, Snapshot};
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;
use world_domain::{
    CognitionDeadlineInput, CognitionInputOutcome, CognitionUnavailableReason, Digest, DomainEvent,
    EventBatch, EventSequence, SimTick, WorldId, WorldManifest, WorldStatus,
};

use crate::PostgresStore;

/// Persist one replay cache checkpoint per this many committed batches. Genesis and
/// terminal transitions are always retained independently of this cadence.
const SNAPSHOT_SEQUENCE_INTERVAL: u64 = 64;

#[derive(FromRow)]
struct WorldRow {
    id: Uuid,
    seed: String,
    status: String,
    ruleset_version: i32,
    current_tick: i64,
    current_sequence: i64,
    predecessor_world_id: Option<Uuid>,
    manifest: Value,
    manifest_checksum: Vec<u8>,
    last_event_checksum: Vec<u8>,
    current_state_checksum: Vec<u8>,
}

#[derive(FromRow)]
struct EventBatchRow {
    world_id: Uuid,
    sequence: i64,
    tick: i64,
    event_schema_version: i32,
    ruleset_version: i32,
    payload: Value,
    checksum: Vec<u8>,
    previous_checksum: Vec<u8>,
    post_state_checksum: Vec<u8>,
}

#[derive(FromRow)]
struct SnapshotRow {
    world_id: Uuid,
    through_sequence: i64,
    tick: i64,
    snapshot_schema_version: i32,
    ruleset_version: i32,
    state: Value,
    checksum: Vec<u8>,
    last_event_checksum: Vec<u8>,
}

#[derive(FromRow)]
struct CognitionLatchRow {
    world_id: Uuid,
    deadline_tick: i64,
    target_sequence: i64,
    latch_payload: Value,
    latch_checksum: Vec<u8>,
}

#[async_trait]
impl WorldStore for PostgresStore {
    async fn create_world(
        &self,
        manifest: &WorldManifest,
        predecessor_world_id: Option<WorldId>,
    ) -> Result<StoredWorld, StoreError> {
        if manifest.ruleset_version == 0 {
            return Err(StoreError::Conflict(
                "ruleset version must be greater than zero".to_owned(),
            ));
        }

        let initial_state = EngineState::new(manifest.clone());
        let snapshot =
            Snapshot::new(initial_state, EventSequence::ZERO, Digest::ZERO).map_err(corrupt)?;
        let manifest_hash = Digest::canonical(manifest).map_err(corrupt)?;
        let manifest_json = serde_json::to_value(manifest).map_err(corrupt)?;
        let snapshot_json = serde_json::to_value(&snapshot).map_err(corrupt)?;
        let ruleset_version = i32::try_from(manifest.ruleset_version).map_err(|_| {
            StoreError::Conflict("ruleset version exceeds PostgreSQL integer range".to_owned())
        })?;

        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        sqlx::query(
            r#"
            INSERT INTO worlds (
                id,
                seed,
                status,
                ruleset_version,
                current_tick,
                current_sequence,
                predecessor_world_id,
                manifest,
                manifest_checksum,
                last_event_checksum,
                current_state_checksum
            )
            VALUES ($1, $2, 'initializing', $3, 0, 0, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(manifest.world_id.as_uuid())
        .bind(manifest.seed.to_string())
        .bind(ruleset_version)
        .bind(predecessor_world_id.map(WorldId::as_uuid))
        .bind(manifest_json)
        .bind(manifest_hash.as_bytes().as_slice())
        .bind(Digest::ZERO.as_bytes().as_slice())
        .bind(snapshot.state_hash.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;

        insert_snapshot(&mut transaction, &snapshot, ruleset_version, snapshot_json).await?;
        transaction.commit().await.map_err(operation_error)?;

        Ok(StoredWorld {
            manifest: manifest.clone(),
            status: WorldStatus::Initializing,
            cursor: WorldCursor {
                sequence: EventSequence::ZERO,
                tick: SimTick::ZERO,
                last_event_hash: Digest::ZERO,
                state_hash: snapshot.state_hash,
            },
            predecessor_world_id,
        })
    }

    async fn load_world(&self, world_id: WorldId) -> Result<StoredWorld, StoreError> {
        let row = fetch_world(self.pool(), world_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("world {world_id}")))?;
        parse_world(row)
    }

    async fn list_running_world_ids(&self) -> Result<Vec<WorldId>, StoreError> {
        let ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM worlds
            WHERE status = 'running'
            ORDER BY id ASC
            "#,
        )
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        Ok(ids.into_iter().map(WorldId::from_uuid).collect())
    }

    async fn list_world_ids(&self) -> Result<Vec<WorldId>, StoreError> {
        let ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM worlds
            ORDER BY id ASC
            "#,
        )
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;
        Ok(ids.into_iter().map(WorldId::from_uuid).collect())
    }

    async fn load_event_batches(
        &self,
        world_id: WorldId,
        after_sequence: EventSequence,
    ) -> Result<Vec<EventBatch>, StoreError> {
        let after_sequence = to_i64(after_sequence.get(), "event sequence")?;
        let rows = sqlx::query_as::<_, EventBatchRow>(
            r#"
            SELECT
                world_id,
                sequence,
                tick,
                event_schema_version,
                ruleset_version,
                payload,
                checksum,
                previous_checksum,
                post_state_checksum
            FROM event_batches
            WHERE world_id = $1 AND sequence > $2
            ORDER BY sequence ASC
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(after_sequence)
        .fetch_all(self.pool())
        .await
        .map_err(operation_error)?;

        rows.into_iter().map(parse_event_batch).collect()
    }

    async fn load_latest_snapshot(&self, world_id: WorldId) -> Result<Snapshot, StoreError> {
        let row = sqlx::query_as::<_, SnapshotRow>(
            r#"
            SELECT
                world_id,
                through_sequence,
                tick,
                snapshot_schema_version,
                ruleset_version,
                state,
                checksum,
                last_event_checksum
            FROM snapshots
            WHERE world_id = $1
            ORDER BY through_sequence DESC
            LIMIT 1
            "#,
        )
        .bind(world_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?
        .ok_or_else(|| StoreError::NotFound(format!("snapshot for world {world_id}")))?;

        parse_snapshot(row)
    }

    async fn commit_transition(
        &self,
        expected: WorldCursor,
        batch: &EventBatch,
        snapshot: &Snapshot,
        effects: &TransitionEffects,
    ) -> Result<StoredWorld, StoreError> {
        batch.verify_integrity().map_err(corrupt)?;
        snapshot.verify_integrity().map_err(corrupt)?;
        validate_transition(expected, batch, snapshot)?;
        effects
            .validate_for(batch.world_id, batch.sequence, batch.tick)
            .map_err(corrupt)?;
        for memory in &effects.memory_retains {
            if !snapshot
                .state
                .organisms()
                .any(|organism| organism.organism_id() == memory.agent_id)
            {
                return Err(StoreError::Conflict(format!(
                    "memory operation {} refers to an unknown agent",
                    memory.operation_id
                )));
            }
        }

        let sequence = to_i64(batch.sequence.get(), "event sequence")?;
        let tick = to_i64(batch.tick.get(), "simulation tick")?;
        let event_schema_version = i32::from(batch.event_schema_version);
        let ruleset_version = i32::try_from(batch.ruleset_version).map_err(|_| {
            StoreError::Conflict("ruleset version exceeds PostgreSQL integer range".to_owned())
        })?;
        let batch_json = serde_json::to_value(batch).map_err(corrupt)?;
        let persist_snapshot = sequence == 1
            || batch
                .sequence
                .get()
                .is_multiple_of(SNAPSHOT_SEQUENCE_INTERVAL)
            || snapshot.state.status() != WorldStatus::Running;
        let snapshot_json = persist_snapshot
            .then(|| serde_json::to_value(snapshot).map_err(corrupt))
            .transpose()?;

        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        let persisted = fetch_world_for_update(&mut transaction, batch.world_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("world {}", batch.world_id)))?;
        let persisted = parse_world(persisted)?;
        if persisted.cursor != expected {
            return Err(StoreError::Conflict(format!(
                "world {} cursor changed before sequence {}",
                batch.world_id, batch.sequence
            )));
        }
        if persisted.status == WorldStatus::Archived {
            return Err(StoreError::Conflict(format!(
                "world {} is already archived",
                batch.world_id
            )));
        }
        if persisted.manifest.world_id != batch.world_id
            || persisted.manifest.ruleset_version != batch.ruleset_version
        {
            return Err(StoreError::Conflict(
                "batch does not match the persisted world manifest".to_owned(),
            ));
        }

        sqlx::query(
            r#"
            INSERT INTO event_batches (
                world_id,
                sequence,
                tick,
                event_schema_version,
                ruleset_version,
                payload,
                checksum,
                previous_checksum,
                post_state_checksum
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(batch.world_id.as_uuid())
        .bind(sequence)
        .bind(tick)
        .bind(event_schema_version)
        .bind(ruleset_version)
        .bind(batch_json)
        .bind(batch.batch_hash.as_bytes().as_slice())
        .bind(batch.previous_hash.as_bytes().as_slice())
        .bind(batch.post_state_hash.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;

        // Cognition selection is a causal event, so its worker request is created in
        // this same transaction. No projection or runner-side effect can diverge from
        // the canonical batch.
        for record in &batch.events {
            let DomainEvent::CognitionRequestSelected { selection } = &record.event else {
                continue;
            };
            selection.validate().map_err(corrupt)?;
            if selection.world_id != batch.world_id || selection.selected_at_tick != batch.tick {
                return Err(StoreError::Conflict(
                    "cognition selection does not match its source batch".to_owned(),
                ));
            }
            let selection_json = serde_json::to_value(selection).map_err(corrupt)?;
            let selection_checksum = selection.canonical_hash().map_err(corrupt)?;
            sqlx::query(
                r#"
                INSERT INTO cognition_requests (
                    request_id,
                    world_id,
                    agent_id,
                    source_sequence,
                    source_event_id,
                    source_event_index,
                    selected_tick,
                    deadline_tick,
                    ordinal,
                    selection_schema_version,
                    selection,
                    selection_checksum
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                "#,
            )
            .bind(selection.request_id)
            .bind(selection.world_id.as_uuid())
            .bind(selection.organism_id.as_uuid())
            .bind(sequence)
            .bind(record.event_id.as_uuid())
            .bind(i64::from(record.index))
            .bind(to_i64(
                selection.selected_at_tick.get(),
                "cognition selected tick",
            )?)
            .bind(to_i64(
                selection.deadline_tick.get(),
                "cognition deadline tick",
            )?)
            .bind(i64::from(selection.ordinal))
            .bind(i32::from(selection.schema_version))
            .bind(selection_json)
            .bind(selection_checksum.as_bytes().as_slice())
            .execute(&mut *transaction)
            .await
            .map_err(operation_error)?;
        }

        consume_cognition_latches(&mut transaction, batch).await?;

        if let Some(snapshot_json) = snapshot_json {
            insert_snapshot(&mut transaction, snapshot, ruleset_version, snapshot_json).await?;
        }

        let contains_started = batch
            .events
            .iter()
            .any(|record| matches!(record.event, DomainEvent::WorldStarted { .. }));
        let contains_extinct = batch
            .events
            .iter()
            .any(|record| matches!(record.event, DomainEvent::WorldExtinct));
        let contains_archived = batch
            .events
            .iter()
            .any(|record| matches!(record.event, DomainEvent::WorldArchived));
        let status = status_text(snapshot.state.status());

        let updated = sqlx::query(
            r#"
            UPDATE worlds
            SET
                status = $2,
                current_tick = $3,
                current_sequence = $4,
                last_event_checksum = $5,
                current_state_checksum = $6,
                started_at = CASE WHEN $7 THEN COALESCE(started_at, NOW()) ELSE started_at END,
                extinct_at = CASE WHEN $8 THEN COALESCE(extinct_at, NOW()) ELSE extinct_at END,
                archived_at = CASE WHEN $9 THEN COALESCE(archived_at, NOW()) ELSE archived_at END
            WHERE id = $1
              AND current_sequence = $10
              AND last_event_checksum = $11
              AND status <> 'archived'
            "#,
        )
        .bind(batch.world_id.as_uuid())
        .bind(status)
        .bind(tick)
        .bind(sequence)
        .bind(batch.batch_hash.as_bytes().as_slice())
        .bind(snapshot.state_hash.as_bytes().as_slice())
        .bind(contains_started)
        .bind(contains_extinct)
        .bind(contains_archived)
        .bind(to_i64(expected.sequence.get(), "expected event sequence")?)
        .bind(expected.last_event_hash.as_bytes().as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::Conflict(format!(
                "world {} cursor update lost a race",
                batch.world_id
            )));
        }

        let outbox_id = deterministic_outbox_id(batch.world_id, batch.sequence);
        sqlx::query(
            r#"
            INSERT INTO outbox (id, world_id, source_sequence, topic, payload)
            VALUES ($1, $2, $3, 'canonical.transition.committed', $4)
            "#,
        )
        .bind(outbox_id)
        .bind(batch.world_id.as_uuid())
        .bind(sequence)
        .bind(json!({
            "world_id": batch.world_id,
            "sequence": batch.sequence,
            "tick": batch.tick,
            "batch_hash": batch.batch_hash,
            "state_hash": snapshot.state_hash,
            "status": snapshot.state.status(),
        }))
        .execute(&mut *transaction)
        .await
        .map_err(operation_error)?;

        for memory in &effects.memory_retains {
            let payload = serde_json::to_value(memory).map_err(corrupt)?;
            sqlx::query(
                r#"
                INSERT INTO memory_outbox (
                    operation_id,
                    document_id,
                    world_id,
                    agent_id,
                    source_sequence,
                    bank_id,
                    payload_version,
                    payload
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(memory.operation_id)
            .bind(memory.document_id)
            .bind(memory.world_id.as_uuid())
            .bind(memory.agent_id.as_uuid())
            .bind(to_i64(
                memory.source_sequence.get(),
                "memory source sequence",
            )?)
            .bind(&memory.bank_id)
            .bind(i32::from(memory.payload_version))
            .bind(payload)
            .execute(&mut *transaction)
            .await
            .map_err(operation_error)?;
        }

        transaction.commit().await.map_err(operation_error)?;

        Ok(StoredWorld {
            manifest: persisted.manifest,
            status: snapshot.state.status(),
            cursor: WorldCursor {
                sequence: batch.sequence,
                tick: batch.tick,
                last_event_hash: batch.batch_hash,
                state_hash: snapshot.state_hash,
            },
            predecessor_world_id: persisted.predecessor_world_id,
        })
    }
}

async fn consume_cognition_latches(
    transaction: &mut Transaction<'_, Postgres>,
    batch: &EventBatch,
) -> Result<(), StoreError> {
    let target_sequence = to_i64(batch.sequence.get(), "cognition target sequence")?;
    let batch_tick = to_i64(batch.tick.get(), "cognition input tick")?;
    let mut seen = Vec::new();
    for (position, record) in batch.events.iter().enumerate() {
        let DomainEvent::CognitionInputRecorded { input } = &record.event else {
            continue;
        };
        input.validate().map_err(corrupt)?;
        if input.world_id != batch.world_id
            || !seen
                .iter()
                .all(|request_id| request_id != &input.request_id)
        {
            return Err(StoreError::Conflict(
                "cognition input world or request uniqueness is invalid".to_owned(),
            ));
        }
        seen.push(input.request_id);
        let deadline_tick = to_i64(input.deadline_tick.get(), "cognition deadline tick")?;
        if batch_tick > deadline_tick {
            return Err(StoreError::Conflict(
                "late cognition input cannot enter canonical history".to_owned(),
            ));
        }
        let payload = serde_json::to_value(input).map_err(corrupt)?;
        let checksum = input.canonical_hash().map_err(corrupt)?;
        if batch_tick < deadline_tick {
            validate_early_cognition_resolution(batch, position, input)?;
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
                VALUES ($1, $2, $3, $4, 'unavailable', $5, $6)
                "#,
            )
            .bind(input.request_id)
            .bind(batch.world_id.as_uuid())
            .bind(deadline_tick)
            .bind(target_sequence)
            .bind(payload)
            .bind(checksum.as_bytes().as_slice())
            .execute(&mut **transaction)
            .await
            .map_err(operation_error)?;
            insert_cognition_consumption(transaction, batch, record, checksum).await?;
            continue;
        }

        let latch = sqlx::query_as::<_, CognitionLatchRow>(
            r#"
            SELECT world_id, deadline_tick, target_sequence, latch_payload, latch_checksum
            FROM cognition_deadline_latches
            WHERE request_id = $1
            FOR UPDATE
            "#,
        )
        .bind(input.request_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(operation_error)?
        .ok_or_else(|| {
            StoreError::Conflict(format!(
                "cognition input {} has no durable deadline latch",
                input.request_id
            ))
        })?;
        if latch.world_id != batch.world_id.as_uuid()
            || latch.deadline_tick != deadline_tick
            || latch.target_sequence != target_sequence
            || latch.latch_payload != payload
            || digest_from_db(&latch.latch_checksum, "cognition latch checksum")? != checksum
        {
            return Err(StoreError::Conflict(
                "canonical cognition input differs from its immutable deadline latch".to_owned(),
            ));
        }
        insert_cognition_consumption(transaction, batch, record, checksum).await?;
    }

    let unconsumed: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM cognition_deadline_latches AS latch
        LEFT JOIN cognition_latch_consumptions AS consumption
            ON consumption.request_id = latch.request_id
        WHERE latch.world_id = $1
          AND latch.target_sequence = $2
          AND consumption.request_id IS NULL
        "#,
    )
    .bind(batch.world_id.as_uuid())
    .bind(target_sequence)
    .fetch_one(&mut **transaction)
    .await
    .map_err(operation_error)?;
    if unconsumed != 0 {
        return Err(StoreError::Conflict(
            "transition omitted one or more immutable cognition deadline latches".to_owned(),
        ));
    }
    Ok(())
}

async fn insert_cognition_consumption(
    transaction: &mut Transaction<'_, Postgres>,
    batch: &EventBatch,
    record: &world_domain::EventRecord,
    checksum: Digest,
) -> Result<(), StoreError> {
    let DomainEvent::CognitionInputRecorded { input } = &record.event else {
        return Err(StoreError::Corrupt(
            "cognition consumption was requested for another event kind".to_owned(),
        ));
    };
    sqlx::query(
        r#"
        INSERT INTO cognition_latch_consumptions (
            request_id,
            world_id,
            source_sequence,
            source_event_id,
            source_event_index,
            latch_checksum
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(input.request_id)
    .bind(batch.world_id.as_uuid())
    .bind(to_i64(batch.sequence.get(), "cognition source sequence")?)
    .bind(record.event_id.as_uuid())
    .bind(i64::from(record.index))
    .bind(checksum.as_bytes().as_slice())
    .execute(&mut **transaction)
    .await
    .map_err(operation_error)?;
    Ok(())
}

fn validate_early_cognition_resolution(
    batch: &EventBatch,
    position: usize,
    input: &CognitionDeadlineInput,
) -> Result<(), StoreError> {
    if input.recall_outcome_hash != Digest::ZERO
        || input.route_registry_hash != Digest::ZERO
        || input.result_hash != Digest::ZERO
    {
        return Err(StoreError::Conflict(
            "early cognition resolution cannot contain external evidence".to_owned(),
        ));
    }
    let mechanically_supported = match input.outcome {
        CognitionInputOutcome::Unavailable {
            reason: CognitionUnavailableReason::SubjectUnavailable,
        } => batch.events[..position].iter().any(|record| {
            matches!(
                record.event,
                DomainEvent::OrganismDied { organism_id, .. }
                    if organism_id == input.organism_id
            )
        }),
        CognitionInputOutcome::Unavailable {
            reason: CognitionUnavailableReason::WorldArchived,
        } => batch.events[position + 1..]
            .iter()
            .any(|record| matches!(record.event, DomainEvent::WorldArchived)),
        _ => false,
    };
    if mechanically_supported {
        Ok(())
    } else {
        Err(StoreError::Conflict(
            "early cognition resolution lacks its mechanical lifecycle event".to_owned(),
        ))
    }
}

async fn fetch_world(
    pool: &sqlx::PgPool,
    world_id: WorldId,
) -> Result<Option<WorldRow>, StoreError> {
    sqlx::query_as::<_, WorldRow>(world_select(false))
        .bind(world_id.as_uuid())
        .fetch_optional(pool)
        .await
        .map_err(operation_error)
}

async fn fetch_world_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    world_id: WorldId,
) -> Result<Option<WorldRow>, StoreError> {
    sqlx::query_as::<_, WorldRow>(world_select(true))
        .bind(world_id.as_uuid())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(operation_error)
}

fn world_select(for_update: bool) -> &'static str {
    if for_update {
        r#"
        SELECT
            id,
            seed,
            status,
            ruleset_version,
            current_tick,
            current_sequence,
            predecessor_world_id,
            manifest,
            manifest_checksum,
            last_event_checksum,
            current_state_checksum
        FROM worlds
        WHERE id = $1
        FOR UPDATE
        "#
    } else {
        r#"
        SELECT
            id,
            seed,
            status,
            ruleset_version,
            current_tick,
            current_sequence,
            predecessor_world_id,
            manifest,
            manifest_checksum,
            last_event_checksum,
            current_state_checksum
        FROM worlds
        WHERE id = $1
        "#
    }
}

async fn insert_snapshot(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &Snapshot,
    ruleset_version: i32,
    snapshot_json: Value,
) -> Result<(), StoreError> {
    sqlx::query(
        r#"
        INSERT INTO snapshots (
            world_id,
            through_sequence,
            tick,
            snapshot_schema_version,
            ruleset_version,
            state,
            checksum,
            last_event_checksum
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(snapshot.world_id.as_uuid())
    .bind(to_i64(
        snapshot.through_sequence.get(),
        "snapshot sequence",
    )?)
    .bind(to_i64(snapshot.state.tick().get(), "snapshot tick")?)
    .bind(i32::from(snapshot.snapshot_schema_version))
    .bind(ruleset_version)
    .bind(snapshot_json)
    .bind(snapshot.state_hash.as_bytes().as_slice())
    .bind(snapshot.last_event_hash.as_bytes().as_slice())
    .execute(&mut **transaction)
    .await
    .map_err(operation_error)?;
    Ok(())
}

fn parse_world(row: WorldRow) -> Result<StoredWorld, StoreError> {
    let manifest: WorldManifest = serde_json::from_value(row.manifest).map_err(corrupt)?;
    let world_id = WorldId::from_uuid(row.id);
    if manifest.world_id != world_id
        || manifest.seed.to_string() != row.seed
        || i32::try_from(manifest.ruleset_version).ok() != Some(row.ruleset_version)
    {
        return Err(StoreError::Corrupt(format!(
            "world {world_id} columns disagree with its manifest"
        )));
    }
    let manifest_hash = Digest::canonical(&manifest).map_err(corrupt)?;
    let stored_manifest_hash = digest_from_db(&row.manifest_checksum, "manifest checksum")?;
    if manifest_hash != stored_manifest_hash {
        return Err(StoreError::Corrupt(format!(
            "world {world_id} manifest checksum mismatch"
        )));
    }

    Ok(StoredWorld {
        manifest,
        status: parse_status(&row.status)?,
        cursor: WorldCursor {
            sequence: EventSequence::new(from_i64(row.current_sequence, "event sequence")?),
            tick: SimTick::new(from_i64(row.current_tick, "simulation tick")?),
            last_event_hash: digest_from_db(&row.last_event_checksum, "last event checksum")?,
            state_hash: digest_from_db(&row.current_state_checksum, "state checksum")?,
        },
        predecessor_world_id: row.predecessor_world_id.map(WorldId::from_uuid),
    })
}

fn parse_event_batch(row: EventBatchRow) -> Result<EventBatch, StoreError> {
    let batch: EventBatch = serde_json::from_value(row.payload).map_err(corrupt)?;
    batch.verify_integrity().map_err(corrupt)?;
    if batch.world_id.as_uuid() != row.world_id
        || to_i64(batch.sequence.get(), "event sequence")? != row.sequence
        || to_i64(batch.tick.get(), "simulation tick")? != row.tick
        || i32::from(batch.event_schema_version) != row.event_schema_version
        || i32::try_from(batch.ruleset_version).ok() != Some(row.ruleset_version)
        || digest_from_db(&row.checksum, "batch checksum")? != batch.batch_hash
        || digest_from_db(&row.previous_checksum, "previous checksum")? != batch.previous_hash
        || digest_from_db(&row.post_state_checksum, "post-state checksum")? != batch.post_state_hash
    {
        return Err(StoreError::Corrupt(format!(
            "event batch {} indexed columns disagree with its payload",
            batch.sequence
        )));
    }
    Ok(batch)
}

fn parse_snapshot(row: SnapshotRow) -> Result<Snapshot, StoreError> {
    let snapshot: Snapshot = serde_json::from_value(row.state).map_err(corrupt)?;
    snapshot.verify_integrity().map_err(corrupt)?;
    if snapshot.world_id.as_uuid() != row.world_id
        || to_i64(snapshot.through_sequence.get(), "snapshot sequence")? != row.through_sequence
        || to_i64(snapshot.state.tick().get(), "snapshot tick")? != row.tick
        || i32::from(snapshot.snapshot_schema_version) != row.snapshot_schema_version
        || i32::try_from(snapshot.state.ruleset_version()).ok() != Some(row.ruleset_version)
        || digest_from_db(&row.checksum, "snapshot checksum")? != snapshot.state_hash
        || digest_from_db(&row.last_event_checksum, "snapshot event checksum")?
            != snapshot.last_event_hash
    {
        return Err(StoreError::Corrupt(format!(
            "snapshot at sequence {} disagrees with its indexed columns",
            snapshot.through_sequence
        )));
    }
    Ok(snapshot)
}

fn validate_transition(
    expected: WorldCursor,
    batch: &EventBatch,
    snapshot: &Snapshot,
) -> Result<(), StoreError> {
    let expected_sequence = expected
        .sequence
        .checked_next()
        .map_err(|error| StoreError::Conflict(error.to_string()))?;
    if batch.sequence != expected_sequence
        || batch.previous_hash != expected.last_event_hash
        || snapshot.world_id != batch.world_id
        || snapshot.through_sequence != batch.sequence
        || snapshot.last_event_hash != batch.batch_hash
        || snapshot.state_hash != batch.post_state_hash
        || snapshot.state.tick() != batch.tick
        || snapshot.state.ruleset_version() != batch.ruleset_version
    {
        return Err(StoreError::Conflict(
            "batch, snapshot, and expected cursor do not describe one transition".to_owned(),
        ));
    }
    Ok(())
}

fn deterministic_outbox_id(world_id: WorldId, sequence: EventSequence) -> Uuid {
    Uuid::new_v5(
        &world_id.as_uuid(),
        format!("projection:{}", sequence.get()).as_bytes(),
    )
}

fn status_text(status: WorldStatus) -> &'static str {
    match status {
        WorldStatus::Initializing => "initializing",
        WorldStatus::Running => "running",
        WorldStatus::Extinct => "extinct",
        WorldStatus::Archived => "archived",
    }
}

fn parse_status(status: &str) -> Result<WorldStatus, StoreError> {
    match status {
        "initializing" => Ok(WorldStatus::Initializing),
        "running" => Ok(WorldStatus::Running),
        "extinct" => Ok(WorldStatus::Extinct),
        "archived" => Ok(WorldStatus::Archived),
        other => Err(StoreError::Corrupt(format!("unknown world status {other}"))),
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
        .map_err(|_| StoreError::Conflict(format!("{field} exceeds PostgreSQL bigint range")))
}

fn from_i64(value: i64, field: &str) -> Result<u64, StoreError> {
    u64::try_from(value)
        .map_err(|_| StoreError::Corrupt(format!("{field} is unexpectedly negative")))
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
