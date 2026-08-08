use async_trait::async_trait;
use observer_projection::{
    ObserverProjectionStoreError, ObserverWorldStore, PublicWorld, PublicWorldInputStatus,
    PublicWorldTelemetry,
};
use sqlx::FromRow;
use world_domain::{Digest, EventSequence, SimTick, WorldId, WorldStatus};

use crate::PostgresStore;
use crate::{advance_projection_cursor, lock_projection_cursor, verify_committed_batch_range};

const PUBLIC_WORLD_TELEMETRY_PROJECTION_NAME: &str = "public-world-telemetry-v1";
const PUBLIC_WORLD_TELEMETRY_PROJECTION_VERSION: i32 = 1;

#[derive(FromRow)]
struct PublicWorldRow {
    id: uuid::Uuid,
    status: String,
    current_sequence: i64,
    current_tick: i64,
    manifest_checksum: Vec<u8>,
    last_event_checksum: Vec<u8>,
    current_state_checksum: Vec<u8>,
    predecessor_world_id: Option<uuid::Uuid>,
    composition_id: Option<String>,
    composition_version: Option<String>,
    composition_hash: Option<String>,
}

#[derive(FromRow)]
struct TelemetryRow {
    id: uuid::Uuid,
    current_sequence: i64,
    current_tick: i64,
    committed_batches: i64,
    committed_events: i64,
    canonical_payload_bytes: i64,
    last_committed_at: chrono::DateTime<chrono::Utc>,
    timeline_sequence: i64,
    organism_sequence: i64,
    findings_sequence: i64,
    telemetry_sequence: i64,
    living_people: i64,
    living_fauna: i64,
}

impl PostgresStore {
    pub async fn public_world_telemetry_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError> {
        let cursor = sqlx::query_scalar::<_, i64>(
            "SELECT through_sequence FROM projection_offsets WHERE projection_name=$1 AND world_id=$2",
        )
        .bind(PUBLIC_WORLD_TELEMETRY_PROJECTION_NAME)
        .bind(world_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?
        .unwrap_or(0);
        Ok(EventSequence::new(nonnegative(cursor, "telemetry cursor")?))
    }

    pub async fn apply_public_world_telemetry_batches(
        &self,
        batches: &[world_domain::EventBatch],
    ) -> Result<u64, ObserverProjectionStoreError> {
        let Some(first) = batches.first() else {
            return Ok(0);
        };
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        let cursor = lock_projection_cursor(
            &mut tx,
            PUBLIC_WORLD_TELEMETRY_PROJECTION_NAME,
            first.world_id,
        )
        .await?;
        let counter_cursor = sqlx::query_scalar::<_, i64>(
            "SELECT through_sequence FROM observer_world_telemetry WHERE projection_version=$1 AND world_id=$2",
        )
        .bind(PUBLIC_WORLD_TELEMETRY_PROJECTION_VERSION)
        .bind(first.world_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?
        .unwrap_or(0);
        if counter_cursor != cursor {
            return Err(corrupt("telemetry counter cursor"));
        }
        let start = batches.partition_point(|batch| {
            i64::try_from(batch.sequence.get()).is_ok_and(|sequence| sequence <= cursor)
        });
        let pending = &batches[start..];
        let Some(first_pending) = pending.first() else {
            tx.commit().await.map_err(unavailable)?;
            return Ok(0);
        };
        if i64::try_from(first_pending.sequence.get()).map_err(|_| corrupt("telemetry sequence"))?
            != cursor + 1
        {
            return Err(corrupt("noncontiguous telemetry range"));
        }
        verify_committed_batch_range(&mut tx, pending).await?;
        let committed_events = pending.iter().try_fold(0_u64, |total, batch| {
            total
                .checked_add(batch.events.len() as u64)
                .ok_or_else(|| corrupt("telemetry event count"))
        })?;
        let canonical_payload_bytes = pending.iter().try_fold(0_u64, |total, batch| {
            let bytes = serde_json::to_vec(batch)
                .map_err(|error| ObserverProjectionStoreError::Corrupt(error.to_string()))?;
            total
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| corrupt("telemetry payload bytes"))
        })?;
        let last_sequence = i64::try_from(pending[pending.len() - 1].sequence.get())
            .map_err(|_| corrupt("telemetry sequence"))?;
        sqlx::query(
            r#"
            INSERT INTO observer_world_telemetry
                (projection_version,world_id,through_sequence,committed_events,canonical_payload_bytes)
            VALUES ($1,$2,$3,$4,$5)
            ON CONFLICT (projection_version,world_id) DO UPDATE SET
                through_sequence=EXCLUDED.through_sequence,
                committed_events=observer_world_telemetry.committed_events+EXCLUDED.committed_events,
                canonical_payload_bytes=observer_world_telemetry.canonical_payload_bytes+EXCLUDED.canonical_payload_bytes
            "#,
        )
        .bind(PUBLIC_WORLD_TELEMETRY_PROJECTION_VERSION)
        .bind(first.world_id.as_uuid())
        .bind(last_sequence)
        .bind(i64::try_from(committed_events).map_err(|_| corrupt("telemetry event count"))?)
        .bind(i64::try_from(canonical_payload_bytes).map_err(|_| corrupt("telemetry payload bytes"))?)
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
        advance_projection_cursor(
            &mut tx,
            PUBLIC_WORLD_TELEMETRY_PROJECTION_NAME,
            first.world_id,
            last_sequence,
        )
        .await?;
        tx.commit().await.map_err(unavailable)?;
        u64::try_from(pending.len()).map_err(|_| corrupt("telemetry batch count"))
    }
}

#[async_trait]
impl ObserverWorldStore for PostgresStore {
    async fn list_public_worlds(&self) -> Result<Vec<PublicWorld>, ObserverProjectionStoreError> {
        let rows = sqlx::query_as::<_, PublicWorldRow>(
            r#"
            SELECT worlds.id, worlds.status, worlds.current_sequence, worlds.current_tick,
                   worlds.manifest_checksum, worlds.last_event_checksum,
                   worlds.current_state_checksum, worlds.predecessor_world_id,
                   provisional.composition ->> 'composition_id' AS composition_id,
                   provisional.composition ->> 'composition_version' AS composition_version,
                   provisional.composition ->> 'content_hash' AS composition_hash
            FROM worlds
            LEFT JOIN event_batches AS genesis
              ON genesis.world_id = worlds.id AND genesis.sequence = 1
            LEFT JOIN LATERAL (
                SELECT record -> 'event' -> 'data' -> 'configuration'
                              -> 'provisional_world_composition' AS composition
                FROM jsonb_array_elements(genesis.payload -> 'events') AS record
                WHERE record #>> '{event,type}' = 'world_configured'
                  AND record -> 'event' -> 'data' -> 'configuration'
                             ? 'provisional_world_composition'
                LIMIT 1
            ) AS provisional ON TRUE
            ORDER BY worlds.current_sequence DESC, worlds.id ASC
            "#,
        )
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        rows.into_iter().map(parse_row).collect()
    }

    async fn public_world_telemetry(
        &self,
        world_id: WorldId,
    ) -> Result<Option<PublicWorldTelemetry>, ObserverProjectionStoreError> {
        let row = sqlx::query_as::<_, TelemetryRow>(
            r#"
            SELECT w.id, w.current_sequence, w.current_tick,
                   w.current_sequence AS committed_batches,
                   COALESCE(telemetry.committed_events, 0) AS committed_events,
                   COALESCE(telemetry.canonical_payload_bytes, 0) AS canonical_payload_bytes,
                   COALESCE(latest.appended_at, w.created_at) AS last_committed_at,
                   projections.timeline_sequence, projections.organism_sequence,
                   projections.findings_sequence, projections.telemetry_sequence,
                   lives.living_people, lives.living_fauna
            FROM worlds w
            LEFT JOIN observer_world_telemetry telemetry
              ON telemetry.world_id=w.id AND telemetry.projection_version=1
            LEFT JOIN LATERAL (
                SELECT b.appended_at FROM event_batches b
                WHERE b.world_id=w.id ORDER BY b.sequence DESC LIMIT 1
            ) latest ON TRUE
            CROSS JOIN LATERAL (
                SELECT COALESCE(MAX(p.through_sequence) FILTER (WHERE p.projection_name = 'public-timeline-v1'), 0)::BIGINT AS timeline_sequence,
                       COALESCE(MAX(p.through_sequence) FILTER (WHERE p.projection_name = 'public-organism-v1'), 0)::BIGINT AS organism_sequence,
                       COALESCE(MAX(p.through_sequence) FILTER (WHERE p.projection_name = 'public-finding-v1'), 0)::BIGINT AS findings_sequence,
                       COALESCE(MAX(p.through_sequence) FILTER (WHERE p.projection_name = 'public-world-telemetry-v1'), 0)::BIGINT AS telemetry_sequence
                FROM projection_offsets p WHERE p.world_id = w.id
            ) projections
            CROSS JOIN LATERAL (
                SELECT COUNT(*) FILTER (WHERE l.role = 'person' AND e.organism_id IS NULL)::BIGINT AS living_people,
                       COUNT(*) FILTER (WHERE l.role = 'fauna' AND e.organism_id IS NULL)::BIGINT AS living_fauna
                FROM observer_finding_lives l
                LEFT JOIN observer_finding_life_endings e
                  ON e.world_id = l.world_id AND e.organism_id = l.organism_id
                 AND e.projection_version = l.projection_version
                WHERE l.world_id = w.id AND l.projection_version = 1
            ) lives
            WHERE w.id = $1
            "#,
        )
        .bind(world_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
        row.map(parse_telemetry).transpose()
    }
}

fn parse_telemetry(
    row: TelemetryRow,
) -> Result<PublicWorldTelemetry, ObserverProjectionStoreError> {
    let sequence = nonnegative(row.current_sequence, "world sequence")?;
    let timeline = nonnegative(row.timeline_sequence, "timeline sequence")?;
    let organisms = nonnegative(row.organism_sequence, "organism sequence")?;
    let findings = nonnegative(row.findings_sequence, "finding sequence")?;
    let telemetry = nonnegative(row.telemetry_sequence, "telemetry sequence")?;
    Ok(PublicWorldTelemetry {
        world_id: WorldId::from_uuid(row.id),
        through_sequence: EventSequence::new(sequence),
        tick: SimTick::new(nonnegative(row.current_tick, "world tick")?),
        committed_batches: nonnegative(row.committed_batches, "committed batch count")?,
        committed_events: nonnegative(row.committed_events, "committed event count")?,
        canonical_payload_bytes: nonnegative(
            row.canonical_payload_bytes,
            "canonical payload bytes",
        )?,
        last_committed_at: row.last_committed_at,
        timeline_through_sequence: EventSequence::new(timeline),
        organism_index_through_sequence: EventSequence::new(organisms),
        findings_through_sequence: EventSequence::new(findings),
        telemetry_through_sequence: EventSequence::new(telemetry),
        timeline_lag_batches: sequence.saturating_sub(timeline),
        organism_index_lag_batches: sequence.saturating_sub(organisms),
        findings_lag_batches: sequence.saturating_sub(findings),
        telemetry_lag_batches: sequence.saturating_sub(telemetry),
        living_people: nonnegative(row.living_people, "living people")?,
        living_fauna: nonnegative(row.living_fauna, "living fauna")?,
    })
}

fn nonnegative(value: i64, field: &str) -> Result<u64, ObserverProjectionStoreError> {
    u64::try_from(value).map_err(|_| corrupt(field))
}

fn parse_row(row: PublicWorldRow) -> Result<PublicWorld, ObserverProjectionStoreError> {
    let status = match row.status.as_str() {
        "initializing" => WorldStatus::Initializing,
        "running" => WorldStatus::Running,
        "extinct" => WorldStatus::Extinct,
        "archived" => WorldStatus::Archived,
        _ => return Err(corrupt("world status")),
    };
    let sequence = u64::try_from(row.current_sequence).map_err(|_| corrupt("world sequence"))?;
    let tick = u64::try_from(row.current_tick).map_err(|_| corrupt("world tick"))?;
    let (input_status, composition_id, composition_version, composition_hash) = match (
        row.composition_id,
        row.composition_version,
        row.composition_hash,
    ) {
        (None, None, None) => (None, None, None, None),
        (Some(id), Some(version), Some(hash)) => (
            Some(PublicWorldInputStatus::ProvisionalNotScientificallyAdmitted),
            Some(id),
            Some(version),
            Some(
                hash.parse::<Digest>()
                    .map_err(|_| corrupt("provisional composition checksum"))?,
            ),
        ),
        _ => return Err(corrupt("provisional composition metadata")),
    };
    Ok(PublicWorld {
        world_id: WorldId::from_uuid(row.id),
        status,
        through_sequence: EventSequence::new(sequence),
        tick: SimTick::new(tick),
        manifest_hash: parse_digest(row.manifest_checksum, "world manifest checksum")?,
        event_hash: parse_digest(row.last_event_checksum, "world event checksum")?,
        state_hash: parse_digest(row.current_state_checksum, "world state checksum")?,
        predecessor_world_id: row.predecessor_world_id.map(WorldId::from_uuid),
        input_status,
        composition_id,
        composition_version,
        composition_hash,
    })
}

fn parse_digest(bytes: Vec<u8>, field: &str) -> Result<Digest, ObserverProjectionStoreError> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| corrupt(field))?;
    Ok(Digest::from_bytes(bytes))
}

fn corrupt(field: &str) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Corrupt(format!("invalid stored {field}"))
}

fn unavailable(error: sqlx::Error) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Unavailable(error.to_string())
}
