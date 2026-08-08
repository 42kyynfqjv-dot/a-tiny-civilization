use async_trait::async_trait;
use stripe_adapter::{StripeCheckoutSession, StripeCheckoutSessionStore, StripeCheckoutStoreError};
use url::Url;
use uuid::Uuid;

use crate::PostgresStore;

#[async_trait]
impl StripeCheckoutSessionStore for PostgresStore {
    async fn record_checkout_session(
        &self,
        reservation_id: Uuid,
        session: &StripeCheckoutSession,
    ) -> Result<StripeCheckoutSession, StripeCheckoutStoreError> {
        let inserted = sqlx::query(
            r#"
            INSERT INTO supporter_checkout_sessions (reservation_id,stripe_session_id,checkout_url)
            VALUES ($1,$2,$3) ON CONFLICT (reservation_id) DO NOTHING
            "#,
        )
        .bind(reservation_id)
        .bind(&session.session_id)
        .bind(session.checkout_url.as_str())
        .execute(self.pool())
        .await
        .map_err(unavailable)?;
        let stored = self
            .load_checkout_session(reservation_id)
            .await?
            .ok_or_else(|| {
                StripeCheckoutStoreError::Unavailable("inserted row disappeared".to_owned())
            })?;
        if inserted.rows_affected() == 0 && stored != *session {
            return Err(StripeCheckoutStoreError::Conflict(format!(
                "reservation {reservation_id} already has a different Checkout session"
            )));
        }
        Ok(stored)
    }

    async fn load_checkout_session(
        &self,
        reservation_id: Uuid,
    ) -> Result<Option<StripeCheckoutSession>, StripeCheckoutStoreError> {
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT stripe_session_id,checkout_url FROM supporter_checkout_sessions WHERE reservation_id=$1",
        )
        .bind(reservation_id)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
        row.map(|(session_id, checkout_url)| {
            let checkout_url = Url::parse(&checkout_url)
                .map_err(|error| StripeCheckoutStoreError::Corrupt(error.to_string()))?;
            Ok(StripeCheckoutSession {
                session_id,
                checkout_url,
            })
        })
        .transpose()
    }
}

fn unavailable(error: sqlx::Error) -> StripeCheckoutStoreError {
    StripeCheckoutStoreError::Unavailable(error.to_string())
}
