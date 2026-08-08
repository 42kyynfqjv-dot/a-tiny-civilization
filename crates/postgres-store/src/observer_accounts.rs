use async_trait::async_trait;
use chrono::{DateTime, Utc};
use observer_auth::{
    IdentityProvider, NewObserverSession, ObserverAuthStoreError, ObserverSession,
    ObserverSessionStore, VerifiedExternalIdentity,
};
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;
use world_domain::Digest;

use crate::PostgresStore;

#[derive(FromRow)]
struct SessionRow {
    account_id: Uuid,
    provider: String,
    provider_subject: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[async_trait]
impl ObserverSessionStore for PostgresStore {
    async fn admit_verified_identity(
        &self,
        identity: &VerifiedExternalIdentity,
        session: &NewObserverSession,
    ) -> Result<ObserverSession, ObserverAuthStoreError> {
        identity.validate()?;
        session.validate()?;
        let mut transaction = self.pool().begin().await.map_err(unavailable)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 470521881413))")
            .bind(format!(
                "{}:{}",
                identity.provider.as_str(),
                identity.subject
            ))
            .execute(&mut *transaction)
            .await
            .map_err(unavailable)?;
        let account_id = load_or_create_account(&mut transaction, identity).await?;
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            INSERT INTO observer_sessions (
                session_digest,csrf_digest,account_id,provider,provider_subject,created_at,expires_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7)
            RETURNING account_id,provider,provider_subject,created_at,expires_at
            "#,
        )
        .bind(session.session_digest.as_bytes().as_slice())
        .bind(session.csrf_digest.as_bytes().as_slice())
        .bind(account_id)
        .bind(identity.provider.as_str())
        .bind(&identity.subject)
        .bind(session.created_at)
        .bind(session.expires_at)
        .fetch_one(&mut *transaction)
        .await
        .map_err(unavailable)?;
        transaction.commit().await.map_err(unavailable)?;
        parse_session(row)
    }

    async fn authenticate_session(
        &self,
        session_digest: Digest,
        now: DateTime<Utc>,
    ) -> Result<Option<ObserverSession>, ObserverAuthStoreError> {
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT s.account_id,s.provider,s.provider_subject,s.created_at,s.expires_at
            FROM observer_sessions s JOIN observer_accounts a ON a.id=s.account_id
            WHERE s.session_digest=$1 AND s.revoked_at IS NULL AND s.expires_at>$2
              AND a.disabled_at IS NULL
            "#,
        )
        .bind(session_digest.as_bytes().as_slice())
        .bind(now)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
        row.map(parse_session).transpose()
    }

    async fn revoke_session(&self, session_digest: Digest) -> Result<bool, ObserverAuthStoreError> {
        let result = sqlx::query(
            "UPDATE observer_sessions SET revoked_at=NOW() WHERE session_digest=$1 AND revoked_at IS NULL",
        )
        .bind(session_digest.as_bytes().as_slice())
        .execute(self.pool())
        .await
        .map_err(unavailable)?;
        Ok(result.rows_affected() == 1)
    }
}

async fn load_or_create_account(
    transaction: &mut Transaction<'_, Postgres>,
    identity: &VerifiedExternalIdentity,
) -> Result<Uuid, ObserverAuthStoreError> {
    if let Some(account_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT account_id FROM observer_identities WHERE provider=$1 AND provider_subject=$2",
    )
    .bind(identity.provider.as_str())
    .bind(&identity.subject)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    {
        sqlx::query(
            "UPDATE observer_identities SET email=$3,email_verified=$4,last_authenticated_at=$5 WHERE provider=$1 AND provider_subject=$2",
        )
        .bind(identity.provider.as_str())
        .bind(&identity.subject)
        .bind(&identity.email)
        .bind(identity.email_verified)
        .bind(identity.authenticated_at)
        .execute(&mut **transaction)
        .await
        .map_err(unavailable)?;
        return Ok(account_id);
    }
    let account_id = Uuid::new_v4();
    sqlx::query("INSERT INTO observer_accounts (id,created_at) VALUES ($1,$2)")
        .bind(account_id)
        .bind(identity.authenticated_at)
        .execute(&mut **transaction)
        .await
        .map_err(unavailable)?;
    sqlx::query(
        r#"INSERT INTO observer_identities
        (provider,provider_subject,account_id,email,email_verified,created_at,last_authenticated_at)
        VALUES ($1,$2,$3,$4,$5,$6,$6)"#,
    )
    .bind(identity.provider.as_str())
    .bind(&identity.subject)
    .bind(account_id)
    .bind(&identity.email)
    .bind(identity.email_verified)
    .bind(identity.authenticated_at)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    Ok(account_id)
}

fn parse_session(row: SessionRow) -> Result<ObserverSession, ObserverAuthStoreError> {
    let provider = match row.provider.as_str() {
        "apple" => IdentityProvider::Apple,
        "google" => IdentityProvider::Google,
        other => {
            return Err(ObserverAuthStoreError::Corrupt(format!(
                "unknown provider {other}"
            )));
        }
    };
    Ok(ObserverSession {
        account_id: row.account_id,
        provider,
        subject: row.provider_subject,
        created_at: row.created_at,
        expires_at: row.expires_at,
    })
}

fn unavailable(error: sqlx::Error) -> ObserverAuthStoreError {
    ObserverAuthStoreError::Unavailable(error.to_string())
}
