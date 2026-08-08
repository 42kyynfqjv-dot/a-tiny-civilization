//! Authenticated observer-only supporter checkout orchestration.

use std::sync::Arc;

use observer_auth::ObserverSession;
use observer_projection::{
    ReservationRequest, ReservationState, ReservationStoreError, ReservationTarget,
    SupporterReservation, SupporterReservationStore,
};
use stripe_adapter::{
    PreparedStripeRefund, StripeCheckoutError, StripeCheckoutGateway, StripeCheckoutSession,
    StripeCheckoutSessionStore, StripeCheckoutStoreError, StripeRefundError, StripeRefundGateway,
    StripeRefundReason, StripeRefundStore, StripeRefundStoreError,
};
use thiserror::Error;
use uuid::Uuid;
use world_domain::{BirthCategory, WorldId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupporterCheckoutRequest {
    pub reservation_id: Uuid,
    pub world_id: WorldId,
    pub observer_label: String,
    pub target: ReservationTarget,
    pub birth_category: BirthCategory,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupporterCheckout {
    pub reservation: SupporterReservation,
    pub checkout: StripeCheckoutSession,
}

pub struct SupporterCheckoutService {
    reservations: Arc<dyn SupporterReservationStore>,
    sessions: Arc<dyn StripeCheckoutSessionStore>,
    gateway: Arc<dyn StripeCheckoutGateway>,
}

impl SupporterCheckoutService {
    #[must_use]
    pub fn new(
        reservations: Arc<dyn SupporterReservationStore>,
        sessions: Arc<dyn StripeCheckoutSessionStore>,
        gateway: Arc<dyn StripeCheckoutGateway>,
    ) -> Self {
        Self {
            reservations,
            sessions,
            gateway,
        }
    }

    pub async fn begin(
        &self,
        authenticated: &ObserverSession,
        request: &SupporterCheckoutRequest,
    ) -> Result<SupporterCheckout, SupporterCheckoutError> {
        let reservation = self
            .reservations
            .create_reservation(&ReservationRequest {
                reservation_id: request.reservation_id,
                world_id: request.world_id,
                supporter_subject: account_subject(authenticated),
                observer_label: request.observer_label.clone(),
                target: request.target.clone(),
                birth_category: request.birth_category.clone(),
            })
            .await?;
        if let Some(checkout) = self
            .sessions
            .load_checkout_session(request.reservation_id)
            .await?
        {
            return Ok(SupporterCheckout {
                reservation,
                checkout,
            });
        }
        if reservation.state != ReservationState::PendingPayment {
            return Err(SupporterCheckoutError::Conflict(
                "paid reservation has no durable Checkout correlation".to_owned(),
            ));
        }
        let checkout = self.gateway.create_session(request.reservation_id).await?;
        let checkout = self
            .sessions
            .record_checkout_session(request.reservation_id, &checkout)
            .await?;
        Ok(SupporterCheckout {
            reservation,
            checkout,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupporterCancellation {
    pub reservation: SupporterReservation,
    pub stripe_refund_id: Option<String>,
}

pub struct SupporterCancellationService {
    reservations: Arc<dyn SupporterReservationStore>,
    refunds: Arc<dyn StripeRefundStore>,
    gateway: Arc<dyn StripeRefundGateway>,
}

impl SupporterCancellationService {
    #[must_use]
    pub fn new(
        reservations: Arc<dyn SupporterReservationStore>,
        refunds: Arc<dyn StripeRefundStore>,
        gateway: Arc<dyn StripeRefundGateway>,
    ) -> Self {
        Self {
            reservations,
            refunds,
            gateway,
        }
    }

    pub async fn cancel(
        &self,
        authenticated: &ObserverSession,
        reservation_id: Uuid,
    ) -> Result<SupporterCancellation, SupporterCancellationError> {
        let reservation = self
            .reservations
            .cancel_reservation(reservation_id, &account_subject(authenticated))
            .await?;
        if reservation.payment_reference.is_none() {
            return Ok(SupporterCancellation {
                reservation,
                stripe_refund_id: None,
            });
        }
        let prepared = self
            .refunds
            .prepare_stripe_refund(reservation_id, StripeRefundReason::SupporterCancellation)
            .await?;
        let stripe_refund_id =
            complete_or_resume_refund(self.refunds.as_ref(), self.gateway.as_ref(), &prepared)
                .await?;
        Ok(SupporterCancellation {
            reservation,
            stripe_refund_id: Some(stripe_refund_id),
        })
    }
}

async fn complete_or_resume_refund(
    store: &dyn StripeRefundStore,
    gateway: &dyn StripeRefundGateway,
    prepared: &PreparedStripeRefund,
) -> Result<String, SupporterCancellationError> {
    if let Some(refund_id) = prepared.stripe_refund_id.as_ref() {
        return Ok(refund_id.clone());
    }
    let refund_id = gateway.create_refund(prepared).await?;
    store
        .complete_stripe_refund(prepared.reservation_id, &refund_id)
        .await?;
    Ok(refund_id)
}

fn account_subject(authenticated: &ObserverSession) -> String {
    format!("account:{}", authenticated.account_id)
}

#[derive(Debug, Error)]
pub enum SupporterCheckoutError {
    #[error(transparent)]
    Reservation(#[from] ReservationStoreError),
    #[error(transparent)]
    Stripe(#[from] StripeCheckoutError),
    #[error(transparent)]
    Session(#[from] StripeCheckoutStoreError),
    #[error("supporter Checkout conflict: {0}")]
    Conflict(String),
}

#[derive(Debug, Error)]
pub enum SupporterCancellationError {
    #[error(transparent)]
    Reservation(#[from] ReservationStoreError),
    #[error(transparent)]
    RefundStore(#[from] StripeRefundStoreError),
    #[error(transparent)]
    Stripe(#[from] StripeRefundError),
}
