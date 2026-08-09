use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use observer_api::ApiState;
use observer_projection::SupporterReservationStore;
use oidc_adapter::{AppleOidcClient, GoogleOidcClient};
use postgres_store::PostgresStore;
use stripe_adapter::{
    StripeCheckoutClient, StripeRefundClient, StripeRefundGateway, StripeRefundReason,
    StripeRefundStore, StripeWebhookVerifier,
};
use supporter_application::{SupporterCancellationService, SupporterCheckoutService};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;
use uuid::Uuid;

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
#[allow(clippy::large_enum_variant)] // Parsed once at process startup; boxing every CLI field adds noise.
enum Command {
    /// Apply all pending PostgreSQL migrations and exit.
    Migrate,
    /// Issue or safely resume a full Stripe refund for a terminal supporter reservation.
    Refund {
        #[arg(long)]
        reservation_id: Uuid,
        #[arg(long, value_parser = [
            "moderation_rejection",
            "world_extinction",
            "supporter_cancellation",
            "duplicate_charge",
            "service_failure",
        ])]
        reason: String,
        #[arg(long, env = "STRIPE_SECRET_KEY", hide_env_values = true)]
        stripe_secret_key: String,
        #[arg(
            long,
            env = "STRIPE_API_BASE_URL",
            default_value = "https://api.stripe.com/"
        )]
        stripe_api_base_url: String,
    },
    /// List paid labels awaiting review and fail when the queue is stale.
    ModerationQueue {
        #[arg(long, default_value_t = 100)]
        limit: u32,
        #[arg(long, default_value_t = 60)]
        max_age_minutes: i64,
    },
    /// Approve or reject one paid label; rejection is followed by an idempotent refund.
    Moderate {
        #[arg(long)]
        reservation_id: Uuid,
        #[arg(long, value_parser = ["approve", "reject"])]
        decision: String,
        #[arg(long, env = "ATINY_MODERATOR_ID")]
        moderator_id: String,
        #[arg(long, env = "STRIPE_SECRET_KEY", hide_env_values = true)]
        stripe_secret_key: Option<String>,
        #[arg(
            long,
            env = "STRIPE_API_BASE_URL",
            default_value = "https://api.stripe.com/"
        )]
        stripe_api_base_url: String,
    },
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
        #[arg(long, env = "STRIPE_SECRET_KEY", hide_env_values = true)]
        stripe_secret_key: Option<String>,
        #[arg(long, env = "STRIPE_SUPPORTER_PRICE_ID")]
        stripe_supporter_price_id: Option<String>,
        #[arg(
            long,
            env = "STRIPE_SUCCESS_URL",
            default_value = "https://atinycivilization.com/?supporter=success"
        )]
        stripe_success_url: String,
        #[arg(
            long,
            env = "STRIPE_CANCEL_URL",
            default_value = "https://atinycivilization.com/?supporter=cancelled"
        )]
        stripe_cancel_url: String,
        #[arg(long, env = "GOOGLE_OAUTH_CLIENT_ID")]
        google_oauth_client_id: Option<String>,
        #[arg(long, env = "GOOGLE_OAUTH_CLIENT_SECRET", hide_env_values = true)]
        google_oauth_client_secret: Option<String>,
        #[arg(
            long,
            env = "GOOGLE_OAUTH_REDIRECT_URI",
            default_value = "https://atinycivilization.com/api/v1/auth/google/callback"
        )]
        google_oauth_redirect_uri: String,
        #[arg(long, env = "APPLE_CLIENT_ID")]
        apple_client_id: Option<String>,
        #[arg(long, env = "APPLE_TEAM_ID")]
        apple_team_id: Option<String>,
        #[arg(long, env = "APPLE_KEY_ID")]
        apple_key_id: Option<String>,
        #[arg(long, env = "APPLE_PRIVATE_KEY", hide_env_values = true)]
        apple_private_key: Option<String>,
        #[arg(
            long,
            env = "APPLE_REDIRECT_URI",
            default_value = "https://atinycivilization.com/api/v1/auth/apple/callback"
        )]
        apple_redirect_uri: String,
        #[arg(long, env = "NEWSLETTER_WEEKLY_SIGNUP_URL")]
        newsletter_weekly_signup_url: Option<String>,
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
        stripe_secret_key: None,
        stripe_supporter_price_id: None,
        stripe_success_url: "https://atinycivilization.com/?supporter=success".to_owned(),
        stripe_cancel_url: "https://atinycivilization.com/?supporter=cancelled".to_owned(),
        google_oauth_client_id: None,
        google_oauth_client_secret: None,
        google_oauth_redirect_uri: "https://atinycivilization.com/api/v1/auth/google/callback"
            .to_owned(),
        apple_client_id: None,
        apple_team_id: None,
        apple_key_id: None,
        apple_private_key: None,
        apple_redirect_uri: "https://atinycivilization.com/api/v1/auth/apple/callback".to_owned(),
        newsletter_weekly_signup_url: None,
    }) {
        Command::Migrate => {
            store.migrate().await.context("apply database migrations")?;
            tracing::info!("database migrations complete");
        }
        Command::Refund {
            reservation_id,
            reason,
            stripe_secret_key,
            stripe_api_base_url,
        } => {
            let reason = reason
                .parse::<StripeRefundReason>()
                .context("parse refund reason")?;
            let prepared = store
                .prepare_stripe_refund(reservation_id, reason)
                .await
                .context("durably prepare supporter refund")?;
            if let Some(refund_id) = prepared.stripe_refund_id.as_deref() {
                tracing::info!(%reservation_id, %refund_id, "supporter refund was already complete");
            } else {
                let gateway = StripeRefundClient::new(
                    &stripe_api_base_url,
                    stripe_secret_key,
                    Duration::from_secs(10),
                )
                .context("configure Stripe refund client")?;
                let refund_id = gateway
                    .create_refund(&prepared)
                    .await
                    .context("request idempotent Stripe refund")?;
                store
                    .complete_stripe_refund(reservation_id, &refund_id)
                    .await
                    .context("record Stripe refund completion")?;
                tracing::info!(%reservation_id, %refund_id, "supporter refund complete");
            }
        }
        Command::ModerationQueue {
            limit,
            max_age_minutes,
        } => {
            if max_age_minutes <= 0 {
                anyhow::bail!("max-age-minutes must be positive");
            }
            let reservations = store
                .list_pending_moderation(limit)
                .await
                .context("load supporter moderation queue")?;
            let now = Utc::now();
            let mut stale = 0_u32;
            for reservation in &reservations {
                let payment_verified_at = reservation
                    .payment_verified_at
                    .context("pending moderation reservation has no verified-payment timestamp")?;
                let age_minutes = (now - payment_verified_at).num_minutes().max(0);
                if age_minutes >= max_age_minutes {
                    stale = stale.saturating_add(1);
                }
                tracing::info!(
                    reservation_id = %reservation.request.reservation_id,
                    world_id = %reservation.request.world_id,
                    observer_label = %reservation.request.observer_label,
                    target = ?reservation.request.target,
                    birth_category = %reservation.request.birth_category.as_str(),
                    age_minutes,
                    "paid supporter label awaiting review"
                );
            }
            tracing::info!(
                pending = reservations.len(),
                stale,
                "moderation queue inspected"
            );
            if stale > 0 {
                anyhow::bail!(
                    "{stale} moderation item(s) exceeded the {max_age_minutes}-minute threshold"
                );
            }
        }
        Command::Moderate {
            reservation_id,
            decision,
            moderator_id,
            stripe_secret_key,
            stripe_api_base_url,
        } => match decision.as_str() {
            "approve" => {
                let reservation = store
                    .approve_reservation(reservation_id, &moderator_id)
                    .await
                    .context("approve supporter reservation")?;
                tracing::info!(
                    %reservation_id,
                    state = ?reservation.state,
                    "supporter label approved"
                );
            }
            "reject" => {
                store
                    .reject_reservation(reservation_id, &moderator_id)
                    .await
                    .context("reject supporter reservation")?;
                let prepared = store
                    .prepare_stripe_refund(reservation_id, StripeRefundReason::ModerationRejection)
                    .await
                    .context("durably prepare rejected-label refund")?;
                if let Some(refund_id) = prepared.stripe_refund_id.as_deref() {
                    tracing::info!(%reservation_id, %refund_id, "rejection refund was already complete");
                } else {
                    let secret_key = stripe_secret_key
                        .filter(|value| !value.is_empty())
                        .context(
                            "STRIPE_SECRET_KEY is required to reject and refund a paid label",
                        )?;
                    let gateway = StripeRefundClient::new(
                        &stripe_api_base_url,
                        secret_key,
                        Duration::from_secs(10),
                    )
                    .context("configure Stripe refund client")?;
                    let refund_id = gateway
                        .create_refund(&prepared)
                        .await
                        .context("request idempotent rejected-label refund")?;
                    store
                        .complete_stripe_refund(reservation_id, &refund_id)
                        .await
                        .context("record rejected-label refund completion")?;
                    tracing::info!(%reservation_id, %refund_id, "supporter label rejected and refunded");
                }
            }
            _ => unreachable!("clap validates moderation decision"),
        },
        Command::Serve {
            bind,
            environment,
            stripe_webhook_secret,
            stripe_webhook_tolerance_seconds,
            stripe_live_mode,
            stripe_supporter_currency,
            stripe_supporter_amount_minor,
            stripe_secret_key,
            stripe_supporter_price_id,
            stripe_success_url,
            stripe_cancel_url,
            google_oauth_client_id,
            google_oauth_client_secret,
            google_oauth_redirect_uri,
            apple_client_id,
            apple_team_id,
            apple_key_id,
            apple_private_key,
            apple_redirect_uri,
            newsletter_weekly_signup_url,
        } => {
            let secure_cookies = environment == "production";
            let mut state = ApiState::new(Arc::new(store.clone()), environment);
            let stripe_webhook_secret = nonempty(stripe_webhook_secret);
            let stripe_secret_key = nonempty(stripe_secret_key);
            let stripe_supporter_price_id = nonempty(stripe_supporter_price_id);
            let google_oauth_client_id = nonempty(google_oauth_client_id);
            let google_oauth_client_secret = nonempty(google_oauth_client_secret);
            let google = match (google_oauth_client_id, google_oauth_client_secret) {
                (Some(client_id), Some(client_secret)) => {
                    let client = GoogleOidcClient::new(
                        "https://accounts.google.com/o/oauth2/v2/auth",
                        "https://oauth2.googleapis.com/token",
                        "https://www.googleapis.com/oauth2/v3/certs",
                        client_id,
                        client_secret,
                        &google_oauth_redirect_uri,
                        Duration::from_secs(10),
                    )
                    .context("configure Google OIDC")?;
                    tracing::info!("Google observer sign-in enabled");
                    Some(client)
                }
                (None, None) => {
                    tracing::info!(
                        "Google observer sign-in disabled because credentials are absent"
                    );
                    None
                }
                _ => anyhow::bail!("Google OAuth client ID and secret must be configured together"),
            };
            let apple = match (
                nonempty(apple_client_id),
                nonempty(apple_team_id),
                nonempty(apple_key_id),
                nonempty(apple_private_key),
            ) {
                (Some(client_id), Some(team_id), Some(key_id), Some(private_key)) => {
                    let client = AppleOidcClient::new(
                        "https://appleid.apple.com/auth/authorize",
                        "https://appleid.apple.com/auth/token",
                        "https://appleid.apple.com/auth/keys",
                        client_id,
                        team_id,
                        key_id,
                        private_key,
                        &apple_redirect_uri,
                        Duration::from_secs(10),
                    )
                    .context("configure Sign in with Apple")?;
                    tracing::info!("Apple observer sign-in enabled");
                    Some(client)
                }
                (None, None, None, None) => {
                    tracing::info!(
                        "Apple observer sign-in disabled because credentials are absent"
                    );
                    None
                }
                _ => anyhow::bail!(
                    "Apple client ID, team ID, key ID, and private key must be configured together"
                ),
            };
            let auth_configured = google.is_some() || apple.is_some();
            if auth_configured {
                state = state.with_observer_auth(
                    google,
                    apple,
                    Arc::new(store.clone()),
                    secure_cookies,
                );
            }
            match nonempty(newsletter_weekly_signup_url) {
                Some(weekly) => {
                    state = state.with_newsletter(
                        external_https_url(&weekly)
                            .context("validate weekly newsletter signup URL")?,
                    );
                    tracing::info!("external newsletter signup enabled");
                }
                None => tracing::info!("external newsletter signup disabled"),
            }
            if let Some(secret) = stripe_webhook_secret.as_ref() {
                let verifier = StripeWebhookVerifier::new(
                    secret.as_bytes(),
                    stripe_webhook_tolerance_seconds,
                    stripe_live_mode,
                    stripe_supporter_currency.clone(),
                    stripe_supporter_amount_minor,
                )
                .context("configure Stripe webhook verifier")?;
                state = state.with_stripe(verifier, Arc::new(store.clone()));
                tracing::info!(live_mode = stripe_live_mode, "Stripe webhook enabled");
            } else {
                tracing::info!("Stripe webhook disabled because no endpoint secret is configured");
            }
            match (stripe_secret_key, stripe_supporter_price_id) {
                (Some(secret_key), Some(price_id))
                    if auth_configured && stripe_webhook_secret.is_some() =>
                {
                    let gateway = StripeCheckoutClient::new(
                        "https://api.stripe.com/",
                        secret_key.clone(),
                        price_id,
                        &stripe_success_url,
                        &stripe_cancel_url,
                        Duration::from_secs(10),
                    )
                    .context("configure Stripe Checkout")?;
                    state = state.with_supporter_checkout(SupporterCheckoutService::new(
                        Arc::new(store.clone()),
                        Arc::new(store.clone()),
                        Arc::new(gateway),
                    ));
                    let refund_gateway = StripeRefundClient::new(
                        "https://api.stripe.com/",
                        secret_key,
                        Duration::from_secs(10),
                    )
                    .context("configure authenticated supporter cancellation refunds")?;
                    state = state.with_supporter_cancellation(SupporterCancellationService::new(
                        Arc::new(store.clone()),
                        Arc::new(store.clone()),
                        Arc::new(refund_gateway),
                    ));
                    tracing::info!("authenticated supporter Checkout enabled");
                }
                (None, None) => tracing::info!(
                    "supporter Checkout disabled because Stripe product configuration is absent"
                ),
                (Some(_), Some(_)) => anyhow::bail!(
                    "supporter Checkout requires observer sign-in and the Stripe webhook endpoint"
                ),
                _ => anyhow::bail!(
                    "Stripe secret key and supporter price ID must be configured together"
                ),
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

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn external_https_url(value: &str) -> Result<Url> {
    let parsed = Url::parse(value).context("parse URL")?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        anyhow::bail!(
            "newsletter signup URL must be an absolute HTTPS URL without credentials or a fragment"
        );
    }
    Ok(parsed)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
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
