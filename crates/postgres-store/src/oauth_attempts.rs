use async_trait::async_trait;
use chrono::{DateTime, Utc};
use observer_auth::{IdentityProvider, OAuthAttempt, OAuthAttemptStore, ObserverAuthStoreError};
use sqlx::FromRow;
use world_domain::Digest;

use crate::PostgresStore;

#[derive(FromRow)]
struct AttemptRow {
    provider: String,
    state_digest: Vec<u8>,
    nonce_digest: Vec<u8>,
    verifier_digest: Vec<u8>,
    browser_binding_digest: Vec<u8>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[async_trait]
impl OAuthAttemptStore for PostgresStore {
    async fn create_oauth_attempt(
        &self,
        attempt: &OAuthAttempt,
    ) -> Result<(), ObserverAuthStoreError> {
        attempt.validate()?;
        sqlx::query(
            r#"INSERT INTO observer_oauth_attempts
            (state_digest,provider,nonce_digest,verifier_digest,browser_binding_digest,created_at,expires_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
        )
        .bind(attempt.state_digest.as_bytes().as_slice())
        .bind(attempt.provider.as_str())
        .bind(attempt.nonce_digest.as_bytes().as_slice())
        .bind(attempt.verifier_digest.as_bytes().as_slice())
        .bind(attempt.browser_binding_digest.as_bytes().as_slice())
        .bind(attempt.created_at)
        .bind(attempt.expires_at)
        .execute(self.pool())
        .await
        .map_err(unavailable)?;
        Ok(())
    }

    async fn load_oauth_attempt(
        &self,
        state_digest: Digest,
        browser_binding_digest: Digest,
        now: DateTime<Utc>,
    ) -> Result<Option<OAuthAttempt>, ObserverAuthStoreError> {
        let row = sqlx::query_as::<_, AttemptRow>(
            r#"SELECT provider,state_digest,nonce_digest,verifier_digest,browser_binding_digest,
            created_at,expires_at FROM observer_oauth_attempts
            WHERE state_digest=$1 AND browser_binding_digest=$2 AND consumed_at IS NULL
              AND expires_at>$3"#,
        )
        .bind(state_digest.as_bytes().as_slice())
        .bind(browser_binding_digest.as_bytes().as_slice())
        .bind(now)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
        row.map(parse_attempt).transpose()
    }

    async fn consume_oauth_attempt(
        &self,
        state_digest: Digest,
        now: DateTime<Utc>,
    ) -> Result<bool, ObserverAuthStoreError> {
        let result = sqlx::query(
            "UPDATE observer_oauth_attempts SET consumed_at=$2 WHERE state_digest=$1 AND consumed_at IS NULL AND expires_at>$2",
        )
        .bind(state_digest.as_bytes().as_slice())
        .bind(now)
        .execute(self.pool())
        .await
        .map_err(unavailable)?;
        Ok(result.rows_affected() == 1)
    }
}

fn parse_attempt(row: AttemptRow) -> Result<OAuthAttempt, ObserverAuthStoreError> {
    let attempt = OAuthAttempt {
        provider: match row.provider.as_str() {
            "apple" => IdentityProvider::Apple,
            "google" => IdentityProvider::Google,
            other => {
                return Err(ObserverAuthStoreError::Corrupt(format!(
                    "unknown provider {other}"
                )));
            }
        },
        state_digest: parse_digest(row.state_digest, "state")?,
        nonce_digest: parse_digest(row.nonce_digest, "nonce")?,
        verifier_digest: parse_digest(row.verifier_digest, "verifier")?,
        browser_binding_digest: parse_digest(row.browser_binding_digest, "browser binding")?,
        created_at: row.created_at,
        expires_at: row.expires_at,
    };
    attempt.validate()?;
    Ok(attempt)
}

fn parse_digest(value: Vec<u8>, field: &str) -> Result<Digest, ObserverAuthStoreError> {
    let bytes: [u8; 32] = value.try_into().map_err(|_| {
        ObserverAuthStoreError::Corrupt(format!("OAuth {field} digest is not 32 bytes"))
    })?;
    Ok(Digest::from_bytes(bytes))
}

fn unavailable(error: sqlx::Error) -> ObserverAuthStoreError {
    ObserverAuthStoreError::Unavailable(error.to_string())
}
