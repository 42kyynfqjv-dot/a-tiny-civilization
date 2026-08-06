use async_trait::async_trait;
use observer_projection::{
    ClaimProvenance, ObserverFindingStore, ObserverProjectionStoreError,
    PUBLIC_FINDING_PROJECTION_NAME, PUBLIC_FINDING_PROJECTION_VERSION, PublicFinding,
    PublicFindingKind,
};
use sqlx::FromRow;
use world_domain::{DomainEvent, EventId, EventSequence, OrganismRole, SimTick, WorldId};

use crate::PostgresStore;

#[derive(FromRow)]
struct FindingRow {
    projection_version: i32,
    world_id: uuid::Uuid,
    source_event_id: uuid::Uuid,
    source_sequence: i64,
    source_tick: i64,
    kind: String,
    finding_key: String,
    provenance: String,
    title: String,
    summary: String,
}

#[async_trait]
impl ObserverFindingStore for PostgresStore {
    async fn apply_public_finding_batch(
        &self,
        batch: &world_domain::EventBatch,
    ) -> Result<bool, ObserverProjectionStoreError> {
        batch
            .verify_integrity()
            .map_err(|e| corrupt(&e.to_string()))?;
        let mut tx = self.pool().begin().await.map_err(unavailable)?;
        verify(&mut tx, batch).await?;
        let cursor = lock(&mut tx, batch.world_id).await?;
        let sequence = i64::try_from(batch.sequence.get()).map_err(|_| corrupt("sequence"))?;
        if sequence <= cursor {
            tx.commit().await.map_err(unavailable)?;
            return Ok(false);
        }
        if sequence != cursor + 1 {
            return Err(corrupt("finding batch sequence gap"));
        }
        let mut latest_intro: [Option<&world_domain::EventRecord>; 2] = [None, None];
        for record in &batch.events {
            match &record.event {
                DomainEvent::WorldStarted { .. } => {
                    insert_finding(
                        &mut tx,
                        batch,
                        record,
                        PublicFindingKind::First,
                        "world_began",
                        "A world began",
                        "Initial conditions were committed to the public record.",
                    )
                    .await?
                }
                DomainEvent::OrganismInitialized {
                    organism_id, role, ..
                }
                | DomainEvent::OrganismBorn {
                    organism_id, role, ..
                } => {
                    let role_code = role_code(*role);
                    let result = sqlx::query("INSERT INTO observer_finding_lives (projection_version, world_id, organism_id, role) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
                        .bind(i32::from(PUBLIC_FINDING_PROJECTION_VERSION)).bind(batch.world_id.as_uuid()).bind(organism_id.as_uuid()).bind(role_code).execute(&mut *tx).await.map_err(unavailable)?;
                    if result.rows_affected() == 0 {
                        return Err(corrupt("duplicate organism introduction"));
                    }
                    let index = role_index(*role);
                    latest_intro[index] = Some(record);
                    let (key, title, summary) = match role {
                        OrganismRole::Person => (
                            "first_person_recorded",
                            "A first person was recorded",
                            "An individual person entered the durable observer record.",
                        ),
                        OrganismRole::Fauna => (
                            "first_animal_recorded",
                            "A first animal was recorded",
                            "An individual animal entered the durable observer record.",
                        ),
                    };
                    insert_finding(
                        &mut tx,
                        batch,
                        record,
                        PublicFindingKind::First,
                        key,
                        title,
                        summary,
                    )
                    .await?;
                }
                DomainEvent::OrganismDied { organism_id, .. } => {
                    let result = sqlx::query("INSERT INTO observer_finding_life_endings (projection_version, world_id, organism_id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
                        .bind(i32::from(PUBLIC_FINDING_PROJECTION_VERSION)).bind(batch.world_id.as_uuid()).bind(organism_id.as_uuid()).execute(&mut *tx).await.map_err(unavailable)?;
                    if result.rows_affected() == 0 {
                        return Err(corrupt("duplicate organism ending"));
                    }
                    insert_finding(
                        &mut tx,
                        batch,
                        record,
                        PublicFindingKind::First,
                        "first_life_ended",
                        "A first life ended",
                        "The first life-ending event entered the durable observer record.",
                    )
                    .await?;
                }
                DomainEvent::WorldExtinct => {
                    insert_finding(
                        &mut tx,
                        batch,
                        record,
                        PublicFindingKind::First,
                        "people_extinct",
                        "No people remained",
                        "The world reached its mechanical extinction condition.",
                    )
                    .await?
                }
                DomainEvent::WorldArchived => {
                    insert_finding(
                        &mut tx,
                        batch,
                        record,
                        PublicFindingKind::First,
                        "world_archived",
                        "A world entered its archive",
                        "Its committed history remains available for observation.",
                    )
                    .await?
                }
                DomainEvent::WorldConfigured { .. }
                | DomainEvent::TickAdvanced { .. }
                | DomainEvent::OrganismPerceived { .. }
                | DomainEvent::OrganismActed { .. } => {}
            }
        }
        for role in [OrganismRole::Person, OrganismRole::Fauna] {
            if let Some(record) = latest_intro[role_index(role)] {
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM observer_finding_lives l LEFT JOIN observer_finding_life_endings e ON e.projection_version=l.projection_version AND e.world_id=l.world_id AND e.organism_id=l.organism_id WHERE l.projection_version=$1 AND l.world_id=$2 AND l.role=$3 AND e.organism_id IS NULL")
                    .bind(i32::from(PUBLIC_FINDING_PROJECTION_VERSION)).bind(batch.world_id.as_uuid()).bind(role_code(role)).fetch_one(&mut *tx).await.map_err(unavailable)?;
                let metric = if role == OrganismRole::Person {
                    "people_population"
                } else {
                    "animal_population"
                };
                let previous = sqlx::query_scalar::<_, i64>("SELECT value FROM observer_finding_records WHERE projection_version=$1 AND world_id=$2 AND metric=$3 FOR UPDATE")
                    .bind(i32::from(PUBLIC_FINDING_PROJECTION_VERSION)).bind(batch.world_id.as_uuid()).bind(metric).fetch_optional(&mut *tx).await.map_err(unavailable)?.unwrap_or(-1);
                if count > previous {
                    sqlx::query("INSERT INTO observer_finding_records (projection_version,world_id,metric,value) VALUES ($1,$2,$3,$4) ON CONFLICT (projection_version,world_id,metric) DO UPDATE SET value=EXCLUDED.value")
                        .bind(i32::from(PUBLIC_FINDING_PROJECTION_VERSION)).bind(batch.world_id.as_uuid()).bind(metric).bind(count).execute(&mut *tx).await.map_err(unavailable)?;
                    let (key, title, summary) = if role == OrganismRole::Person {
                        (
                            format!("people_population_record_{count}"),
                            "A population record was reached",
                            format!("The recorded population reached {count} people."),
                        )
                    } else {
                        (
                            format!("animal_population_record_{count}"),
                            "An animal population record was reached",
                            format!("The recorded animal population reached {count}."),
                        )
                    };
                    insert_finding(
                        &mut tx,
                        batch,
                        record,
                        PublicFindingKind::Record,
                        &key,
                        title,
                        &summary,
                    )
                    .await?;
                }
            }
        }
        sqlx::query("UPDATE projection_offsets SET through_sequence=$3, updated_at=NOW() WHERE projection_name=$1 AND world_id=$2").bind(PUBLIC_FINDING_PROJECTION_NAME).bind(batch.world_id.as_uuid()).bind(sequence).execute(&mut *tx).await.map_err(unavailable)?;
        tx.commit().await.map_err(unavailable)?;
        Ok(true)
    }
    async fn public_finding_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError> {
        let value = sqlx::query_scalar::<_,i64>("SELECT through_sequence FROM projection_offsets WHERE projection_name=$1 AND world_id=$2").bind(PUBLIC_FINDING_PROJECTION_NAME).bind(world_id.as_uuid()).fetch_optional(self.pool()).await.map_err(unavailable)?.unwrap_or(0);
        Ok(EventSequence::new(
            u64::try_from(value).map_err(|_| corrupt("cursor"))?,
        ))
    }
    async fn list_public_findings(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<Vec<PublicFinding>, ObserverProjectionStoreError> {
        let rows = sqlx::query_as::<_,FindingRow>("SELECT projection_version,world_id,source_event_id,source_sequence,source_tick,kind,finding_key,provenance,title,summary FROM observer_findings WHERE projection_version=$1 AND world_id=$2 ORDER BY source_sequence DESC,source_event_id DESC LIMIT $3").bind(i32::from(PUBLIC_FINDING_PROJECTION_VERSION)).bind(world_id.as_uuid()).bind(i64::from(limit.clamp(1,200))).fetch_all(self.pool()).await.map_err(unavailable)?;
        rows.into_iter().map(parse).collect()
    }
}

async fn verify(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    batch: &world_domain::EventBatch,
) -> Result<(), ObserverProjectionStoreError> {
    let checksum = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT checksum FROM event_batches WHERE world_id=$1 AND sequence=$2",
    )
    .bind(batch.world_id.as_uuid())
    .bind(i64::try_from(batch.sequence.get()).map_err(|_| corrupt("sequence"))?)
    .fetch_optional(&mut **tx)
    .await
    .map_err(unavailable)?
    .ok_or_else(|| corrupt("uncommitted finding batch"))?;
    if checksum.as_slice() != batch.batch_hash.as_bytes() {
        return Err(corrupt("altered finding batch"));
    }
    Ok(())
}
async fn lock(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    world: WorldId,
) -> Result<i64, ObserverProjectionStoreError> {
    sqlx::query("INSERT INTO projection_offsets (projection_name,world_id,through_sequence,updated_at) VALUES ($1,$2,0,NOW()) ON CONFLICT DO NOTHING").bind(PUBLIC_FINDING_PROJECTION_NAME).bind(world.as_uuid()).execute(&mut **tx).await.map_err(unavailable)?;
    sqlx::query_scalar("SELECT through_sequence FROM projection_offsets WHERE projection_name=$1 AND world_id=$2 FOR UPDATE").bind(PUBLIC_FINDING_PROJECTION_NAME).bind(world.as_uuid()).fetch_one(&mut **tx).await.map_err(unavailable)
}
async fn insert_finding(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    batch: &world_domain::EventBatch,
    record: &world_domain::EventRecord,
    kind: PublicFindingKind,
    key: &str,
    title: &str,
    summary: &str,
) -> Result<(), ObserverProjectionStoreError> {
    sqlx::query("INSERT INTO observer_findings (projection_version,world_id,source_event_id,source_sequence,source_tick,kind,finding_key,provenance,title,summary) VALUES ($1,$2,$3,$4,$5,$6,$7,'world_fact',$8,$9) ON CONFLICT (projection_version,world_id,finding_key) DO NOTHING").bind(i32::from(PUBLIC_FINDING_PROJECTION_VERSION)).bind(batch.world_id.as_uuid()).bind(record.event_id.as_uuid()).bind(i64::try_from(batch.sequence.get()).map_err(|_|corrupt("sequence"))?).bind(i64::try_from(batch.tick.get()).map_err(|_|corrupt("tick"))?).bind(kind_code(kind)).bind(key).bind(title).bind(summary).execute(&mut **tx).await.map_err(unavailable)?;
    Ok(())
}
fn parse(r: FindingRow) -> Result<PublicFinding, ObserverProjectionStoreError> {
    Ok(PublicFinding {
        projection_version: u16::try_from(r.projection_version).map_err(|_| corrupt("version"))?,
        world_id: WorldId::from_uuid(r.world_id),
        source_event_id: EventId::from_uuid(r.source_event_id),
        source_sequence: EventSequence::new(
            u64::try_from(r.source_sequence).map_err(|_| corrupt("sequence"))?,
        ),
        source_tick: SimTick::new(u64::try_from(r.source_tick).map_err(|_| corrupt("tick"))?),
        kind: match r.kind.as_str() {
            "first" => PublicFindingKind::First,
            "record" => PublicFindingKind::Record,
            "streak" => PublicFindingKind::Streak,
            _ => return Err(corrupt("kind")),
        },
        finding_key: r.finding_key,
        provenance: match r.provenance.as_str() {
            "world_fact" => ClaimProvenance::WorldFact,
            _ => return Err(corrupt("provenance")),
        },
        title: r.title,
        summary: r.summary,
    })
}
const fn role_code(role: OrganismRole) -> &'static str {
    match role {
        OrganismRole::Person => "person",
        OrganismRole::Fauna => "fauna",
    }
}
const fn role_index(role: OrganismRole) -> usize {
    match role {
        OrganismRole::Person => 0,
        OrganismRole::Fauna => 1,
    }
}
const fn kind_code(kind: PublicFindingKind) -> &'static str {
    match kind {
        PublicFindingKind::First => "first",
        PublicFindingKind::Record => "record",
        PublicFindingKind::Streak => "streak",
    }
}
fn corrupt(message: &str) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Corrupt(message.to_owned())
}
fn unavailable(error: sqlx::Error) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Unavailable(error.to_string())
}
