//! HTTP read boundary for the external observer system.

use std::{sync::Arc, time::Instant};

use application::FoundationStore;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use observer_projection::{
    ObserverFindingStore, ObserverOrganismStore, ObserverTimelineStore, ObserverWorldStore,
    PublicFinding, PublicOrganism, PublicTimelineItem, PublicWorld, PublicWorldTelemetry,
};
use serde::Deserialize;
use serde::Serialize;
use stripe_adapter::{
    StripeWebhookDisposition, StripeWebhookError, StripeWebhookStore, StripeWebhookStoreError,
    StripeWebhookVerifier,
};
use tower_http::trace::TraceLayer;
use world_domain::{EntityId, WorldId};

/// Read-only observer composition. The simulation runner does not import this port.
pub trait ObserverReadStore:
    FoundationStore
    + ObserverTimelineStore
    + ObserverOrganismStore
    + ObserverWorldStore
    + ObserverFindingStore
{
}

impl<T> ObserverReadStore for T where
    T: FoundationStore
        + ObserverTimelineStore
        + ObserverOrganismStore
        + ObserverWorldStore
        + ObserverFindingStore
{
}

#[derive(Clone)]
pub struct ApiState {
    store: Arc<dyn ObserverReadStore>,
    environment: Arc<str>,
    started_at: Instant,
    stripe: Option<Arc<StripeWebhookRuntime>>,
}

impl ApiState {
    #[must_use]
    pub fn new(store: Arc<dyn ObserverReadStore>, environment: impl Into<Arc<str>>) -> Self {
        Self {
            store,
            environment: environment.into(),
            started_at: Instant::now(),
            stripe: None,
        }
    }

    #[must_use]
    pub fn with_stripe(
        mut self,
        verifier: StripeWebhookVerifier,
        store: Arc<dyn StripeWebhookStore>,
    ) -> Self {
        self.stripe = Some(Arc::new(StripeWebhookRuntime { verifier, store }));
        self
    }
}

struct StripeWebhookRuntime {
    verifier: StripeWebhookVerifier,
    store: Arc<dyn StripeWebhookStore>,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/status", get(status))
        .route(
            "/api/v1/supporters/stripe/webhook",
            post(stripe_webhook).layer(axum::extract::DefaultBodyLimit::max(65_536)),
        )
        .route("/api/v1/worlds", get(public_worlds))
        .route(
            "/api/v1/worlds/{world_id}/telemetry",
            get(public_world_telemetry),
        )
        .route("/api/v1/worlds/{world_id}/timeline", get(public_timeline))
        .route("/api/v1/worlds/{world_id}/findings", get(public_findings))
        .route("/api/v1/worlds/{world_id}/organisms", get(public_organisms))
        .route(
            "/api/v1/worlds/{world_id}/organisms/{organism_id}",
            get(public_organism),
        )
        .fallback(not_found)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Serialize)]
struct StripeWebhookResponse {
    received: bool,
    disposition: &'static str,
}

async fn stripe_webhook(
    State(state): State<ApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<StripeWebhookResponse>, ApiError> {
    let runtime = state.stripe.as_ref().ok_or(ApiError::NotFound)?;
    let signature = headers
        .get("Stripe-Signature")
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::BadRequest(
            "invalid_webhook_signature",
            "Stripe webhook signature is invalid",
        ))?;
    let event = runtime
        .verifier
        .verify(signature, &body, Utc::now().timestamp())
        .map_err(map_stripe_verification_error)?;
    let disposition = runtime
        .store
        .record_verified_stripe_event(&event)
        .await
        .map_err(map_stripe_store_error)?;
    let disposition = match disposition {
        StripeWebhookDisposition::PaymentRecorded => "payment_recorded",
        StripeWebhookDisposition::Duplicate => "duplicate",
        StripeWebhookDisposition::Ignored => "ignored",
    };
    Ok(Json(StripeWebhookResponse {
        received: true,
        disposition,
    }))
}

fn map_stripe_verification_error(error: StripeWebhookError) -> ApiError {
    tracing::warn!(error = %error, "Stripe webhook verification rejected");
    ApiError::BadRequest("invalid_webhook", "Stripe webhook verification failed")
}

fn map_stripe_store_error(error: StripeWebhookStoreError) -> ApiError {
    match error {
        StripeWebhookStoreError::Unavailable(message) => {
            tracing::error!(error = %message, "Stripe webhook persistence unavailable");
            ApiError::Unavailable
        }
        StripeWebhookStoreError::ReservationNotFound(reservation_id) => {
            tracing::warn!(%reservation_id, "Stripe webhook references unknown reservation");
            ApiError::Conflict(
                "unknown_reservation",
                "payment does not match an existing reservation",
            )
        }
        StripeWebhookStoreError::Conflict(message) => {
            tracing::warn!(error = %message, "Stripe webhook evidence conflict");
            ApiError::Conflict(
                "payment_conflict",
                "payment conflicts with existing evidence",
            )
        }
    }
}

#[derive(Serialize)]
struct HealthResponse<'a> {
    status: &'a str,
    service: &'a str,
    version: &'a str,
    uptime_seconds: u64,
}

async fn live(State(state): State<ApiState>) -> Json<HealthResponse<'static>> {
    Json(HealthResponse {
        status: "ok",
        service: "observer-api",
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: state.started_at.elapsed().as_secs(),
    })
}

async fn ready(State(state): State<ApiState>) -> Result<Json<HealthResponse<'static>>, ApiError> {
    state.store.ready().await.map_err(|error| {
        tracing::warn!(%error, "readiness check failed");
        ApiError::Unavailable
    })?;

    Ok(Json(HealthResponse {
        status: "ready",
        service: "observer-api",
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: state.started_at.elapsed().as_secs(),
    }))
}

#[derive(Serialize)]
struct StatusResponse {
    api_version: &'static str,
    environment: String,
    database_time: DateTime<Utc>,
    worlds: WorldCounts,
    latest_runner_heartbeat: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct WorldCounts {
    initializing: i64,
    running: i64,
    archived: i64,
}

async fn status(State(state): State<ApiState>) -> Result<Json<StatusResponse>, ApiError> {
    let status = state.store.foundation_status().await.map_err(|error| {
        tracing::error!(%error, "status query failed");
        ApiError::Unavailable
    })?;

    Ok(Json(StatusResponse {
        api_version: "v1",
        environment: state.environment.to_string(),
        database_time: status.database_time,
        worlds: WorldCounts {
            initializing: status.initializing_worlds,
            running: status.running_worlds,
            archived: status.archived_worlds,
        },
        latest_runner_heartbeat: status.latest_runner_heartbeat,
    }))
}

#[derive(Serialize)]
struct WorldsResponse {
    worlds: Vec<PublicWorld>,
}

async fn public_worlds(State(state): State<ApiState>) -> Result<Json<WorldsResponse>, ApiError> {
    let worlds = state
        .store
        .list_public_worlds()
        .await
        .map_err(log_observer_error)?;
    Ok(Json(WorldsResponse { worlds }))
}

async fn public_world_telemetry(
    State(state): State<ApiState>,
    Path(world_id): Path<String>,
) -> Result<Json<PublicWorldTelemetry>, ApiError> {
    let world_id = world_id
        .parse::<WorldId>()
        .map_err(|_| ApiError::NotFound)?;
    let telemetry = state
        .store
        .public_world_telemetry(world_id)
        .await
        .map_err(log_observer_error)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(telemetry))
}

#[derive(Debug, Deserialize)]
struct TimelineQuery {
    limit: Option<u16>,
}

#[derive(Serialize)]
struct TimelineResponse {
    projection_version: u16,
    items: Vec<PublicTimelineItem>,
}

#[derive(Serialize)]
struct FindingsResponse {
    projection_version: u16,
    findings: Vec<PublicFinding>,
}

async fn public_findings(
    State(state): State<ApiState>,
    Path(world_id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<FindingsResponse>, ApiError> {
    let world_id = world_id
        .parse::<WorldId>()
        .map_err(|_| ApiError::NotFound)?;
    let findings = state
        .store
        .list_public_findings(world_id, query.limit.unwrap_or(50))
        .await
        .map_err(log_observer_error)?;
    Ok(Json(FindingsResponse {
        projection_version: observer_projection::PUBLIC_FINDING_PROJECTION_VERSION,
        findings,
    }))
}

async fn public_timeline(
    State(state): State<ApiState>,
    Path(world_id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<TimelineResponse>, ApiError> {
    let world_id = world_id
        .parse::<WorldId>()
        .map_err(|_| ApiError::NotFound)?;
    let items = state
        .store
        .list_public_timeline(world_id, query.limit.unwrap_or(50))
        .await
        .map_err(|error| {
            tracing::error!(%error, "observer timeline read failed");
            ApiError::Unavailable
        })?;
    Ok(Json(TimelineResponse {
        projection_version: observer_projection::PUBLIC_TIMELINE_PROJECTION_VERSION,
        items,
    }))
}

#[derive(Serialize)]
struct OrganismsResponse {
    projection_version: u16,
    organisms: Vec<PublicOrganism>,
}

async fn public_organisms(
    State(state): State<ApiState>,
    Path(world_id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<OrganismsResponse>, ApiError> {
    let world_id = world_id
        .parse::<WorldId>()
        .map_err(|_| ApiError::NotFound)?;
    let organisms = state
        .store
        .list_public_organisms(world_id, query.limit.unwrap_or(50))
        .await
        .map_err(log_observer_error)?;
    Ok(Json(OrganismsResponse {
        projection_version: observer_projection::PUBLIC_ORGANISM_PROJECTION_VERSION,
        organisms,
    }))
}

async fn public_organism(
    State(state): State<ApiState>,
    Path((world_id, organism_id)): Path<(String, String)>,
) -> Result<Json<PublicOrganism>, ApiError> {
    let world_id = world_id
        .parse::<WorldId>()
        .map_err(|_| ApiError::NotFound)?;
    let organism_id = organism_id
        .parse::<EntityId>()
        .map_err(|_| ApiError::NotFound)?;
    let organism = state
        .store
        .get_public_organism(world_id, organism_id)
        .await
        .map_err(log_observer_error)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(organism))
}

fn log_observer_error(error: observer_projection::ObserverProjectionStoreError) -> ApiError {
    tracing::error!(%error, "observer projection read failed");
    ApiError::Unavailable
}

async fn not_found() -> ApiError {
    ApiError::NotFound
}

enum ApiError {
    NotFound,
    BadRequest(&'static str, &'static str),
    Conflict(&'static str, &'static str),
    Unavailable,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "resource not found"),
            Self::BadRequest(code, message) => (StatusCode::BAD_REQUEST, code, message),
            Self::Conflict(code, message) => (StatusCode::CONFLICT, code, message),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "observer data is temporarily unavailable",
            ),
        };

        (
            status,
            Json(serde_json::json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use observer_projection::PublicWorldInputStatus;
    use world_domain::{Digest, EventSequence, SimTick, WorldStatus};

    #[test]
    fn worlds_response_exposes_provisional_input_identity() {
        let composition_hash = Digest::sha256(b"composition");
        let response = WorldsResponse {
            worlds: vec![PublicWorld {
                world_id: "00000000-0000-0000-0000-000000000001"
                    .parse()
                    .expect("world ID"),
                status: WorldStatus::Running,
                through_sequence: EventSequence::new(2),
                tick: SimTick::new(3),
                manifest_hash: Digest::sha256(b"manifest"),
                event_hash: Digest::sha256(b"events"),
                state_hash: Digest::sha256(b"state"),
                predecessor_world_id: None,
                input_status: Some(PublicWorldInputStatus::ProvisionalNotScientificallyAdmitted),
                composition_id: Some("full-earth-provisional-v1".to_owned()),
                composition_version: Some("0.1.0".to_owned()),
                composition_hash: Some(composition_hash),
            }],
        };
        let value = serde_json::to_value(response).expect("serialize worlds response");
        let world = &value["worlds"][0];
        assert_eq!(
            world["input_status"],
            "provisional-not-scientifically-admitted"
        );
        assert_eq!(world["composition_id"], "full-earth-provisional-v1");
        assert_eq!(world["composition_version"], "0.1.0");
        assert_eq!(world["composition_hash"], composition_hash.to_string());
    }
}
