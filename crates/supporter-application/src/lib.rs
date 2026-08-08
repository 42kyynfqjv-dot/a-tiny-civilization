//! Authenticated observer-only supporter checkout orchestration.

use std::sync::Arc;

use observer_auth::ObserverSession;
use observer_projection::{
    ReservationRequest, ReservationState, ReservationStoreError, ReservationTarget,
    SupporterReservation, SupporterReservationStore,
};
use stripe_adapter::{
    StripeCheckoutError, StripeCheckoutGateway, StripeCheckoutSession, StripeCheckoutSessionStore,
    StripeCheckoutStoreError,
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
                supporter_subject: format!("account:{}", authenticated.account_id),
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
