//! HTTP read boundary for the external observer system.

use std::{sync::Arc, time::Instant};

use application::FoundationStore;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct ApiState {
    store: Arc<dyn FoundationStore>,
    environment: Arc<str>,
    started_at: Instant,
}

impl ApiState {
    #[must_use]
    pub fn new(store: Arc<dyn FoundationStore>, environment: impl Into<Arc<str>>) -> Self {
        Self {
            store,
            environment: environment.into(),
            started_at: Instant::now(),
        }
    }
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/status", get(status))
        .fallback(not_found)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
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
    environment: Arc<str>,
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
        environment: state.environment,
        database_time: status.database_time,
        worlds: WorldCounts {
            initializing: status.initializing_worlds,
            running: status.running_worlds,
            archived: status.archived_worlds,
        },
        latest_runner_heartbeat: status.latest_runner_heartbeat,
    }))
}

async fn not_found() -> ApiError {
    ApiError::NotFound
}

enum ApiError {
    NotFound,
    Unavailable,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "resource not found"),
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
