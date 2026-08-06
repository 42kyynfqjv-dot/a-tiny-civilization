use std::time::Duration;

use anyhow::{Context, Result};
use application::{FoundationStore, ServiceHeartbeat};
use clap::Parser;
use postgres_store::PostgresStore;
use serde_json::json;
use sim_engine::RULESET_VERSION;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(version, about = "A Tiny Civilization simulation runner")]
struct Config {
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    #[arg(long, env = "DATABASE_MAX_CONNECTIONS", default_value_t = 5)]
    database_max_connections: u32,

    #[arg(long, env = "RUNNER_HEARTBEAT_SECONDS", default_value_t = 10)]
    heartbeat_seconds: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    let config = Config::parse();
    let store = PostgresStore::connect(&config.database_url, config.database_max_connections)
        .await
        .context("connect runner to PostgreSQL")?;
    let instance_id = Uuid::new_v4();
    let heartbeat = ServiceHeartbeat {
        service_name: "simulation-runner".to_owned(),
        instance_id,
        metadata: json!({
            "ruleset_version": RULESET_VERSION,
            "runner_version": env!("CARGO_PKG_VERSION"),
            "mode": "foundation",
        }),
    };
    let mut interval = tokio::time::interval(Duration::from_secs(config.heartbeat_seconds.max(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    tracing::info!(%instance_id, ruleset_version = RULESET_VERSION, "runner started");

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(error) = store.record_heartbeat(&heartbeat).await {
                    tracing::warn!(%error, "runner heartbeat failed; will retry without stopping");
                }
            }
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    tracing::error!(%error, "failed to listen for shutdown signal");
                }
                tracing::info!("runner stopping");
                break;
            }
        }
    }

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}
