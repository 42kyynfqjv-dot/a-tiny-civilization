//! Runs deterministic observer projections over already-committed history.

use anyhow::{Context, Result};
use application::WorldStore;
use clap::{Parser, Subcommand};
use observer_projection::{ObserverOrganismStore, ObserverTimelineStore};
use postgres_store::PostgresStore;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
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
            loop {
                let world_ids = store.list_world_ids().await.context("list worlds")?;
                project_worlds(&store, &world_ids).await?;
                tokio::time::sleep(interval).await;
            }
        }
    }
    Ok(())
}

async fn project_worlds(store: &PostgresStore, world_ids: &[WorldId]) -> Result<()> {
    for world_id in world_ids {
        let cursor = store
            .public_timeline_cursor(*world_id)
            .await
            .context("load observer projection cursor")?;
        let batches = store
            .load_event_batches(*world_id, cursor)
            .await
            .context("load committed event batches")?;
        let mut applied = 0_u64;
        for batch in &batches {
            if store
                .apply_public_timeline_batch(batch)
                .await
                .context("persist observer timeline batch")?
            {
                applied += 1;
            }
        }
        let organism_cursor = store
            .public_organism_cursor(*world_id)
            .await
            .context("load public organism projection cursor")?;
        let organism_batches = store
            .load_event_batches(*world_id, organism_cursor)
            .await
            .context("load committed event batches for public organism projection")?;
        let mut indexed = 0_u64;
        for batch in &organism_batches {
            if store
                .apply_public_organism_batch(batch)
                .await
                .context("persist public organism batch")?
            {
                indexed += 1;
            }
        }
        let through = store
            .public_timeline_cursor(*world_id)
            .await
            .context("read observer projection cursor")?;
        tracing::info!(
            world_id = %world_id,
            applied_batches = applied,
            through_sequence = through.get(),
            indexed_batches = indexed,
            "public timeline projection completed"
        );
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
