use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use anyhow::Result;
use application::WorldStore;
use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::{Duration, Utc};
use observer_auth::{
    IdentityProvider, NewObserverSession, ObserverSessionStore, SessionSecrets,
    VerifiedExternalIdentity,
};
use observer_projection::{ReservationRequest, ReservationTarget, SupporterReservationStore};
use postgres_store::PostgresStore;
use sqlx::PgPool;
use stripe_adapter::{
    PreparedStripeRefund, StripeRefundError, StripeRefundGateway, StripeWebhookDisposition,
    StripeWebhookStore, VerifiedCheckoutPayment, VerifiedStripeEvent,
};
use supporter_application::SupporterCancellationService;
use tower::ServiceExt;
use uuid::Uuid;
use world_domain::{BirthCategory, Digest, WorldId, WorldManifest, WorldSeed};

struct FakeRefund {
    calls: AtomicUsize,
}

#[async_trait]
impl StripeRefundGateway for FakeRefund {
    async fn create_refund(
        &self,
        request: &PreparedStripeRefund,
    ) -> Result<String, StripeRefundError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(format!("re_http_{}", request.reservation_id.simple()))
    }
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn cancellation_route_requires_account_session_and_csrf_and_refunds_once(
    pool: PgPool,
) -> Result<()> {
    let store = Arc::new(PostgresStore::from_pool(pool));
    let world_id = WorldId::from_uuid(Uuid::new_v4());
    store
        .create_world(&WorldManifest::new(world_id, WorldSeed::new(88), 18), None)
        .await?;

    let now = Utc::now();
    let secrets = SessionSecrets::generate()?;
    let session = store
        .admit_verified_identity(
            &VerifiedExternalIdentity {
                provider: IdentityProvider::Google,
                subject: "google-http-fixture".to_owned(),
                email: None,
                email_verified: false,
                authenticated_at: now,
            },
            &NewObserverSession {
                session_digest: secrets.session_digest(),
                csrf_digest: secrets.csrf_digest(),
                created_at: now,
                expires_at: now + Duration::hours(1),
            },
        )
        .await?;
    let reservation_id = Uuid::new_v4();
    store
        .create_reservation(&ReservationRequest {
            reservation_id,
            world_id,
            supporter_subject: format!("account:{}", session.account_id),
            observer_label: "Juniper".to_owned(),
            target: ReservationTarget::Person,
            birth_category: BirthCategory::new("female")?,
        })
        .await?;
    assert_eq!(
        store
            .record_verified_stripe_event(&VerifiedStripeEvent::Paid(VerifiedCheckoutPayment {
                event_id: "evt_http_cancel_fixture".to_owned(),
                event_type: "checkout.session.completed".to_owned(),
                checkout_session_id: "cs_http_cancel_fixture".to_owned(),
                payment_intent_id: "pi_http_cancel_fixture".to_owned(),
                reservation_id,
                amount_minor: 500,
                currency: "usd".to_owned(),
                live_mode: false,
                payload_hash: Digest::sha256(b"signed HTTP cancellation fixture"),
            },))
            .await?,
        StripeWebhookDisposition::PaymentRecorded
    );

    let refund = Arc::new(FakeRefund {
        calls: AtomicUsize::new(0),
    });
    let state = observer_api::ApiState::new(store.clone(), "test")
        .with_observer_auth(None, None, store.clone(), false)
        .with_supporter_cancellation(SupporterCancellationService::new(
            store.clone(),
            store,
            refund.clone(),
        ));
    let app = observer_api::router(state);
    let path = format!("/api/v1/supporters/{reservation_id}/cancel");
    let cookie = format!(
        "atiny_session={}; atiny_csrf={}",
        secrets.session_token(),
        secrets.csrf_token()
    );

    let unauthorized_list = app
        .clone()
        .oneshot(Request::get("/api/v1/supporters/reservations").body(Body::empty())?)
        .await?;
    assert_eq!(unauthorized_list.status(), StatusCode::UNAUTHORIZED);
    let list = app
        .clone()
        .oneshot(
            Request::get("/api/v1/supporters/reservations")
                .header("cookie", &cookie)
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body: serde_json::Value =
        serde_json::from_slice(&to_bytes(list.into_body(), 32_768).await?)?;
    assert_eq!(list_body["reservations"].as_array().map(Vec::len), Some(1));
    let listed = &list_body["reservations"][0];
    assert_eq!(listed["reservation_id"], reservation_id.to_string());
    assert!(listed.get("supporter_subject").is_none());
    assert!(listed.get("payment_reference").is_none());

    let missing_csrf = app
        .clone()
        .oneshot(
            Request::post(&path)
                .header("cookie", &cookie)
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(missing_csrf.status(), StatusCode::UNAUTHORIZED);

    let request = || {
        Request::post(&path)
            .header("cookie", &cookie)
            .header("x-csrf-token", secrets.csrf_token())
            .body(Body::empty())
    };
    let response = app.clone().oneshot(request()?).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 16_384).await?)?;
    assert_eq!(body["state"], "cancelled_by_supporter");
    assert_eq!(body["refunded"], true);
    assert!(body.get("stripe_refund_id").is_none());

    let retry = app.clone().oneshot(request()?).await?;
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(refund.calls.load(Ordering::SeqCst), 1);
    let after = app
        .oneshot(
            Request::get("/api/v1/supporters/reservations")
                .header("cookie", &cookie)
                .body(Body::empty())?,
        )
        .await?;
    let after_body: serde_json::Value =
        serde_json::from_slice(&to_bytes(after.into_body(), 32_768).await?)?;
    assert_eq!(
        after_body["reservations"][0]["state"],
        "cancelled_by_supporter"
    );
    assert_eq!(after_body["reservations"][0]["refund_state"], "completed");
    Ok(())
}
