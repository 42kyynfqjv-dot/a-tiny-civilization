use async_trait::async_trait;
use observer_projection::{
    ClaimProvenance, ObserverProjectionStoreError, ObserverTimelineStore,
    PUBLIC_TIMELINE_PROJECTION_NAME, PublicTimelineItem, PublicTimelineKind,
    project_public_timeline,
};
use sqlx::FromRow;
use world_domain::{EventId, EventSequence, SimTick, WorldId};

use crate::PostgresStore;

#[derive(FromRow)]
struct TimelineRow {
    projection_version: i32,
    world_id: uuid::Uuid,
    source_event_id: uuid::Uuid,
    source_sequence: i64,
    source_tick: i64,
    source_event_index: i32,
    kind: String,
    provenance: String,
    title: String,
    summary: String,
}

#[async_trait]
impl ObserverTimelineStore for PostgresStore {
    async fn apply_public_timeline_batch(
        &self,
        batch: &world_domain::EventBatch,
    ) -> Result<bool, ObserverProjectionStoreError> {
        batch
            .verify_integrity()
            .map_err(|error| ObserverProjectionStoreError::Corrupt(error.to_string()))?;
        let mut transaction = self.pool().begin().await.map_err(unavailable)?;
        let committed_checksum = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT checksum FROM event_batches WHERE world_id = $1 AND sequence = $2",
        )
        .bind(batch.world_id.as_uuid())
        .bind(to_i64(batch.sequence.get(), "source sequence")?)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(unavailable)?
        .ok_or_else(|| {
            ObserverProjectionStoreError::Corrupt(format!(
                "timeline batch {}:{} is not committed",
                batch.world_id, batch.sequence
            ))
        })?;
        if committed_checksum.as_slice() != batch.batch_hash.as_bytes() {
            return Err(ObserverProjectionStoreError::Corrupt(format!(
                "timeline batch {}:{} differs from committed history",
                batch.world_id, batch.sequence
            )));
        }
        sqlx::query(
            r#"
            INSERT INTO projection_offsets (projection_name, world_id, through_sequence, updated_at)
            VALUES ($1, $2, 0, NOW())
            ON CONFLICT (projection_name, world_id) DO NOTHING
            "#,
        )
        .bind(PUBLIC_TIMELINE_PROJECTION_NAME)
        .bind(batch.world_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .map_err(unavailable)?;
        let cursor = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT through_sequence
            FROM projection_offsets
            WHERE projection_name = $1 AND world_id = $2
            FOR UPDATE
            "#,
        )
        .bind(PUBLIC_TIMELINE_PROJECTION_NAME)
        .bind(batch.world_id.as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .map_err(unavailable)?;
        let sequence = to_i64(batch.sequence.get(), "source sequence")?;
        if sequence <= cursor {
            transaction.commit().await.map_err(unavailable)?;
            return Ok(false);
        }
        if sequence != cursor + 1 {
            return Err(ObserverProjectionStoreError::Corrupt(format!(
                "public timeline expected sequence {}, received {sequence}",
                cursor + 1
            )));
        }
        insert_items(&mut transaction, &project_public_timeline(batch)).await?;
        sqlx::query(
            r#"
            INSERT INTO projection_offsets (projection_name, world_id, through_sequence, updated_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (projection_name, world_id)
            DO UPDATE SET through_sequence = EXCLUDED.through_sequence, updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(PUBLIC_TIMELINE_PROJECTION_NAME)
        .bind(batch.world_id.as_uuid())
        .bind(sequence)
        .execute(&mut *transaction)
        .await
        .map_err(unavailable)?;
        transaction.commit().await.map_err(unavailable)?;
        Ok(true)
    }

    async fn public_timeline_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError> {
        let cursor = sqlx::query_scalar::<_, i64>(
            "SELECT through_sequence FROM projection_offsets WHERE projection_name = $1 AND world_id = $2",
        )
        .bind(PUBLIC_TIMELINE_PROJECTION_NAME)
        .bind(world_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?
        .unwrap_or(0);
        Ok(EventSequence::new(to_u64(cursor, "timeline cursor")?))
    }

    async fn list_public_timeline(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<Vec<PublicTimelineItem>, ObserverProjectionStoreError> {
        let limit = limit.clamp(1, 200);
        let rows = sqlx::query_as::<_, TimelineRow>(
            r#"
            SELECT projection_version, world_id, source_event_id, source_sequence, source_tick,
                source_event_index, kind, provenance, title, summary
            FROM observer_timeline_items
            WHERE world_id = $1 AND projection_version = $2
            ORDER BY source_sequence DESC, source_event_index DESC
            LIMIT $3
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(i32::from(
            observer_projection::PUBLIC_TIMELINE_PROJECTION_VERSION,
        ))
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        rows.into_iter().map(parse_row).collect()
    }
}

async fn insert_items(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    items: &[PublicTimelineItem],
) -> Result<(), ObserverProjectionStoreError> {
    for item in items {
        validate_item(item)?;
        sqlx::query(
            r#"
            INSERT INTO observer_timeline_items (
                projection_version, world_id, source_event_id, source_sequence, source_tick,
                source_event_index, kind, provenance, title, summary
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (projection_version, source_event_id) DO NOTHING
            "#,
        )
        .bind(i32::from(item.projection_version))
        .bind(item.world_id.as_uuid())
        .bind(item.source_event_id.as_uuid())
        .bind(to_i64(item.source_sequence.get(), "source sequence")?)
        .bind(to_i64(item.source_tick.get(), "source tick")?)
        .bind(i32::try_from(item.source_event_index).map_err(|_| {
            ObserverProjectionStoreError::Corrupt(
                "source event index exceeds PostgreSQL range".to_owned(),
            )
        })?)
        .bind(kind_code(item.kind))
        .bind(provenance_code(item.provenance))
        .bind(&item.title)
        .bind(&item.summary)
        .execute(&mut **transaction)
        .await
        .map_err(unavailable)?;
    }
    Ok(())
}

fn validate_item(item: &PublicTimelineItem) -> Result<(), ObserverProjectionStoreError> {
    if item.projection_version != observer_projection::PUBLIC_TIMELINE_PROJECTION_VERSION
        || item.provenance != ClaimProvenance::WorldFact
        || item.title.is_empty()
        || item.title.len() > 160
        || item.summary.is_empty()
        || item.summary.len() > 480
        || item.title.chars().any(char::is_control)
        || item.summary.chars().any(char::is_control)
    {
        return Err(ObserverProjectionStoreError::Corrupt(
            "invalid public timeline item".to_owned(),
        ));
    }
    Ok(())
}

fn parse_row(row: TimelineRow) -> Result<PublicTimelineItem, ObserverProjectionStoreError> {
    Ok(PublicTimelineItem {
        projection_version: u16::try_from(row.projection_version)
            .map_err(|_| corrupt("projection version"))?,
        world_id: WorldId::from_uuid(row.world_id),
        source_event_id: EventId::from_uuid(row.source_event_id),
        source_sequence: EventSequence::new(to_u64(row.source_sequence, "source sequence")?),
        source_tick: SimTick::new(to_u64(row.source_tick, "source tick")?),
        source_event_index: u32::try_from(row.source_event_index)
            .map_err(|_| corrupt("source event index"))?,
        kind: parse_kind(&row.kind)?,
        provenance: parse_provenance(&row.provenance)?,
        title: row.title,
        summary: row.summary,
    })
}

const fn kind_code(kind: PublicTimelineKind) -> &'static str {
    match kind {
        PublicTimelineKind::WorldBegan => "world_began",
        PublicTimelineKind::InitialPersonPresent => "initial_person_present",
        PublicTimelineKind::InitialAnimalPresent => "initial_animal_present",
        PublicTimelineKind::PersonBorn => "person_born",
        PublicTimelineKind::AnimalBorn => "animal_born",
        PublicTimelineKind::LifeEnded => "life_ended",
        PublicTimelineKind::PeopleExtinct => "people_extinct",
        PublicTimelineKind::WorldArchived => "world_archived",
    }
}

const fn provenance_code(provenance: ClaimProvenance) -> &'static str {
    match provenance {
        ClaimProvenance::WorldFact => "world_fact",
        ClaimProvenance::ObservedEvidence => "observed_evidence",
        ClaimProvenance::ContemporaryClaim => "contemporary_claim",
        ClaimProvenance::LaterInterpretation => "later_interpretation",
        ClaimProvenance::ObserverInference => "observer_inference",
        ClaimProvenance::Disputed => "disputed",
    }
}

fn parse_kind(value: &str) -> Result<PublicTimelineKind, ObserverProjectionStoreError> {
    match value {
        "world_began" => Ok(PublicTimelineKind::WorldBegan),
        "initial_person_present" => Ok(PublicTimelineKind::InitialPersonPresent),
        "initial_animal_present" => Ok(PublicTimelineKind::InitialAnimalPresent),
        "person_born" => Ok(PublicTimelineKind::PersonBorn),
        "animal_born" => Ok(PublicTimelineKind::AnimalBorn),
        "life_ended" => Ok(PublicTimelineKind::LifeEnded),
        "people_extinct" => Ok(PublicTimelineKind::PeopleExtinct),
        "world_archived" => Ok(PublicTimelineKind::WorldArchived),
        _ => Err(corrupt("timeline kind")),
    }
}

fn parse_provenance(value: &str) -> Result<ClaimProvenance, ObserverProjectionStoreError> {
    match value {
        "world_fact" => Ok(ClaimProvenance::WorldFact),
        _ => Err(corrupt("timeline provenance")),
    }
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
