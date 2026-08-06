use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use observer_api::ApiState;
use postgres_store::PostgresStore;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Parser)]
#[command(version, about = "Emergent Civilization observer API")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    #[arg(long, env = "DATABASE_MAX_CONNECTIONS", default_value_t = 10)]
    database_max_connections: u32,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Apply all pending PostgreSQL migrations and exit.
    Migrate,
    /// Serve the observer API.
    Serve {
        #[arg(long, env = "API_BIND", default_value = "0.0.0.0:8080")]
        bind: SocketAddr,
        #[arg(long, env = "APP_ENV", default_value = "development")]
        environment: String,
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

    match cli.command.unwrap_or(Command::Serve {
        bind: "0.0.0.0:8080"
            .parse()
            .context("parse default API bind address")?,
        environment: "development".to_owned(),
    }) {
        Command::Migrate => {
            store.migrate().await.context("apply database migrations")?;
            tracing::info!("database migrations complete");
        }
        Command::Serve { bind, environment } => {
            let state = ApiState::new(Arc::new(store), environment);
            let listener = tokio::net::TcpListener::bind(bind)
                .await
                .with_context(|| format!("bind observer API to {bind}"))?;
            tracing::info!(%bind, "observer API listening");
            axum::serve(listener, observer_api::router(state))
                .with_graceful_shutdown(shutdown_signal())
                .await
                .context("serve observer API")?;
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

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install shutdown signal handler");
    }
}
