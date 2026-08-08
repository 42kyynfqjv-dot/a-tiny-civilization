//! HTTP read boundary for the external observer system.

use std::{sync::Arc, time::Instant};

use application::FoundationStore;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Form, Path, Query, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use observer_auth::{
    NewObserverSession, OAuthAttemptSecrets, ObserverAuthStore, ObserverAuthStoreError,
    ObserverSession, SessionSecrets,
};
use observer_projection::{
    ObserverFindingStore, ObserverOrganismStore, ObserverTimelineStore, ObserverWorldStore,
    PublicFinding, PublicOrganism, PublicTimelineItem, PublicWorld, PublicWorldTelemetry,
};
use oidc_adapter::{AppleOidcClient, GoogleOidcClient, OidcError};
use serde::Deserialize;
use serde::Serialize;
use stripe_adapter::{
    StripeWebhookDisposition, StripeWebhookError, StripeWebhookStore, StripeWebhookStoreError,
    StripeWebhookVerifier,
};
use supporter_application::{
    SupporterCancellationError, SupporterCancellationService, SupporterCheckoutError,
    SupporterCheckoutRequest, SupporterCheckoutService,
};
use tower_http::trace::TraceLayer;
use uuid::Uuid;
use world_domain::{BirthCategory, Digest, EntityId, WorldId};

const OAUTH_ATTEMPT_MINUTES: i64 = 10;
const OBSERVER_SESSION_DAYS: i64 = 30;

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
    auth: Option<Arc<AuthRuntime>>,
    supporter_checkout: Option<Arc<SupporterCheckoutService>>,
    supporter_cancellation: Option<Arc<SupporterCancellationService>>,
}

impl ApiState {
    #[must_use]
    pub fn new(store: Arc<dyn ObserverReadStore>, environment: impl Into<Arc<str>>) -> Self {
        Self {
            store,
            environment: environment.into(),
            started_at: Instant::now(),
            stripe: None,
            auth: None,
            supporter_checkout: None,
            supporter_cancellation: None,
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

    #[must_use]
    pub fn with_observer_auth(
        mut self,
        google: Option<GoogleOidcClient>,
        apple: Option<AppleOidcClient>,
        store: Arc<dyn ObserverAuthStore>,
        secure_cookies: bool,
    ) -> Self {
        self.auth = Some(Arc::new(AuthRuntime {
            google,
            apple,
            store,
            secure_cookies,
        }));
        self
    }

    #[must_use]
    pub fn with_supporter_checkout(mut self, service: SupporterCheckoutService) -> Self {
        self.supporter_checkout = Some(Arc::new(service));
        self
    }

    #[must_use]
    pub fn with_supporter_cancellation(mut self, service: SupporterCancellationService) -> Self {
        self.supporter_cancellation = Some(Arc::new(service));
        self
    }
}

struct StripeWebhookRuntime {
    verifier: StripeWebhookVerifier,
    store: Arc<dyn StripeWebhookStore>,
}

struct AuthRuntime {
    google: Option<GoogleOidcClient>,
    apple: Option<AppleOidcClient>,
    store: Arc<dyn ObserverAuthStore>,
    secure_cookies: bool,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/status", get(status))
        .route("/api/v1/auth/google/start", get(google_auth_start))
        .route("/api/v1/auth/google/callback", get(google_auth_callback))
        .route("/api/v1/auth/apple/start", get(apple_auth_start))
        .route(
            "/api/v1/auth/apple/callback",
            post(apple_auth_callback).layer(axum::extract::DefaultBodyLimit::max(16_384)),
        )
        .route("/api/v1/auth/session", get(auth_session))
        .route("/api/v1/auth/logout", post(auth_logout))
        .route(
            "/api/v1/supporters/checkout",
            post(supporter_checkout).layer(axum::extract::DefaultBodyLimit::max(16_384)),
        )
        .route(
            "/api/v1/supporters/reservations",
            get(supporter_reservations),
        )
        .route(
            "/api/v1/supporters/{reservation_id}/cancel",
            post(supporter_cancel),
        )
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
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request<_>| {
                tracing::info_span!(
                    "http_request",
                    method = %request.method(),
                    path = %request.uri().path()
                )
            }),
        )
        .with_state(state)
}

async fn google_auth_start(State(state): State<ApiState>) -> Result<Response, ApiError> {
    auth_start(state, observer_auth::IdentityProvider::Google).await
}

async fn apple_auth_start(State(state): State<ApiState>) -> Result<Response, ApiError> {
    auth_start(state, observer_auth::IdentityProvider::Apple).await
}

async fn auth_start(
    state: ApiState,
    provider: observer_auth::IdentityProvider,
) -> Result<Response, ApiError> {
    let runtime = state.auth.as_ref().ok_or(ApiError::NotFound)?;
    let now = Utc::now();
    let secrets = OAuthAttemptSecrets::generate().map_err(|_| ApiError::Unavailable)?;
    let attempt = secrets.attempt(
        provider,
        now,
        now + chrono::Duration::minutes(OAUTH_ATTEMPT_MINUTES),
    );
    runtime
        .store
        .create_oauth_attempt(&attempt)
        .await
        .map_err(map_auth_store_error)?;
    let authorization_url = match provider {
        observer_auth::IdentityProvider::Google => runtime
            .google
            .as_ref()
            .ok_or(ApiError::NotFound)?
            .authorization_url(&secrets),
        observer_auth::IdentityProvider::Apple => runtime
            .apple
            .as_ref()
            .ok_or(ApiError::NotFound)?
            .authorization_url(&secrets),
    };
    let mut response = redirect(authorization_url.as_str())?;
    append_cookie(
        &mut response,
        &set_cookie(
            cookie_name(runtime.secure_cookies, "oauth_binding"),
            &secrets.browser_binding(),
            OAUTH_ATTEMPT_MINUTES * 60,
            true,
            runtime.secure_cookies,
        ),
    )?;
    append_cookie(
        &mut response,
        &set_cookie(
            cookie_name(runtime.secure_cookies, "oauth_pkce"),
            &secrets.code_verifier(),
            OAUTH_ATTEMPT_MINUTES * 60,
            true,
            runtime.secure_cookies,
        ),
    )?;
    Ok(response)
}

#[derive(Deserialize)]
struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn google_auth_callback(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Response, ApiError> {
    auth_callback(
        state,
        headers,
        query,
        observer_auth::IdentityProvider::Google,
    )
    .await
}

async fn apple_auth_callback(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Form(query): Form<OAuthCallbackQuery>,
) -> Result<Response, ApiError> {
    auth_callback(
        state,
        headers,
        query,
        observer_auth::IdentityProvider::Apple,
    )
    .await
}

async fn auth_callback(
    state: ApiState,
    headers: HeaderMap,
    query: OAuthCallbackQuery,
    provider: observer_auth::IdentityProvider,
) -> Result<Response, ApiError> {
    let runtime = state.auth.as_ref().ok_or(ApiError::NotFound)?;
    match provider {
        observer_auth::IdentityProvider::Google if runtime.google.is_none() => {
            return Err(ApiError::NotFound);
        }
        observer_auth::IdentityProvider::Apple if runtime.apple.is_none() => {
            return Err(ApiError::NotFound);
        }
        _ => {}
    }
    if query.error.is_some() {
        return Err(ApiError::BadRequest(
            "login_rejected",
            "Google sign-in was not completed",
        ));
    }
    let code = query
        .code
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .ok_or(ApiError::Unauthorized)?;
    let state_secret = query
        .state
        .filter(|value| valid_secret(value))
        .ok_or(ApiError::Unauthorized)?;
    let binding = cookie_value(
        &headers,
        cookie_name(runtime.secure_cookies, "oauth_binding"),
    )
    .filter(|value| valid_secret(value))
    .ok_or(ApiError::Unauthorized)?;
    let verifier = cookie_value(&headers, cookie_name(runtime.secure_cookies, "oauth_pkce"))
        .filter(|value| valid_secret(value))
        .ok_or(ApiError::Unauthorized)?;
    let state_digest = Digest::sha256(state_secret.as_bytes());
    let now = Utc::now();
    let attempt = runtime
        .store
        .load_oauth_attempt(state_digest, Digest::sha256(binding.as_bytes()), now)
        .await
        .map_err(map_auth_store_error)?
        .ok_or(ApiError::Unauthorized)?;
    if attempt.provider != provider {
        return Err(ApiError::Unauthorized);
    }
    let identity = match provider {
        observer_auth::IdentityProvider::Google => {
            runtime
                .google
                .as_ref()
                .ok_or(ApiError::NotFound)?
                .complete(&code, &verifier, &attempt, now)
                .await
        }
        observer_auth::IdentityProvider::Apple => {
            runtime
                .apple
                .as_ref()
                .ok_or(ApiError::NotFound)?
                .complete(&code, &attempt, now)
                .await
        }
    }
    .map_err(map_oidc_error)?;
    if !runtime
        .store
        .consume_oauth_attempt(state_digest, now)
        .await
        .map_err(map_auth_store_error)?
    {
        return Err(ApiError::Unauthorized);
    }
    let secrets = SessionSecrets::generate().map_err(|_| ApiError::Unavailable)?;
    let session_input = NewObserverSession {
        session_digest: secrets.session_digest(),
        csrf_digest: secrets.csrf_digest(),
        created_at: now,
        expires_at: now + chrono::Duration::days(OBSERVER_SESSION_DAYS),
    };
    runtime
        .store
        .admit_verified_identity(&identity, &session_input)
        .await
        .map_err(map_auth_store_error)?;
    let mut response = redirect("/")?;
    append_cookie(
        &mut response,
        &set_cookie(
            cookie_name(runtime.secure_cookies, "session"),
            &secrets.session_token(),
            OBSERVER_SESSION_DAYS * 86_400,
            true,
            runtime.secure_cookies,
        ),
    )?;
    append_cookie(
        &mut response,
        &set_cookie(
            cookie_name(runtime.secure_cookies, "csrf"),
            &secrets.csrf_token(),
            OBSERVER_SESSION_DAYS * 86_400,
            false,
            runtime.secure_cookies,
        ),
    )?;
    for kind in ["oauth_binding", "oauth_pkce"] {
        append_cookie(
            &mut response,
            &clear_cookie(
                cookie_name(runtime.secure_cookies, kind),
                true,
                runtime.secure_cookies,
            ),
        )?;
    }
    Ok(response)
}

#[derive(Serialize)]
struct AuthSessionResponse {
    authenticated: bool,
    account_id: Option<Uuid>,
}

async fn auth_session(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<AuthSessionResponse>, ApiError> {
    let runtime = state.auth.as_ref().ok_or(ApiError::NotFound)?;
    let session = authenticate(runtime, &headers, false).await?;
    Ok(Json(AuthSessionResponse {
        authenticated: session.is_some(),
        account_id: session.map(|value| value.account_id),
    }))
}

async fn auth_logout(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let runtime = state.auth.as_ref().ok_or(ApiError::NotFound)?;
    authenticate(runtime, &headers, true)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let token = session_secret(runtime, &headers)?;
    runtime
        .store
        .revoke_session(secret_digest(&token).ok_or(ApiError::Unauthorized)?)
        .await
        .map_err(map_auth_store_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    for (kind, http_only) in [("session", true), ("csrf", false)] {
        append_cookie(
            &mut response,
            &clear_cookie(
                cookie_name(runtime.secure_cookies, kind),
                http_only,
                runtime.secure_cookies,
            ),
        )?;
    }
    Ok(response)
}

#[derive(Deserialize)]
struct SupporterCheckoutBody {
    reservation_id: Uuid,
    world_id: WorldId,
    observer_label: String,
    target: observer_projection::ReservationTarget,
    birth_category: BirthCategory,
}

#[derive(Serialize)]
struct SupporterCheckoutResponse {
    reservation_id: Uuid,
    state: observer_projection::ReservationState,
    checkout_url: String,
}

async fn supporter_checkout(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(body): Json<SupporterCheckoutBody>,
) -> Result<Json<SupporterCheckoutResponse>, ApiError> {
    let auth = state.auth.as_ref().ok_or(ApiError::NotFound)?;
    let service = state
        .supporter_checkout
        .as_ref()
        .ok_or(ApiError::NotFound)?;
    let session = authenticate(auth, &headers, true)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let checkout = service
        .begin(
            &session,
            &SupporterCheckoutRequest {
                reservation_id: body.reservation_id,
                world_id: body.world_id,
                observer_label: body.observer_label,
                target: body.target,
                birth_category: body.birth_category,
            },
        )
        .await
        .map_err(map_supporter_checkout_error)?;
    Ok(Json(SupporterCheckoutResponse {
        reservation_id: checkout.reservation.request.reservation_id,
        state: checkout.reservation.state,
        checkout_url: checkout.checkout.checkout_url.to_string(),
    }))
}

#[derive(Serialize)]
struct SupporterCancellationResponse {
    reservation_id: Uuid,
    state: observer_projection::ReservationState,
    refunded: bool,
}

async fn supporter_cancel(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(reservation_id): Path<Uuid>,
) -> Result<Json<SupporterCancellationResponse>, ApiError> {
    let auth = state.auth.as_ref().ok_or(ApiError::NotFound)?;
    let service = state
        .supporter_cancellation
        .as_ref()
        .ok_or(ApiError::NotFound)?;
    let session = authenticate(auth, &headers, true)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let cancellation = service
        .cancel(&session, reservation_id)
        .await
        .map_err(map_supporter_cancellation_error)?;
    Ok(Json(SupporterCancellationResponse {
        reservation_id,
        state: cancellation.reservation.state,
        refunded: cancellation.stripe_refund_id.is_some(),
    }))
}

#[derive(Serialize)]
struct AccountReservationResponse {
    reservation_id: Uuid,
    world_id: WorldId,
    observer_label: String,
    target: observer_projection::ReservationTarget,
    birth_category: BirthCategory,
    state: observer_projection::ReservationState,
    created_at: DateTime<Utc>,
    payment_verified_at: Option<DateTime<Utc>>,
    activated_at: Option<DateTime<Utc>>,
    matched_birth: Option<observer_projection::MatchedBirth>,
    refund_state: Option<observer_projection::SupporterRefundState>,
}

#[derive(Serialize)]
struct AccountReservationsResponse {
    reservations: Vec<AccountReservationResponse>,
}

async fn supporter_reservations(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<AccountReservationsResponse>, ApiError> {
    let auth = state.auth.as_ref().ok_or(ApiError::NotFound)?;
    let service = state
        .supporter_cancellation
        .as_ref()
        .ok_or(ApiError::NotFound)?;
    let session = authenticate(auth, &headers, false)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let reservations = service
        .list_account_reservations(&session, query.limit.unwrap_or(100))
        .await
        .map_err(map_supporter_cancellation_error)?
        .into_iter()
        .map(|entry| AccountReservationResponse {
            reservation_id: entry.reservation.request.reservation_id,
            world_id: entry.reservation.request.world_id,
            observer_label: entry.reservation.request.observer_label,
            target: entry.reservation.request.target,
            birth_category: entry.reservation.request.birth_category,
            state: entry.reservation.state,
            created_at: entry.reservation.created_at,
            payment_verified_at: entry.reservation.payment_verified_at,
            activated_at: entry.reservation.activated_at,
            matched_birth: entry.reservation.matched_birth,
            refund_state: entry.refund_state,
        })
        .collect();
    Ok(Json(AccountReservationsResponse { reservations }))
}

async fn authenticate(
    runtime: &AuthRuntime,
    headers: &HeaderMap,
    require_csrf: bool,
) -> Result<Option<ObserverSession>, ApiError> {
    let session = session_secret(runtime, headers)?;
    let session_digest = secret_digest(&session).ok_or(ApiError::Unauthorized)?;
    if !require_csrf {
        return runtime
            .store
            .authenticate_session(session_digest, Utc::now())
            .await
            .map_err(map_auth_store_error);
    }
    let csrf_cookie = cookie_value(headers, cookie_name(runtime.secure_cookies, "csrf"))
        .filter(|value| valid_secret(value))
        .ok_or(ApiError::Unauthorized)?;
    let csrf_header = headers
        .get("X-CSRF-Token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_secret(value) && *value == csrf_cookie)
        .ok_or(ApiError::Unauthorized)?;
    runtime
        .store
        .authenticate_session_with_csrf(
            session_digest,
            secret_digest(csrf_header).ok_or(ApiError::Unauthorized)?,
            Utc::now(),
        )
        .await
        .map_err(map_auth_store_error)
}

fn session_secret(runtime: &AuthRuntime, headers: &HeaderMap) -> Result<String, ApiError> {
    cookie_value(headers, cookie_name(runtime.secure_cookies, "session"))
        .filter(|value| valid_secret(value))
        .ok_or(ApiError::Unauthorized)
}

fn cookie_name(secure: bool, kind: &str) -> String {
    if secure {
        format!("__Host-atiny_{kind}")
    } else {
        format!("atiny_{kind}")
    }
}

fn cookie_value(headers: &HeaderMap, name: String) -> Option<String> {
    let mut found = None;
    for header in headers.get_all(header::COOKIE) {
        let value = header.to_str().ok()?;
        for pair in value.split(';') {
            let (key, value) = pair.trim().split_once('=')?;
            if key == name {
                if found.is_some() {
                    return None;
                }
                found = Some(value.to_owned());
            }
        }
    }
    found
}

fn valid_secret(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn secret_digest(value: &str) -> Option<Digest> {
    if !valid_secret(value) {
        return None;
    }
    let bytes = hex::decode(value).ok()?;
    Some(Digest::sha256(&bytes))
}

fn set_cookie(
    name: String,
    value: &str,
    max_age_seconds: i64,
    http_only: bool,
    secure: bool,
) -> String {
    let mut cookie = format!("{name}={value}; Path=/; Max-Age={max_age_seconds}; SameSite=Lax");
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn clear_cookie(name: String, http_only: bool, secure: bool) -> String {
    set_cookie(name, "", 0, http_only, secure)
}

fn append_cookie(response: &mut Response, cookie: &str) -> Result<(), ApiError> {
    let value = HeaderValue::from_str(cookie).map_err(|_| ApiError::Unavailable)?;
    response.headers_mut().append(header::SET_COOKIE, value);
    Ok(())
}

fn redirect(location: &str) -> Result<Response, ApiError> {
    let location = HeaderValue::from_str(location).map_err(|_| ApiError::Unavailable)?;
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(header::LOCATION, location);
    Ok(response)
}

fn map_auth_store_error(error: ObserverAuthStoreError) -> ApiError {
    tracing::error!(error = %error, "observer authentication persistence failed");
    match error {
        ObserverAuthStoreError::Validation(_) | ObserverAuthStoreError::Conflict(_) => {
            ApiError::Unauthorized
        }
        ObserverAuthStoreError::Unavailable(_) | ObserverAuthStoreError::Corrupt(_) => {
            ApiError::Unavailable
        }
    }
}

fn map_oidc_error(error: OidcError) -> ApiError {
    tracing::warn!(error = %error, "Google OIDC callback rejected");
    match error {
        OidcError::Unavailable(_) => ApiError::Unavailable,
        OidcError::Configuration(_) => ApiError::Unavailable,
        OidcError::ProviderRejected(_)
        | OidcError::InvalidTokenResponse
        | OidcError::InvalidIdToken
        | OidcError::AttemptMismatch => ApiError::Unauthorized,
    }
}

fn map_supporter_checkout_error(error: SupporterCheckoutError) -> ApiError {
    tracing::warn!(error = %error, "supporter Checkout rejected");
    match error {
        SupporterCheckoutError::Reservation(
            observer_projection::ReservationStoreError::Validation(_),
        ) => ApiError::BadRequest("invalid_reservation", "supporter reservation is invalid"),
        SupporterCheckoutError::Conflict(_)
        | SupporterCheckoutError::Reservation(
            observer_projection::ReservationStoreError::Conflict(_),
        )
        | SupporterCheckoutError::Session(stripe_adapter::StripeCheckoutStoreError::Conflict(_)) => {
            ApiError::Conflict(
                "checkout_conflict",
                "supporter Checkout conflicts with existing evidence",
            )
        }
        SupporterCheckoutError::Reservation(
            observer_projection::ReservationStoreError::Unavailable(_)
            | observer_projection::ReservationStoreError::NotFound(_)
            | observer_projection::ReservationStoreError::Corrupt(_),
        )
        | SupporterCheckoutError::Session(
            stripe_adapter::StripeCheckoutStoreError::Unavailable(_)
            | stripe_adapter::StripeCheckoutStoreError::Corrupt(_),
        )
        | SupporterCheckoutError::Stripe(_) => ApiError::Unavailable,
    }
}

fn map_supporter_cancellation_error(error: SupporterCancellationError) -> ApiError {
    tracing::warn!(error = %error, "supporter cancellation rejected");
    match error {
        SupporterCancellationError::Reservation(
            observer_projection::ReservationStoreError::Unavailable(_)
            | observer_projection::ReservationStoreError::Corrupt(_),
        )
        | SupporterCancellationError::RefundStore(
            stripe_adapter::StripeRefundStoreError::Unavailable(_)
            | stripe_adapter::StripeRefundStoreError::Corrupt(_),
        )
        | SupporterCancellationError::Stripe(_) => ApiError::Unavailable,
        SupporterCancellationError::Reservation(_) | SupporterCancellationError::RefundStore(_) => {
            ApiError::Conflict(
                "cancellation_conflict",
                "reservation cannot be cancelled or refunded",
            )
        }
    }
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
    Unauthorized,
    BadRequest(&'static str, &'static str),
    Conflict(&'static str, &'static str),
    Unavailable,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "resource not found"),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "authentication or request proof is invalid",
            ),
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

    #[test]
    fn browser_session_token_round_trips_to_the_persisted_digest() {
        let secrets = SessionSecrets::generate().expect("OS entropy");
        assert_eq!(
            secret_digest(&secrets.session_token()),
            Some(secrets.session_digest())
        );
        assert_eq!(
            secret_digest(&secrets.csrf_token()),
            Some(secrets.csrf_digest())
        );
        assert!(secret_digest(&"A".repeat(64)).is_none());
    }

    #[test]
    fn cookie_parser_rejects_duplicate_names_and_production_flags_are_strict() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("a=one; b=two"));
        assert_eq!(
            cookie_value(&headers, "a".to_owned()).as_deref(),
            Some("one")
        );
        headers.insert(header::COOKIE, HeaderValue::from_static("a=one; a=two"));
        assert!(cookie_value(&headers, "a".to_owned()).is_none());

        let cookie = set_cookie(
            cookie_name(true, "session"),
            &"a".repeat(64),
            60,
            true,
            true,
        );
        assert!(cookie.starts_with("__Host-atiny_session="));
        assert!(cookie.contains("; Path=/"));
        assert!(cookie.contains("; SameSite=Lax"));
        assert!(cookie.contains("; HttpOnly"));
        assert!(cookie.ends_with("; Secure"));
        assert!(!cookie.contains("Domain="));
    }
}
