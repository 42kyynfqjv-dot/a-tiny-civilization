//! PostgreSQL implementation of application persistence ports.

use std::time::Duration;

use application::{FoundationStatus, FoundationStore, ServiceHeartbeat, StoreError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use observer_projection::ObserverProjectionStoreError;
use sqlx::{PgPool, postgres::PgPoolOptions};
use world_domain::{EventBatch, WorldId};

mod cancer_research_jobs;
mod cancer_tissue_refinement;
mod cognition_jobs;
mod memory_outbox;
mod oauth_attempts;
mod observer_accounts;
mod observer_artifacts;
mod observer_findings;
mod observer_habitat;
mod observer_history_commitments;
mod observer_language;
mod observer_memory;
mod observer_organisms;
mod observer_research;
mod observer_timeline;
mod observer_worlds;
mod stripe_checkout_sessions;
mod stripe_refunds;
mod stripe_webhooks;
mod supporter_reservations;
mod world_store;

#[derive(Clone, Debug)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await
            .map_err(unavailable)?;

        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), StoreError> {
        // Embedded migrations include optional-provider-aware Cancer routing.
        sqlx::migrate!("../../db/migrations")
            .run(&self.pool)
            .await
            .map_err(|error| StoreError::Migration(error.to_string()))
    }

    #[must_use]
    pub const fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Holds the database-wide canonical-writer lease for the life of the returned
    /// connection. PostgreSQL releases this operational lock on connection loss.
    pub async fn acquire_runner_writer_lock(&self) -> Result<sqlx::PgConnection, StoreError> {
        const RUNNER_WRITER_LOCK_KEY: i64 = 0x4154_494E_5957_5249;
        self.acquire_writer_lock(RUNNER_WRITER_LOCK_KEY, "canonical-writer")
            .await
    }

    /// Holds a writer lease for exactly one canonical world. Independent worlds
    /// may advance in separate failure domains, while a second writer for the
    /// same world still fails closed and PostgreSQL releases the lease on crash.
    pub async fn acquire_world_writer_lock(
        &self,
        world_id: WorldId,
    ) -> Result<sqlx::PgConnection, StoreError> {
        const RUNNER_WRITER_LOCK_KEY: i64 = 0x4154_494E_5957_5249;
        const WORLD_WRITER_LOCK_NAMESPACE: i64 = 0x4154_494E_5957_4C44;
        let uuid = world_id.as_uuid();
        let bytes = uuid.as_bytes();
        let high = i64::from_be_bytes(bytes[..8].try_into().expect("UUID high half"));
        let low = i64::from_be_bytes(bytes[8..].try_into().expect("UUID low half"));
        let world_lock_key = WORLD_WRITER_LOCK_NAMESPACE ^ high ^ low;
        let mut connection = self.pool.acquire().await.map_err(unavailable)?;
        let global_scope_available: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock_shared($1)")
                .bind(RUNNER_WRITER_LOCK_KEY)
                .fetch_one(&mut *connection)
                .await
                .map_err(unavailable)?;
        if !global_scope_available {
            return Err(StoreError::Conflict(
                "an all-world simulation runner holds the canonical-writer lock".to_owned(),
            ));
        }
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(world_lock_key)
            .fetch_one(&mut *connection)
            .await
            .map_err(unavailable)?;
        if !acquired {
            return Err(StoreError::Conflict(format!(
                "another simulation runner holds the world {world_id} canonical-writer lock"
            )));
        }
        Ok(connection.detach())
    }

    async fn acquire_writer_lock(
        &self,
        lock_key: i64,
        lock_name: &str,
    ) -> Result<sqlx::PgConnection, StoreError> {
        let mut connection = self.pool.acquire().await.map_err(unavailable)?;
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(lock_key)
            .fetch_one(&mut *connection)
            .await
            .map_err(unavailable)?;
        if !acquired {
            return Err(StoreError::Conflict(format!(
                "another simulation runner holds the {lock_name} lock"
            )));
        }
        Ok(connection.detach())
    }
}

async fn lock_projection_cursor(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    projection_name: &str,
    world_id: WorldId,
) -> Result<i64, ObserverProjectionStoreError> {
    sqlx::query(
        "INSERT INTO projection_offsets (projection_name,world_id,through_sequence,updated_at) VALUES ($1,$2,0,NOW()) ON CONFLICT DO NOTHING",
    )
    .bind(projection_name)
    .bind(world_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(observer_unavailable)?;
    sqlx::query_scalar(
        "SELECT through_sequence FROM projection_offsets WHERE projection_name=$1 AND world_id=$2 FOR UPDATE",
    )
    .bind(projection_name)
    .bind(world_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(observer_unavailable)
}

async fn verify_committed_batch_range(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    batches: &[EventBatch],
) -> Result<(), ObserverProjectionStoreError> {
    let Some(first) = batches.first() else {
        return Ok(());
    };
    for batch in batches {
        batch
            .verify_integrity()
            .map_err(|error| ObserverProjectionStoreError::Corrupt(error.to_string()))?;
        if batch.world_id != first.world_id {
            return Err(ObserverProjectionStoreError::Corrupt(
                "projection batch range crosses worlds".to_owned(),
            ));
        }
    }
    if batches
        .windows(2)
        .any(|pair| pair[0].sequence.get().checked_add(1) != Some(pair[1].sequence.get()))
    {
        return Err(ObserverProjectionStoreError::Corrupt(
            "projection batch range is not contiguous".to_owned(),
        ));
    }
    let last = &batches[batches.len() - 1];
    let rows = sqlx::query_as::<_, (i64, Vec<u8>)>(
        "SELECT sequence,checksum FROM event_batches WHERE world_id=$1 AND sequence BETWEEN $2 AND $3 ORDER BY sequence",
    )
    .bind(first.world_id.as_uuid())
    .bind(i64::try_from(first.sequence.get()).map_err(|_| {
        ObserverProjectionStoreError::Corrupt("projection sequence overflow".to_owned())
    })?)
    .bind(i64::try_from(last.sequence.get()).map_err(|_| {
        ObserverProjectionStoreError::Corrupt("projection sequence overflow".to_owned())
    })?)
    .fetch_all(&mut **transaction)
    .await
    .map_err(observer_unavailable)?;
    if rows.len() != batches.len()
        || rows
            .iter()
            .zip(batches)
            .any(|((sequence, checksum), batch)| {
                u64::try_from(*sequence).ok() != Some(batch.sequence.get())
                    || checksum.as_slice() != batch.batch_hash.as_bytes()
            })
    {
        return Err(ObserverProjectionStoreError::Corrupt(
            "projection batch range differs from committed history".to_owned(),
        ));
    }
    Ok(())
}

async fn advance_projection_cursor(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    projection_name: &str,
    world_id: WorldId,
    through_sequence: i64,
) -> Result<(), ObserverProjectionStoreError> {
    sqlx::query(
        "UPDATE projection_offsets SET through_sequence=$3,updated_at=NOW() WHERE projection_name=$1 AND world_id=$2",
    )
    .bind(projection_name)
    .bind(world_id.as_uuid())
    .bind(through_sequence)
    .execute(&mut **transaction)
    .await
    .map_err(observer_unavailable)?;
    Ok(())
}

fn observer_unavailable(error: sqlx::Error) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Unavailable(error.to_string())
}

#[derive(sqlx::FromRow)]
struct StatusRow {
    database_time: DateTime<Utc>,
    initializing_worlds: i64,
    running_worlds: i64,
    archived_worlds: i64,
    latest_runner_heartbeat: Option<DateTime<Utc>>,
    latest_projector_heartbeat: Option<DateTime<Utc>>,
    latest_memory_worker_heartbeat: Option<DateTime<Utc>>,
    latest_cognition_worker_heartbeat: Option<DateTime<Utc>>,
}

#[async_trait]
impl FoundationStore for PostgresStore {
    async fn ready(&self) -> Result<(), StoreError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(unavailable)
    }

    async fn record_heartbeat(&self, heartbeat: &ServiceHeartbeat) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            INSERT INTO service_heartbeats (service_name, instance_id, last_seen_at, metadata)
            VALUES ($1, $2, NOW(), $3)
            ON CONFLICT (service_name, instance_id)
            DO UPDATE SET last_seen_at = EXCLUDED.last_seen_at, metadata = EXCLUDED.metadata
            "#,
        )
        .bind(&heartbeat.service_name)
        .bind(heartbeat.instance_id)
        .bind(&heartbeat.metadata)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(unavailable)
    }

    async fn foundation_status(&self) -> Result<FoundationStatus, StoreError> {
        let row = sqlx::query_as::<_, StatusRow>(
            r#"
            SELECT
                NOW() AS database_time,
                COUNT(*) FILTER (WHERE status = 'initializing') AS initializing_worlds,
                COUNT(*) FILTER (WHERE status = 'running') AS running_worlds,
                COUNT(*) FILTER (WHERE status IN ('archived', 'retired')) AS archived_worlds,
                (
                    SELECT MAX(last_seen_at)
                    FROM service_heartbeats
                    WHERE service_name = 'simulation-runner'
                ) AS latest_runner_heartbeat
                ,(
                    SELECT MAX(last_seen_at)
                    FROM service_heartbeats
                    WHERE service_name = 'observer-projector'
                ) AS latest_projector_heartbeat
                ,(
                    SELECT MAX(last_seen_at)
                    FROM service_heartbeats
                    WHERE service_name = 'memory-worker'
                ) AS latest_memory_worker_heartbeat
                ,(
                    SELECT MAX(last_seen_at)
                    FROM service_heartbeats
                    WHERE service_name = 'cognition-worker'
                ) AS latest_cognition_worker_heartbeat
            FROM worlds
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(unavailable)?;

        Ok(FoundationStatus {
            database_time: row.database_time,
            initializing_worlds: row.initializing_worlds,
            running_worlds: row.running_worlds,
            archived_worlds: row.archived_worlds,
            latest_runner_heartbeat: row.latest_runner_heartbeat,
            latest_projector_heartbeat: row.latest_projector_heartbeat,
            latest_memory_worker_heartbeat: row.latest_memory_worker_heartbeat,
            latest_cognition_worker_heartbeat: row.latest_cognition_worker_heartbeat,
        })
    }
}

fn unavailable(error: sqlx::Error) -> StoreError {
    StoreError::Unavailable(error.to_string())
}
