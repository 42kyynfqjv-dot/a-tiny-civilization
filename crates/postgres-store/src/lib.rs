//! PostgreSQL implementation of application persistence ports.

use std::time::Duration;

use application::{FoundationStatus, FoundationStore, ServiceHeartbeat, StoreError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, postgres::PgPoolOptions};

mod memory_outbox;
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
}

#[derive(sqlx::FromRow)]
struct StatusRow {
    database_time: DateTime<Utc>,
    initializing_worlds: i64,
    running_worlds: i64,
    archived_worlds: i64,
    latest_runner_heartbeat: Option<DateTime<Utc>>,
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
                COUNT(*) FILTER (WHERE status = 'archived') AS archived_worlds,
                (
                    SELECT MAX(last_seen_at)
                    FROM service_heartbeats
                    WHERE service_name = 'simulation-runner'
                ) AS latest_runner_heartbeat
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
        })
    }
}

fn unavailable(error: sqlx::Error) -> StoreError {
    StoreError::Unavailable(error.to_string())
}
