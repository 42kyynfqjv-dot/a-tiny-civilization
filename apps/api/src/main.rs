use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use observer_api::ApiState;
use postgres_store::PostgresStore;
use stripe_adapter::StripeWebhookVerifier;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Parser)]
#[command(version, about = "A Tiny Civilization observer API")]
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
        #[arg(long, env = "STRIPE_WEBHOOK_SECRET", hide_env_values = true)]
        stripe_webhook_secret: Option<String>,
        #[arg(long, env = "STRIPE_WEBHOOK_TOLERANCE_SECONDS", default_value_t = 300)]
        stripe_webhook_tolerance_seconds: i64,
        #[arg(long, env = "STRIPE_LIVE_MODE", default_value_t = false)]
        stripe_live_mode: bool,
        #[arg(long, env = "STRIPE_SUPPORTER_CURRENCY", default_value = "usd")]
        stripe_supporter_currency: String,
        #[arg(long, env = "STRIPE_SUPPORTER_AMOUNT_MINOR", default_value_t = 500)]
        stripe_supporter_amount_minor: u64,
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
        stripe_webhook_secret: None,
        stripe_webhook_tolerance_seconds: 300,
        stripe_live_mode: false,
        stripe_supporter_currency: "usd".to_owned(),
        stripe_supporter_amount_minor: 500,
    }) {
        Command::Migrate => {
            store.migrate().await.context("apply database migrations")?;
            tracing::info!("database migrations complete");
        }
        Command::Serve {
            bind,
            environment,
            stripe_webhook_secret,
            stripe_webhook_tolerance_seconds,
            stripe_live_mode,
            stripe_supporter_currency,
            stripe_supporter_amount_minor,
        } => {
            let mut state = ApiState::new(Arc::new(store.clone()), environment);
            if let Some(secret) = stripe_webhook_secret.filter(|value| !value.is_empty()) {
                let verifier = StripeWebhookVerifier::new(
                    secret,
                    stripe_webhook_tolerance_seconds,
                    stripe_live_mode,
                    stripe_supporter_currency,
                    stripe_supporter_amount_minor,
                )
                .context("configure Stripe webhook verifier")?;
                state = state.with_stripe(verifier, Arc::new(store));
                tracing::info!(live_mode = stripe_live_mode, "Stripe webhook enabled");
            } else {
                tracing::info!("Stripe webhook disabled because no endpoint secret is configured");
            }
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
