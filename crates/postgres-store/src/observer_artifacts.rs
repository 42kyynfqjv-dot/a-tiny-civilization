use async_trait::async_trait;
use observer_projection::{
    ClaimProvenance, ObserverArtifactStore, ObserverProjectionStoreError,
    PUBLIC_ARTIFACT_PROJECTION_NAME, PUBLIC_ARTIFACT_PROJECTION_VERSION, PublicArtifact,
    PublicArtifactTrace,
};
use sqlx::FromRow;
use world_domain::{
    DomainEvent, EntityId, EventId, EventSequence, MaterialIdentity, SimTick, WorldId,
};

use crate::{
    PostgresStore, advance_projection_cursor, lock_projection_cursor, verify_committed_batch_range,
};

#[derive(FromRow)]
struct ArtifactRow {
    world_id: uuid::Uuid,
    object_id: uuid::Uuid,
    material_catalog: String,
    material_identifier: String,
    material_name: String,
    material_source_url: String,
    first_event_id: uuid::Uuid,
    first_sequence: i64,
    first_tick: i64,
    latest_event_id: uuid::Uuid,
    latest_sequence: i64,
    latest_tick: i64,
    to_trace_units: i64,
}

#[derive(FromRow)]
struct ArtifactTraceRow {
    world_id: uuid::Uuid,
    object_id: uuid::Uuid,
    source_event_id: uuid::Uuid,
    source_sequence: i64,
    source_tick: i64,
    from_trace_units: i64,
    applied_force_units: i32,
    to_trace_units: i64,
    provenance: String,
}

impl PostgresStore {
    pub async fn apply_public_artifact_batches(
        &self,
        batches: &[world_domain::EventBatch],
    ) -> Result<u64, ObserverProjectionStoreError> {
        let Some(first) = batches.first() else {
            return Ok(0);
        };
        let mut transaction = self.pool().begin().await.map_err(unavailable)?;
        let cursor = lock_projection_cursor(
            &mut transaction,
            PUBLIC_ARTIFACT_PROJECTION_NAME,
            first.world_id,
        )
        .await?;
        let start = batches.partition_point(|batch| {
            i64::try_from(batch.sequence.get()).is_ok_and(|sequence| sequence <= cursor)
        });
        let pending = &batches[start..];
        let Some(first_pending) = pending.first() else {
            transaction.commit().await.map_err(unavailable)?;
            return Ok(0);
        };
        let first_sequence = to_i64(first_pending.sequence.get(), "source sequence")?;
        if first_sequence != cursor + 1 {
            return Err(ObserverProjectionStoreError::Corrupt(format!(
                "public artifact index expected sequence {}, received {first_sequence}",
                cursor + 1
            )));
        }
        verify_committed_batch_range(&mut transaction, pending).await?;
        for batch in pending {
            let sequence = to_i64(batch.sequence.get(), "source sequence")?;
            let tick = to_i64(batch.tick.get(), "source tick")?;
            for record in &batch.events {
                match &record.event {
                    DomainEvent::MaterialInstanceInitialized {
                        object_id,
                        material,
                        ..
                    } => {
                        material.validate().map_err(|error| {
                            ObserverProjectionStoreError::Corrupt(error.to_string())
                        })?;
                        let inserted = sqlx::query(
                            r#"
                            INSERT INTO observer_material_objects (
                              projection_version,world_id,object_id,material_catalog,
                              material_identifier,material_name,material_source_url,
                              introduced_event_id,introduced_sequence,introduced_tick
                            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                            ON CONFLICT (projection_version,world_id,object_id) DO NOTHING
                            "#,
                        )
                        .bind(i32::from(PUBLIC_ARTIFACT_PROJECTION_VERSION))
                        .bind(batch.world_id.as_uuid())
                        .bind(object_id.as_uuid())
                        .bind(&material.catalog)
                        .bind(&material.identifier)
                        .bind(&material.canonical_name)
                        .bind(&material.source_url)
                        .bind(record.event_id.as_uuid())
                        .bind(sequence)
                        .bind(tick)
                        .execute(&mut *transaction)
                        .await
                        .map_err(unavailable)?;
                        if inserted.rows_affected() != 1 {
                            return Err(corrupt("material object was introduced more than once"));
                        }
                    }
                    DomainEvent::MaterialSurfaceTraceChanged {
                        object_id,
                        from_trace_units,
                        applied_force_units,
                        to_trace_units,
                        ..
                    } => {
                        let inserted = sqlx::query(
                            r#"
                            INSERT INTO observer_artifact_traces (
                              projection_version,world_id,object_id,source_event_id,
                              source_sequence,source_tick,from_trace_units,applied_force_units,
                              to_trace_units,provenance
                            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'world_fact')
                            ON CONFLICT DO NOTHING
                            "#,
                        )
                        .bind(i32::from(PUBLIC_ARTIFACT_PROJECTION_VERSION))
                        .bind(batch.world_id.as_uuid())
                        .bind(object_id.as_uuid())
                        .bind(record.event_id.as_uuid())
                        .bind(sequence)
                        .bind(tick)
                        .bind(i64::from(*from_trace_units))
                        .bind(i32::from(*applied_force_units))
                        .bind(i64::from(*to_trace_units))
                        .execute(&mut *transaction)
                        .await
                        .map_err(unavailable)?;
                        if inserted.rows_affected() != 1 {
                            return Err(corrupt("artifact trace was projected more than once"));
                        }
                    }
                    _ => {}
                }
            }
        }
        let last_sequence = to_i64(pending[pending.len() - 1].sequence.get(), "source sequence")?;
        advance_projection_cursor(
            &mut transaction,
            PUBLIC_ARTIFACT_PROJECTION_NAME,
            first.world_id,
            last_sequence,
        )
        .await?;
        transaction.commit().await.map_err(unavailable)?;
        u64::try_from(pending.len()).map_err(|_| corrupt("artifact batch count"))
    }
}

#[async_trait]
impl ObserverArtifactStore for PostgresStore {
    async fn apply_public_artifact_batch(
        &self,
        batch: &world_domain::EventBatch,
    ) -> Result<bool, ObserverProjectionStoreError> {
        Ok(self
            .apply_public_artifact_batches(std::slice::from_ref(batch))
            .await?
            == 1)
    }

    async fn public_artifact_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError> {
        let cursor = sqlx::query_scalar::<_, i64>(
            "SELECT through_sequence FROM projection_offsets WHERE projection_name=$1 AND world_id=$2",
        )
        .bind(PUBLIC_ARTIFACT_PROJECTION_NAME)
        .bind(world_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?
        .unwrap_or(0);
        Ok(EventSequence::new(to_u64(cursor, "artifact cursor")?))
    }

    async fn list_public_artifacts(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<Vec<PublicArtifact>, ObserverProjectionStoreError> {
        let rows = sqlx::query_as::<_, ArtifactRow>(
            r#"
            WITH first_trace AS (
              SELECT DISTINCT ON (world_id,object_id)
                world_id,object_id,source_event_id,source_sequence,source_tick
              FROM observer_artifact_traces
              WHERE projection_version=$1 AND world_id=$2
              ORDER BY world_id,object_id,source_sequence ASC,source_event_id ASC
            ), latest_trace AS (
              SELECT DISTINCT ON (world_id,object_id)
                world_id,object_id,source_event_id,source_sequence,source_tick,to_trace_units
              FROM observer_artifact_traces
              WHERE projection_version=$1 AND world_id=$2
              ORDER BY world_id,object_id,source_sequence DESC,source_event_id DESC
            )
            SELECT material.world_id,material.object_id,material.material_catalog,
              material.material_identifier,material.material_name,material.material_source_url,
              first_trace.source_event_id AS first_event_id,
              first_trace.source_sequence AS first_sequence,first_trace.source_tick AS first_tick,
              latest_trace.source_event_id AS latest_event_id,
              latest_trace.source_sequence AS latest_sequence,latest_trace.source_tick AS latest_tick,
              latest_trace.to_trace_units
            FROM observer_material_objects material
            JOIN first_trace USING(world_id,object_id)
            JOIN latest_trace USING(world_id,object_id)
            WHERE material.projection_version=$1 AND material.world_id=$2
            ORDER BY latest_trace.source_sequence DESC,material.object_id ASC
            LIMIT $3
            "#,
        )
        .bind(i32::from(PUBLIC_ARTIFACT_PROJECTION_VERSION))
        .bind(world_id.as_uuid())
        .bind(i64::from(limit.clamp(1, 200)))
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        rows.into_iter().map(parse_row).collect()
    }

    async fn list_public_artifact_traces(
        &self,
        world_id: WorldId,
        object_id: EntityId,
        after_sequence: EventSequence,
        limit: u16,
    ) -> Result<Vec<PublicArtifactTrace>, ObserverProjectionStoreError> {
        let rows = sqlx::query_as::<_, ArtifactTraceRow>(
            r#"
            SELECT world_id,object_id,source_event_id,source_sequence,source_tick,
              from_trace_units,applied_force_units,to_trace_units,provenance
            FROM observer_artifact_traces
            WHERE projection_version=$1 AND world_id=$2 AND object_id=$3
              AND source_sequence>$4
            ORDER BY source_sequence ASC,source_event_id ASC
            LIMIT $5
            "#,
        )
        .bind(i32::from(PUBLIC_ARTIFACT_PROJECTION_VERSION))
        .bind(world_id.as_uuid())
        .bind(object_id.as_uuid())
        .bind(to_i64(after_sequence.get(), "trace cursor")?)
        .bind(i64::from(limit.clamp(1, 200)))
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        rows.into_iter().map(parse_trace_row).collect()
    }
}

fn parse_row(row: ArtifactRow) -> Result<PublicArtifact, ObserverProjectionStoreError> {
    Ok(PublicArtifact {
        projection_version: PUBLIC_ARTIFACT_PROJECTION_VERSION,
        world_id: WorldId::from_uuid(row.world_id),
        object_id: EntityId::from_uuid(row.object_id),
        material: MaterialIdentity::new(
            row.material_catalog,
            row.material_identifier,
            row.material_name,
            row.material_source_url,
        )
        .map_err(|error| ObserverProjectionStoreError::Corrupt(error.to_string()))?,
        trace_provenance: ClaimProvenance::WorldFact,
        classification_provenance: ClaimProvenance::ObserverInference,
        first_trace_event_id: EventId::from_uuid(row.first_event_id),
        first_trace_sequence: EventSequence::new(to_u64(row.first_sequence, "first sequence")?),
        first_trace_tick: SimTick::new(to_u64(row.first_tick, "first tick")?),
        latest_trace_event_id: EventId::from_uuid(row.latest_event_id),
        latest_trace_sequence: EventSequence::new(to_u64(row.latest_sequence, "latest sequence")?),
        latest_trace_tick: SimTick::new(to_u64(row.latest_tick, "latest tick")?),
        surface_trace_units: u32::try_from(row.to_trace_units)
            .map_err(|_| corrupt("surface trace units"))?,
    })
}

fn parse_trace_row(
    row: ArtifactTraceRow,
) -> Result<PublicArtifactTrace, ObserverProjectionStoreError> {
    if row.provenance != "world_fact" {
        return Err(corrupt("artifact trace provenance"));
    }
    Ok(PublicArtifactTrace {
        projection_version: PUBLIC_ARTIFACT_PROJECTION_VERSION,
        world_id: WorldId::from_uuid(row.world_id),
        object_id: EntityId::from_uuid(row.object_id),
        source_event_id: EventId::from_uuid(row.source_event_id),
        source_sequence: EventSequence::new(to_u64(row.source_sequence, "trace sequence")?),
        source_tick: SimTick::new(to_u64(row.source_tick, "trace tick")?),
        provenance: ClaimProvenance::WorldFact,
        from_trace_units: u32::try_from(row.from_trace_units)
            .map_err(|_| corrupt("from trace units"))?,
        applied_force_units: u16::try_from(row.applied_force_units)
            .map_err(|_| corrupt("applied force units"))?,
        to_trace_units: u32::try_from(row.to_trace_units).map_err(|_| corrupt("to trace units"))?,
    })
}

fn to_i64(value: u64, field: &str) -> Result<i64, ObserverProjectionStoreError> {
    i64::try_from(value).map_err(|_| corrupt(field))
}
fn to_u64(value: i64, field: &str) -> Result<u64, ObserverProjectionStoreError> {
    u64::try_from(value).map_err(|_| corrupt(field))
}
fn corrupt(field: &str) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Corrupt(format!("invalid stored {field}"))
}
fn unavailable(error: sqlx::Error) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Unavailable(error.to_string())
}
