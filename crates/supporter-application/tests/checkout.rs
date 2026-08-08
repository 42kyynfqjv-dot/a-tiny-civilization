use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use anyhow::Result;
use application::WorldStore;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use observer_auth::{IdentityProvider, ObserverSession};
use observer_projection::ReservationTarget;
use postgres_store::PostgresStore;
use sqlx::PgPool;
use stripe_adapter::{
    PreparedStripeRefund, StripeCheckoutError, StripeCheckoutGateway, StripeCheckoutSession,
    StripeRefundError, StripeRefundGateway, StripeWebhookDisposition, StripeWebhookStore,
    VerifiedCheckoutPayment, VerifiedStripeEvent,
};
use supporter_application::{
    SupporterCancellationService, SupporterCheckoutRequest, SupporterCheckoutService,
};
use uuid::Uuid;
use world_domain::Digest;
use world_domain::{BirthCategory, WorldId, WorldManifest, WorldSeed};

struct FakeStripe {
    calls: AtomicUsize,
}

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
        Ok(format!("re_test_{}", request.reservation_id.simple()))
    }
}

#[async_trait]
impl StripeCheckoutGateway for FakeStripe {
    async fn create_session(
        &self,
        reservation_id: Uuid,
    ) -> Result<StripeCheckoutSession, StripeCheckoutError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(StripeCheckoutSession {
            session_id: format!("cs_test_{}", reservation_id.simple()),
            checkout_url: format!("https://checkout.stripe.com/c/pay/{reservation_id}")
                .parse()
                .expect("valid Checkout URL"),
        })
    }
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn authenticated_checkout_is_account_bound_durable_and_retry_safe(
    pool: PgPool,
) -> Result<()> {
    let store = Arc::new(PostgresStore::from_pool(pool.clone()));
    let world_id = WorldId::from_uuid(Uuid::new_v4());
    store
        .create_world(&WorldManifest::new(world_id, WorldSeed::new(77), 18), None)
        .await?;
    let now = Utc::now();
    let authenticated = ObserverSession {
        account_id: Uuid::new_v4(),
        provider: IdentityProvider::Apple,
        subject: "apple-private-subject".to_owned(),
        created_at: now,
        expires_at: now + Duration::hours(12),
    };
    let gateway = Arc::new(FakeStripe {
        calls: AtomicUsize::new(0),
    });
    let service = SupporterCheckoutService::new(store.clone(), store.clone(), gateway.clone());
    let request = SupporterCheckoutRequest {
        reservation_id: Uuid::new_v4(),
        world_id,
        observer_label: "River".to_owned(),
        target: ReservationTarget::Person,
        birth_category: BirthCategory::new("female").expect("birth category"),
    };
    let first = service.begin(&authenticated, &request).await?;
    let retried = service.begin(&authenticated, &request).await?;
    assert_eq!(first, retried);
    assert_eq!(gateway.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        first.reservation.request.supporter_subject,
        format!("account:{}", authenticated.account_id)
    );
    assert!(
        !first
            .reservation
            .request
            .supporter_subject
            .contains(&authenticated.subject)
    );

    let mut another_account = authenticated.clone();
    another_account.account_id = Uuid::new_v4();
    assert!(service.begin(&another_account, &request).await.is_err());
    assert_eq!(gateway.calls.load(Ordering::SeqCst), 1);
    let payment = VerifiedStripeEvent::Paid(VerifiedCheckoutPayment {
        event_id: "evt_supporter_cancel_fixture".to_owned(),
        event_type: "checkout.session.completed".to_owned(),
        checkout_session_id: first.checkout.session_id.clone(),
        payment_intent_id: "pi_supporter_cancel_fixture".to_owned(),
        reservation_id: request.reservation_id,
        amount_minor: 500,
        currency: "usd".to_owned(),
        live_mode: false,
        payload_hash: Digest::sha256(b"signed cancellation payment fixture"),
    });
    assert_eq!(
        store.record_verified_stripe_event(&payment).await?,
        StripeWebhookDisposition::PaymentRecorded
    );
    let refund_gateway = Arc::new(FakeRefund {
        calls: AtomicUsize::new(0),
    });
    let cancellation =
        SupporterCancellationService::new(store.clone(), store.clone(), refund_gateway.clone());
    assert!(
        cancellation
            .cancel(&another_account, request.reservation_id)
            .await
            .is_err()
    );
    let cancelled = cancellation
        .cancel(&authenticated, request.reservation_id)
        .await?;
    assert_eq!(
        cancelled.reservation.state,
        observer_projection::ReservationState::CancelledBySupporter
    );
    assert!(cancelled.stripe_refund_id.is_some());
    assert_eq!(
        cancellation
            .cancel(&authenticated, request.reservation_id)
            .await?,
        cancelled
    );
    assert_eq!(refund_gateway.calls.load(Ordering::SeqCst), 1);
    assert!(
        sqlx::query("DELETE FROM supporter_cancellations WHERE reservation_id=$1")
            .bind(request.reservation_id)
            .execute(&pool)
            .await
            .is_err()
    );
    let canonical_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_batches")
        .fetch_one(&pool)
        .await?;
    assert_eq!(canonical_events, 0);
    Ok(())
}
