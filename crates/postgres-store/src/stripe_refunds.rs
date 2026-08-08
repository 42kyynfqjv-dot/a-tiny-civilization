use std::str::FromStr;

use async_trait::async_trait;
use sqlx::{FromRow, Postgres, Transaction};
use stripe_adapter::{
    PreparedStripeRefund, StripeRefundReason, StripeRefundStore, StripeRefundStoreError,
};
use uuid::Uuid;

use crate::PostgresStore;

#[derive(FromRow)]
struct RefundRow {
    reservation_id: Uuid,
    payment_intent_id: String,
    reason: String,
    stripe_refund_id: Option<String>,
}

#[async_trait]
impl StripeRefundStore for PostgresStore {
    async fn prepare_stripe_refund(
        &self,
        reservation_id: Uuid,
        reason: StripeRefundReason,
    ) -> Result<PreparedStripeRefund, StripeRefundStoreError> {
        let mut transaction = self.pool().begin().await.map_err(unavailable)?;
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM supporter_reservations WHERE id=$1 FOR UPDATE")
                .bind(reservation_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(unavailable)?;
        let state = state.ok_or(StripeRefundStoreError::ReservationNotFound(reservation_id))?;
        if !eligible(&state, reason) {
            return Err(StripeRefundStoreError::Ineligible(reservation_id));
        }

        if let Some(existing) = load_refund(&mut transaction, reservation_id).await? {
            let parsed = parse_refund(existing)?;
            if parsed.reason != reason {
                return Err(StripeRefundStoreError::Conflict(format!(
                    "reservation {reservation_id} already has a different refund reason"
                )));
            }
            transaction.commit().await.map_err(unavailable)?;
            return Ok(parsed);
        }

        let payment_intent_id = sqlx::query_as::<_, (Option<String>,)>(
            r#"
            SELECT payment_intent_id
            FROM stripe_webhook_events
            WHERE reservation_id=$1 AND outcome='payment_recorded'
            "#,
        )
        .bind(reservation_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(unavailable)?
        .and_then(|row| row.0);
        let payment_intent_id = payment_intent_id.ok_or(
            StripeRefundStoreError::MissingPaymentEvidence(reservation_id),
        )?;

        let row = sqlx::query_as::<_, RefundRow>(
            r#"
            INSERT INTO supporter_refunds (reservation_id,payment_intent_id,reason)
            VALUES ($1,$2,$3)
            RETURNING reservation_id,payment_intent_id,reason,stripe_refund_id
            "#,
        )
        .bind(reservation_id)
        .bind(&payment_intent_id)
        .bind(reason.as_str())
        .fetch_one(&mut *transaction)
        .await
        .map_err(unavailable)?;
        transaction.commit().await.map_err(unavailable)?;
        parse_refund(row)
    }

    async fn complete_stripe_refund(
        &self,
        reservation_id: Uuid,
        stripe_refund_id: &str,
    ) -> Result<PreparedStripeRefund, StripeRefundStoreError> {
        if !valid_refund_id(stripe_refund_id) {
            return Err(StripeRefundStoreError::Corrupt(
                "invalid Stripe refund ID".to_owned(),
            ));
        }
        let mut transaction = self.pool().begin().await.map_err(unavailable)?;
        let row = load_refund(&mut transaction, reservation_id).await?.ok_or(
            StripeRefundStoreError::Conflict(format!(
                "reservation {reservation_id} has no prepared refund"
            )),
        )?;
        if let Some(existing) = row.stripe_refund_id.as_deref() {
            if existing != stripe_refund_id {
                return Err(StripeRefundStoreError::Conflict(format!(
                    "reservation {reservation_id} already has different Stripe refund evidence"
                )));
            }
            let parsed = parse_refund(row)?;
            transaction.commit().await.map_err(unavailable)?;
            return Ok(parsed);
        }
        let row = sqlx::query_as::<_, RefundRow>(
            r#"
            UPDATE supporter_refunds
            SET stripe_refund_id=$2,completed_at=NOW()
            WHERE reservation_id=$1 AND stripe_refund_id IS NULL
            RETURNING reservation_id,payment_intent_id,reason,stripe_refund_id
            "#,
        )
        .bind(reservation_id)
        .bind(stripe_refund_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(unavailable)?;
        transaction.commit().await.map_err(unavailable)?;
        parse_refund(row)
    }
}

async fn load_refund(
    transaction: &mut Transaction<'_, Postgres>,
    reservation_id: Uuid,
) -> Result<Option<RefundRow>, StripeRefundStoreError> {
    sqlx::query_as::<_, RefundRow>(
        r#"
        SELECT reservation_id,payment_intent_id,reason,stripe_refund_id
        FROM supporter_refunds WHERE reservation_id=$1 FOR UPDATE
        "#,
    )
    .bind(reservation_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)
}

fn parse_refund(row: RefundRow) -> Result<PreparedStripeRefund, StripeRefundStoreError> {
    let reason = StripeRefundReason::from_str(&row.reason)
        .map_err(|error| StripeRefundStoreError::Corrupt(error.to_string()))?;
    if !row.payment_intent_id.starts_with("pi_")
        || row.payment_intent_id.len() <= 3
        || row.payment_intent_id.len() > 255
        || !row
            .payment_intent_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        || row
            .stripe_refund_id
            .as_deref()
            .is_some_and(|value| !valid_refund_id(value))
    {
        return Err(StripeRefundStoreError::Corrupt(
            "invalid Stripe identifiers in refund evidence".to_owned(),
        ));
    }
    Ok(PreparedStripeRefund {
        reservation_id: row.reservation_id,
        payment_intent_id: row.payment_intent_id,
        reason,
        stripe_refund_id: row.stripe_refund_id,
    })
}

fn eligible(state: &str, reason: StripeRefundReason) -> bool {
    match reason {
        StripeRefundReason::ModerationRejection => state == "rejected",
        StripeRefundReason::WorldExtinction => state == "expired",
        StripeRefundReason::SupporterCancellation => state == "cancelled_by_supporter",
        StripeRefundReason::DuplicateCharge | StripeRefundReason::ServiceFailure => {
            matches!(state, "rejected" | "cancelled_by_supporter" | "expired")
        }
    }
}

fn valid_refund_id(value: &str) -> bool {
    value.starts_with("re_")
        && value.len() > 3
        && value.len() <= 255
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn unavailable(error: sqlx::Error) -> StripeRefundStoreError {
    StripeRefundStoreError::Unavailable(error.to_string())
}
