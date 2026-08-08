//! Provider-neutral observer identity and opaque browser-session contracts.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use getrandom::fill;
use thiserror::Error;
use uuid::Uuid;
use world_domain::Digest;

pub const SESSION_SECRET_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityProvider {
    Apple,
    Google,
}

impl IdentityProvider {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Apple => "apple",
            Self::Google => "google",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedExternalIdentity {
    pub provider: IdentityProvider,
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub authenticated_at: DateTime<Utc>,
}

impl VerifiedExternalIdentity {
    pub fn validate(&self) -> Result<(), ObserverAuthError> {
        if self.subject.is_empty()
            || self.subject.len() > 255
            || !self.subject.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(ObserverAuthError::InvalidIdentity);
        }
        if self.email.as_ref().is_some_and(|email| {
            email.is_empty()
                || email.len() > 320
                || email.chars().any(char::is_control)
                || !email.contains('@')
        }) {
            return Err(ObserverAuthError::InvalidIdentity);
        }
        if self.email.is_none() && self.email_verified {
            return Err(ObserverAuthError::InvalidIdentity);
        }
        Ok(())
    }
}

pub struct SessionSecrets {
    session: [u8; SESSION_SECRET_BYTES],
    csrf: [u8; SESSION_SECRET_BYTES],
}

impl std::fmt::Debug for SessionSecrets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionSecrets")
            .field("redacted", &true)
            .finish()
    }
}

impl SessionSecrets {
    pub fn generate() -> Result<Self, ObserverAuthError> {
        let mut session = [0_u8; SESSION_SECRET_BYTES];
        let mut csrf = [0_u8; SESSION_SECRET_BYTES];
        fill(&mut session).map_err(|_| ObserverAuthError::EntropyUnavailable)?;
        fill(&mut csrf).map_err(|_| ObserverAuthError::EntropyUnavailable)?;
        Ok(Self { session, csrf })
    }

    #[must_use]
    pub fn session_token(&self) -> String {
        hex::encode(self.session)
    }

    #[must_use]
    pub fn csrf_token(&self) -> String {
        hex::encode(self.csrf)
    }

    #[must_use]
    pub fn session_digest(&self) -> Digest {
        Digest::sha256(&self.session)
    }

    #[must_use]
    pub fn csrf_digest(&self) -> Digest {
        Digest::sha256(&self.csrf)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewObserverSession {
    pub session_digest: Digest,
    pub csrf_digest: Digest,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl NewObserverSession {
    pub fn validate(&self) -> Result<(), ObserverAuthError> {
        if self.session_digest == Digest::ZERO
            || self.csrf_digest == Digest::ZERO
            || self.session_digest == self.csrf_digest
            || self.expires_at <= self.created_at
        {
            return Err(ObserverAuthError::InvalidSession);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObserverSession {
    pub account_id: Uuid,
    pub provider: IdentityProvider,
    pub subject: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait ObserverSessionStore: Send + Sync {
    async fn admit_verified_identity(
        &self,
        identity: &VerifiedExternalIdentity,
        session: &NewObserverSession,
    ) -> Result<ObserverSession, ObserverAuthStoreError>;

    async fn authenticate_session(
        &self,
        session_digest: Digest,
        now: DateTime<Utc>,
    ) -> Result<Option<ObserverSession>, ObserverAuthStoreError>;

    async fn revoke_session(&self, session_digest: Digest) -> Result<bool, ObserverAuthStoreError>;
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ObserverAuthError {
    #[error("verified external identity is invalid")]
    InvalidIdentity,
    #[error("observer session is invalid")]
    InvalidSession,
    #[error("operating-system entropy is unavailable")]
    EntropyUnavailable,
}

#[derive(Debug, Error)]
pub enum ObserverAuthStoreError {
    #[error(transparent)]
    Validation(#[from] ObserverAuthError),
    #[error("observer authentication conflicts with durable identity: {0}")]
    Conflict(String),
    #[error("observer authentication storage is unavailable: {0}")]
    Unavailable(String),
    #[error("observer authentication storage is corrupt: {0}")]
    Corrupt(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_independent_hashed_and_redacted() {
        let secrets = SessionSecrets::generate().expect("OS entropy");
        assert_eq!(secrets.session_token().len(), 64);
        assert_eq!(secrets.csrf_token().len(), 64);
        assert_ne!(secrets.session_digest(), secrets.csrf_digest());
        let debug = format!("{secrets:?}");
        assert!(!debug.contains(&secrets.session_token()));
        assert!(!debug.contains(&secrets.csrf_token()));
    }

    #[test]
    fn identity_uses_provider_subject_and_treats_email_as_optional_metadata() {
        let identity = VerifiedExternalIdentity {
            provider: IdentityProvider::Google,
            subject: "10769150350006150715113082367".to_owned(),
            email: None,
            email_verified: false,
            authenticated_at: Utc::now(),
        };
        identity.validate().expect("subject-only identity");
        let mut invalid = identity;
        invalid.email_verified = true;
        assert_eq!(invalid.validate(), Err(ObserverAuthError::InvalidIdentity));
    }
}
