//! Runs deterministic observer projections over already-committed history.

use anyhow::{Context, Result};
use application::{FoundationStore, ServiceHeartbeat, WorldStore};
use clap::{Parser, Subcommand};
use observer_projection::{
    ObserverArtifactStore, ObserverFindingStore, ObserverHabitatStore, ObserverOrganismStore,
    ObserverTimelineStore, SupporterReservationStore,
};
use postgres_store::PostgresStore;
use serde_json::json;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;
use world_domain::WorldId;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Project committed civilization history for observers"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    #[arg(long, env = "DATABASE_MAX_CONNECTIONS", default_value_t = 4)]
    database_max_connections: u32,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Project every current and archived world once, or one explicit world if supplied.
    Once {
        #[arg(long)]
        world_id: Option<WorldId>,
    },
    /// Repeat read-only projection passes. This never initializes or advances a world.
    Serve {
        #[arg(long, env = "PROJECTOR_POLL_SECONDS", default_value_t = 5)]
        poll_seconds: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    let cli = Cli::parse();
    let store = PostgresStore::connect(&cli.database_url, cli.database_max_connections)
        .await
        .context("connect to PostgreSQL")?;
    match cli.command.unwrap_or(Command::Serve { poll_seconds: 5 }) {
        Command::Once { world_id } => {
            let world_ids = match world_id {
                Some(world_id) => vec![world_id],
                None => store.list_world_ids().await.context("list worlds")?,
            };
            project_worlds(&store, &world_ids).await?;
        }
        Command::Serve { poll_seconds } => {
            let interval = std::time::Duration::from_secs(poll_seconds.max(1));
            let heartbeat = ServiceHeartbeat {
                service_name: "observer-projector".to_owned(),
                instance_id: Uuid::new_v4(),
                metadata: json!({
                    "projector_version": env!("CARGO_PKG_VERSION"),
                    "mode": "continuous-projections",
                }),
            };
            loop {
                tokio::select! {
                    result = async {
                        let world_ids = store.list_world_ids().await.context("list worlds")?;
                        project_worlds(&store, &world_ids).await?;
                        store
                            .record_heartbeat(&heartbeat)
                            .await
                            .context("record observer-projector heartbeat")?;
                        tokio::time::sleep(interval).await;
                        Ok::<(), anyhow::Error>(())
                    } => result?,
                    _ = shutdown_signal() => {
                        tracing::info!("observer projector stopping");
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut interrupt = match signal(SignalKind::interrupt()) {
            Ok(signal) => signal,
            Err(error) => {
                tracing::error!(%error, "failed to install interrupt signal handler");
                std::future::pending::<()>().await;
                unreachable!();
            }
        };
        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(error) => {
                tracing::error!(%error, "failed to install termination signal handler");
                std::future::pending::<()>().await;
                unreachable!();
            }
        };
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install shutdown signal handler");
    }
}

async fn project_worlds(store: &PostgresStore, world_ids: &[WorldId]) -> Result<()> {
    for world_id in world_ids {
        let timeline_cursor = store
            .public_timeline_cursor(*world_id)
            .await
            .context("load observer projection cursor")?;
        let organism_cursor = store
            .public_organism_cursor(*world_id)
            .await
            .context("load public organism projection cursor")?;
        let finding_cursor = store
            .public_finding_cursor(*world_id)
            .await
            .context("load finding cursor")?;
        let telemetry_cursor = store
            .public_world_telemetry_cursor(*world_id)
            .await
            .context("load public telemetry cursor")?;
        let artifact_cursor = store
            .public_artifact_cursor(*world_id)
            .await
            .context("load public artifact cursor")?;
        let habitat_cursor = store
            .public_habitat_cursor(*world_id)
            .await
            .context("load public habitat cursor")?;
        let earliest_cursor = timeline_cursor
            .min(organism_cursor)
            .min(finding_cursor)
            .min(telemetry_cursor)
            .min(artifact_cursor)
            .min(habitat_cursor);
        let batches = store
            .load_event_batches(*world_id, earliest_cursor)
            .await
            .context("load committed event batches once for all public projections")?;
        let timeline_start = batches.partition_point(|batch| batch.sequence <= timeline_cursor);
        let organism_start = batches.partition_point(|batch| batch.sequence <= organism_cursor);
        let finding_start = batches.partition_point(|batch| batch.sequence <= finding_cursor);
        let telemetry_start = batches.partition_point(|batch| batch.sequence <= telemetry_cursor);
        let artifact_start = batches.partition_point(|batch| batch.sequence <= artifact_cursor);
        let habitat_start = batches.partition_point(|batch| batch.sequence <= habitat_cursor);
        // The default projector pool has four connections. Keep at most four
        // long-lived projection transactions concurrent, then run the fifth from
        // its independent cursor. Starting five here can starve one projection
        // until the pool timeout when rebuilding a substantial history.
        let (applied, indexed, findings, telemetry) = tokio::try_join!(
            project_timeline(store, &batches[timeline_start..]),
            project_organisms(store, &batches[organism_start..]),
            project_findings(store, &batches[finding_start..]),
            project_telemetry(store, &batches[telemetry_start..]),
        )?;
        let artifacts = project_artifacts(store, &batches[artifact_start..]).await?;
        let habitat = project_habitat(store, &batches[habitat_start..]).await?;
        // Archive is already an immutable canonical fact. Checking the durable lifecycle
        // state also covers worlds archived before this projector version was deployed.
        // Expiration is idempotent observer-side bookkeeping only.
        let world = store
            .load_world(*world_id)
            .await
            .context("load durable world lifecycle for supporter expiration")?;
        let expired_reservations = if world.status == world_domain::WorldStatus::Archived {
            store
                .expire_world_reservations(*world_id)
                .await
                .context("expire unmatched supporter reservations for archived world")?
        } else {
            0
        };
        let through = store
            .public_timeline_cursor(*world_id)
            .await
            .context("read observer projection cursor")?;
        tracing::info!(
            world_id = %world_id,
            applied_batches = applied,
            through_sequence = through.get(),
            indexed_batches = indexed,
            finding_batches = findings,
            telemetry_batches = telemetry,
            artifact_batches = artifacts,
            habitat_batches = habitat,
            expired_reservations,
            "public timeline projection completed"
        );
    }
    Ok(())
}

async fn project_habitat(
    store: &PostgresStore,
    batches: &[world_domain::EventBatch],
) -> Result<u64> {
    store
        .apply_public_habitat_batches(batches)
        .await
        .context("persist public habitat batch range")
}

async fn project_artifacts(
    store: &PostgresStore,
    batches: &[world_domain::EventBatch],
) -> Result<u64> {
    store
        .apply_public_artifact_batches(batches)
        .await
        .context("persist public artifact batch range")
}

async fn project_telemetry(
    store: &PostgresStore,
    batches: &[world_domain::EventBatch],
) -> Result<u64> {
    store
        .apply_public_world_telemetry_batches(batches)
        .await
        .context("persist public telemetry batch range")
}

async fn project_timeline(
    store: &PostgresStore,
    batches: &[world_domain::EventBatch],
) -> Result<u64> {
    store
        .apply_public_timeline_batches(batches)
        .await
        .context("persist observer timeline batch range")
}

async fn project_organisms(
    store: &PostgresStore,
    batches: &[world_domain::EventBatch],
) -> Result<u64> {
    store
        .apply_public_organism_batches(batches)
        .await
        .context("persist public organism batch range")
}

async fn project_findings(
    store: &PostgresStore,
    batches: &[world_domain::EventBatch],
) -> Result<u64> {
    store
        .apply_public_finding_batches(batches)
        .await
        .context("persist public finding batch range")
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}
