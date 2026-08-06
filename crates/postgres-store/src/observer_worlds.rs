use async_trait::async_trait;
use observer_projection::{ObserverProjectionStoreError, ObserverWorldStore, PublicWorld};
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
}

#[async_trait]
impl ObserverWorldStore for PostgresStore {
    async fn list_public_worlds(&self) -> Result<Vec<PublicWorld>, ObserverProjectionStoreError> {
        let rows = sqlx::query_as::<_, PublicWorldRow>(
            r#"
            SELECT id, status, current_sequence, current_tick, manifest_checksum,
                   last_event_checksum, current_state_checksum, predecessor_world_id
            FROM worlds
            ORDER BY current_sequence DESC, id ASC
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
    Ok(PublicWorld {
        world_id: WorldId::from_uuid(row.id),
        status,
        through_sequence: EventSequence::new(sequence),
        tick: SimTick::new(tick),
        manifest_hash: parse_digest(row.manifest_checksum, "world manifest checksum")?,
        event_hash: parse_digest(row.last_event_checksum, "world event checksum")?,
        state_hash: parse_digest(row.current_state_checksum, "world state checksum")?,
        predecessor_world_id: row.predecessor_world_id.map(WorldId::from_uuid),
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
