use async_trait::async_trait;
use observer_projection::{
    ClaimProvenance, ObserverOrganismStore, ObserverProjectionStoreError,
    PUBLIC_ORGANISM_PROJECTION_NAME, PUBLIC_ORGANISM_PROJECTION_VERSION, PublicOrganism,
    project_public_organisms,
};
use sqlx::FromRow;
use world_domain::{
    DomainEvent, EntityId, EventId, EventSequence, OrganismRole, SimTick, SpeciesIdentity, WorldId,
};

use crate::{
    PostgresStore, advance_projection_cursor, lock_projection_cursor, verify_committed_batch_range,
};

#[derive(FromRow)]
struct OrganismRow {
    projection_version: i32,
    world_id: uuid::Uuid,
    organism_id: uuid::Uuid,
    role: String,
    species_catalog: String,
    species_identifier: String,
    species_scientific_name: String,
    species_source_url: String,
    provenance: String,
    introduced_event_id: uuid::Uuid,
    introduced_sequence: i64,
    introduced_tick: i64,
    ended_event_id: Option<uuid::Uuid>,
    ended_sequence: Option<i64>,
    ended_tick: Option<i64>,
}

impl PostgresStore {
    pub async fn apply_public_organism_batches(
        &self,
        batches: &[world_domain::EventBatch],
    ) -> Result<u64, ObserverProjectionStoreError> {
        let Some(first) = batches.first() else {
            return Ok(0);
        };
        let mut transaction = self.pool().begin().await.map_err(unavailable)?;
        let cursor = lock_projection_cursor(
            &mut transaction,
            PUBLIC_ORGANISM_PROJECTION_NAME,
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
                "public organism index expected sequence {}, received {first_sequence}",
                cursor + 1
            )));
        }
        verify_committed_batch_range(&mut transaction, pending).await?;
        for batch in pending {
            let sequence = to_i64(batch.sequence.get(), "source sequence")?;
            for organism in project_public_organisms(batch) {
                insert_organism(&mut transaction, &organism).await?;
            }
            for record in &batch.events {
                if let DomainEvent::OrganismDied { organism_id, .. } = &record.event {
                    let inserted = sqlx::query(
                        r#"
                        INSERT INTO observer_organism_endings (
                            projection_version, world_id, organism_id, source_event_id, source_sequence, source_tick
                        )
                        VALUES ($1, $2, $3, $4, $5, $6)
                        ON CONFLICT (projection_version, world_id, organism_id) DO NOTHING
                        "#,
                    )
                    .bind(i32::from(PUBLIC_ORGANISM_PROJECTION_VERSION))
                    .bind(batch.world_id.as_uuid())
                    .bind(organism_id.as_uuid())
                    .bind(record.event_id.as_uuid())
                    .bind(sequence)
                    .bind(to_i64(batch.tick.get(), "source tick")?)
                    .execute(&mut *transaction)
                    .await
                    .map_err(unavailable)?;
                    if inserted.rows_affected() == 0 {
                        return Err(ObserverProjectionStoreError::Corrupt(format!(
                            "organism {organism_id} already has a public ending"
                        )));
                    }
                }
            }
        }
        let last_sequence = to_i64(pending[pending.len() - 1].sequence.get(), "source sequence")?;
        advance_projection_cursor(
            &mut transaction,
            PUBLIC_ORGANISM_PROJECTION_NAME,
            first.world_id,
            last_sequence,
        )
        .await?;
        transaction.commit().await.map_err(unavailable)?;
        u64::try_from(pending.len()).map_err(|_| corrupt("indexed batch count"))
    }
}

#[async_trait]
impl ObserverOrganismStore for PostgresStore {
    async fn apply_public_organism_batch(
        &self,
        batch: &world_domain::EventBatch,
    ) -> Result<bool, ObserverProjectionStoreError> {
        Ok(self
            .apply_public_organism_batches(std::slice::from_ref(batch))
            .await?
            == 1)
    }

    async fn public_organism_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError> {
        let cursor = sqlx::query_scalar::<_, i64>(
            "SELECT through_sequence FROM projection_offsets WHERE projection_name = $1 AND world_id = $2",
        )
        .bind(PUBLIC_ORGANISM_PROJECTION_NAME)
        .bind(world_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?
        .unwrap_or(0);
        Ok(EventSequence::new(to_u64(cursor, "organism cursor")?))
    }

    async fn list_public_organisms(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<Vec<PublicOrganism>, ObserverProjectionStoreError> {
        let rows = sqlx::query_as::<_, OrganismRow>(&format!(
            "{} ORDER BY o.introduced_sequence DESC, o.organism_id ASC LIMIT $2",
            organism_select("WHERE o.world_id = $1")
        ))
        .bind(world_id.as_uuid())
        .bind(i64::from(limit.clamp(1, 200)))
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
        rows.into_iter().map(parse_row).collect()
    }

    async fn get_public_organism(
        &self,
        world_id: WorldId,
        organism_id: EntityId,
    ) -> Result<Option<PublicOrganism>, ObserverProjectionStoreError> {
        let row = sqlx::query_as::<_, OrganismRow>(&format!(
            "{} AND o.organism_id = $2",
            organism_select("WHERE o.world_id = $1")
        ))
        .bind(world_id.as_uuid())
        .bind(organism_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
        row.map(parse_row).transpose()
    }
}

async fn insert_organism(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organism: &PublicOrganism,
) -> Result<(), ObserverProjectionStoreError> {
    organism
        .species
        .validate()
        .map_err(|error| ObserverProjectionStoreError::Corrupt(error.to_string()))?;
    let inserted = sqlx::query(
        r#"
        INSERT INTO observer_organisms (
            projection_version, world_id, organism_id, role, species_catalog, species_identifier,
            species_scientific_name, species_source_url, provenance, introduced_event_id,
            introduced_sequence, introduced_tick
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (projection_version, world_id, organism_id) DO NOTHING
        "#,
    )
    .bind(i32::from(organism.projection_version))
    .bind(organism.world_id.as_uuid())
    .bind(organism.organism_id.as_uuid())
    .bind(role_code(organism.role))
    .bind(&organism.species.catalog)
    .bind(&organism.species.identifier)
    .bind(&organism.species.scientific_name)
    .bind(&organism.species.source_url)
    .bind("world_fact")
    .bind(organism.introduced_event_id.as_uuid())
    .bind(to_i64(
        organism.introduced_sequence.get(),
        "introduced sequence",
    )?)
    .bind(to_i64(organism.introduced_tick.get(), "introduced tick")?)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    if inserted.rows_affected() == 0 {
        return Err(ObserverProjectionStoreError::Corrupt(format!(
            "organism {} was introduced more than once",
            organism.organism_id
        )));
    }
    Ok(())
}

fn organism_select(where_clause: &str) -> String {
    format!(
        r#"
        SELECT o.projection_version, o.world_id, o.organism_id, o.role, o.species_catalog,
            o.species_identifier, o.species_scientific_name, o.species_source_url, o.provenance,
            o.introduced_event_id, o.introduced_sequence, o.introduced_tick,
            e.source_event_id AS ended_event_id, e.source_sequence AS ended_sequence,
            e.source_tick AS ended_tick
        FROM observer_organisms o
        LEFT JOIN observer_organism_endings e
          ON e.projection_version = o.projection_version AND e.world_id = o.world_id
          AND e.organism_id = o.organism_id
        {where_clause} AND o.projection_version = {PUBLIC_ORGANISM_PROJECTION_VERSION}
        "#
    )
}

fn parse_row(row: OrganismRow) -> Result<PublicOrganism, ObserverProjectionStoreError> {
    Ok(PublicOrganism {
        projection_version: u16::try_from(row.projection_version)
            .map_err(|_| corrupt("projection version"))?,
        world_id: WorldId::from_uuid(row.world_id),
        organism_id: EntityId::from_uuid(row.organism_id),
        role: match row.role.as_str() {
            "person" => OrganismRole::Person,
            "fauna" => OrganismRole::Fauna,
            _ => return Err(corrupt("organism role")),
        },
        species: SpeciesIdentity::new(
            row.species_catalog,
            row.species_identifier,
            row.species_scientific_name,
            row.species_source_url,
        )
        .map_err(|error| ObserverProjectionStoreError::Corrupt(error.to_string()))?,
        provenance: match row.provenance.as_str() {
            "world_fact" => ClaimProvenance::WorldFact,
            _ => return Err(corrupt("organism provenance")),
        },
        introduced_event_id: EventId::from_uuid(row.introduced_event_id),
        introduced_sequence: EventSequence::new(to_u64(
            row.introduced_sequence,
            "introduced sequence",
        )?),
        introduced_tick: SimTick::new(to_u64(row.introduced_tick, "introduced tick")?),
        ended_event_id: row.ended_event_id.map(EventId::from_uuid),
        ended_sequence: row
            .ended_sequence
            .map(|value| to_u64(value, "ending sequence").map(EventSequence::new))
            .transpose()?,
        ended_tick: row
            .ended_tick
            .map(|value| to_u64(value, "ending tick").map(SimTick::new))
            .transpose()?,
    })
}

const fn role_code(role: OrganismRole) -> &'static str {
    match role {
        OrganismRole::Person => "person",
        OrganismRole::Fauna => "fauna",
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
