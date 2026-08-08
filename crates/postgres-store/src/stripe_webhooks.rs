use async_trait::async_trait;
use sqlx::{FromRow, Postgres, Transaction};
use stripe_adapter::{
    StripeWebhookDisposition, StripeWebhookStore, StripeWebhookStoreError, VerifiedCheckoutPayment,
    VerifiedIgnoredEvent, VerifiedStripeEvent,
};

use crate::PostgresStore;

#[derive(FromRow)]
struct WebhookRow {
    event_type: String,
    payload_hash: Vec<u8>,
    checkout_session_id: Option<String>,
    payment_intent_id: Option<String>,
    reservation_id: Option<uuid::Uuid>,
    amount_minor: Option<i64>,
    currency: Option<String>,
    live_mode: Option<bool>,
    outcome: String,
}

#[derive(FromRow)]
struct ReservationPaymentRow {
    state: String,
    payment_reference: Option<String>,
}

#[derive(FromRow)]
struct PriorPaymentRow {
    reservation_id: uuid::Uuid,
    checkout_session_id: String,
    payment_intent_id: Option<String>,
    amount_minor: i64,
    currency: String,
    live_mode: bool,
}

#[async_trait]
impl StripeWebhookStore for PostgresStore {
    async fn record_verified_stripe_event(
        &self,
        event: &VerifiedStripeEvent,
    ) -> Result<StripeWebhookDisposition, StripeWebhookStoreError> {
        let mut transaction = self.pool().begin().await.map_err(unavailable)?;
        let event_id = match event {
            VerifiedStripeEvent::Paid(payment) => payment.event_id.as_str(),
            VerifiedStripeEvent::Ignored(ignored) => ignored.event_id.as_str(),
        };
        lock_evidence_key(&mut transaction, event_id, 0x4154_5354_5249_5045).await?;
        if let Some(existing) = load_event(&mut transaction, event_id).await? {
            if event_matches_row(event, &existing)? {
                transaction.commit().await.map_err(unavailable)?;
                return Ok(StripeWebhookDisposition::Duplicate);
            }
            return Err(StripeWebhookStoreError::Conflict(format!(
                "event ID {event_id} was already recorded with different evidence"
            )));
        }

        let disposition = match event {
            VerifiedStripeEvent::Ignored(ignored) => {
                insert_ignored(&mut transaction, ignored).await?;
                StripeWebhookDisposition::Ignored
            }
            VerifiedStripeEvent::Paid(payment) => record_payment(&mut transaction, payment).await?,
        };
        transaction.commit().await.map_err(unavailable)?;
        Ok(disposition)
    }
}

async fn load_event(
    transaction: &mut Transaction<'_, Postgres>,
    event_id: &str,
) -> Result<Option<WebhookRow>, StripeWebhookStoreError> {
    sqlx::query_as::<_, WebhookRow>(
        r#"
        SELECT event_type, payload_hash, checkout_session_id, payment_intent_id, reservation_id,
            amount_minor, currency, live_mode, outcome
        FROM stripe_webhook_events WHERE event_id = $1
        "#,
    )
    .bind(event_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)
}

fn event_matches_row(
    event: &VerifiedStripeEvent,
    row: &WebhookRow,
) -> Result<bool, StripeWebhookStoreError> {
    let matches = match event {
        VerifiedStripeEvent::Ignored(ignored) => {
            row.outcome == "ignored"
                && row.event_type == ignored.event_type
                && row.payload_hash.as_slice() == ignored.payload_hash.as_bytes()
                && row.checkout_session_id.is_none()
                && row.payment_intent_id.is_none()
                && row.reservation_id.is_none()
        }
        VerifiedStripeEvent::Paid(payment) => {
            let amount = i64::try_from(payment.amount_minor).map_err(|_| {
                StripeWebhookStoreError::Conflict(
                    "payment amount exceeds PostgreSQL BIGINT".to_owned(),
                )
            })?;
            matches!(
                row.outcome.as_str(),
                "payment_recorded" | "duplicate_payment"
            ) && row.event_type == payment.event_type
                && row.payload_hash.as_slice() == payment.payload_hash.as_bytes()
                && row.checkout_session_id.as_deref() == Some(payment.checkout_session_id.as_str())
                && row.payment_intent_id.as_deref() == Some(payment.payment_intent_id.as_str())
                && row.reservation_id == Some(payment.reservation_id)
                && row.amount_minor == Some(amount)
                && row.currency.as_deref() == Some(payment.currency.as_str())
                && row.live_mode == Some(payment.live_mode)
        }
    };
    Ok(matches)
}

async fn insert_ignored(
    transaction: &mut Transaction<'_, Postgres>,
    event: &VerifiedIgnoredEvent,
) -> Result<(), StripeWebhookStoreError> {
    sqlx::query(
        "INSERT INTO stripe_webhook_events (event_id,event_type,payload_hash,outcome) VALUES ($1,$2,$3,'ignored')",
    )
    .bind(&event.event_id)
    .bind(&event.event_type)
    .bind(event.payload_hash.as_bytes().as_slice())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    Ok(())
}

async fn record_payment(
    transaction: &mut Transaction<'_, Postgres>,
    payment: &VerifiedCheckoutPayment,
) -> Result<StripeWebhookDisposition, StripeWebhookStoreError> {
    let amount = i64::try_from(payment.amount_minor).map_err(|_| {
        StripeWebhookStoreError::Conflict("payment amount exceeds PostgreSQL BIGINT".to_owned())
    })?;
    lock_evidence_key(
        transaction,
        &payment.checkout_session_id,
        0x4154_4348_4543_4B4F,
    )
    .await?;
    let reservation = sqlx::query_as::<_, ReservationPaymentRow>(
        "SELECT state,payment_reference FROM supporter_reservations WHERE id=$1 FOR UPDATE",
    )
    .bind(payment.reservation_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    .ok_or(StripeWebhookStoreError::ReservationNotFound(
        payment.reservation_id,
    ))?;

    let prior = sqlx::query_as::<_, PriorPaymentRow>(
        r#"
        SELECT reservation_id,checkout_session_id,payment_intent_id,amount_minor,currency,live_mode
        FROM stripe_webhook_events
        WHERE outcome='payment_recorded'
          AND (reservation_id=$1 OR checkout_session_id=$2 OR payment_intent_id=$3)
        "#,
    )
    .bind(payment.reservation_id)
    .bind(&payment.checkout_session_id)
    .bind(&payment.payment_intent_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?;
    if let Some(prior) = prior {
        if prior.reservation_id != payment.reservation_id
            || prior.checkout_session_id != payment.checkout_session_id
            || prior.payment_intent_id.as_deref() != Some(payment.payment_intent_id.as_str())
            || prior.amount_minor != amount
            || prior.currency != payment.currency
            || prior.live_mode != payment.live_mode
        {
            return Err(StripeWebhookStoreError::Conflict(format!(
                "reservation {} already has different payment evidence",
                payment.reservation_id
            )));
        }
        insert_payment(transaction, payment, amount, "duplicate_payment").await?;
        return Ok(StripeWebhookDisposition::Duplicate);
    }

    if reservation.state != "pending_payment" || reservation.payment_reference.is_some() {
        return Err(StripeWebhookStoreError::Conflict(format!(
            "reservation {} is not awaiting payment",
            payment.reservation_id
        )));
    }
    sqlx::query(
        r#"
        UPDATE supporter_reservations
        SET state='pending_moderation',payment_reference=$2,payment_verified_at=NOW()
        WHERE id=$1
        "#,
    )
    .bind(payment.reservation_id)
    .bind(&payment.event_id)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    insert_payment(transaction, payment, amount, "payment_recorded").await?;
    Ok(StripeWebhookDisposition::PaymentRecorded)
}

async fn lock_evidence_key(
    transaction: &mut Transaction<'_, Postgres>,
    value: &str,
    namespace: i64,
) -> Result<(), StripeWebhookStoreError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,$2))")
        .bind(value)
        .bind(namespace)
        .execute(&mut **transaction)
        .await
        .map_err(unavailable)?;
    Ok(())
}

async fn insert_payment(
    transaction: &mut Transaction<'_, Postgres>,
    payment: &VerifiedCheckoutPayment,
    amount: i64,
    outcome: &str,
) -> Result<(), StripeWebhookStoreError> {
    sqlx::query(
        r#"
        INSERT INTO stripe_webhook_events (
            event_id,event_type,payload_hash,checkout_session_id,reservation_id,
            payment_intent_id,amount_minor,currency,live_mode,outcome
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
        "#,
    )
    .bind(&payment.event_id)
    .bind(&payment.event_type)
    .bind(payment.payload_hash.as_bytes().as_slice())
    .bind(&payment.checkout_session_id)
    .bind(payment.reservation_id)
    .bind(&payment.payment_intent_id)
    .bind(amount)
    .bind(&payment.currency)
    .bind(payment.live_mode)
    .bind(outcome)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    Ok(())
}

fn unavailable(error: sqlx::Error) -> StripeWebhookStoreError {
    StripeWebhookStoreError::Unavailable(error.to_string())
}
