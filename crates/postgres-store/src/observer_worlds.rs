use async_trait::async_trait;
use observer_projection::{
    ObserverProjectionStoreError, ObserverWorldStore, PublicWorld, PublicWorldInputStatus,
};
use sqlx::FromRow;
use world_domain::{Digest, EventSequence, SimTick, WorldId, WorldStatus};

use crate::PostgresStore;

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
