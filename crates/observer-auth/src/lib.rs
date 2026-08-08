//! Provider-neutral observer identity and opaque browser-session contracts.

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use getrandom::fill;
use thiserror::Error;
use uuid::Uuid;
use world_domain::Digest;

pub const SESSION_SECRET_BYTES: usize = 32;
pub const OAUTH_SECRET_BYTES: usize = 32;

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

pub struct OAuthAttemptSecrets {
    state: [u8; OAUTH_SECRET_BYTES],
    nonce: [u8; OAUTH_SECRET_BYTES],
    verifier: [u8; OAUTH_SECRET_BYTES],
    browser_binding: [u8; OAUTH_SECRET_BYTES],
}

impl std::fmt::Debug for OAuthAttemptSecrets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OAuthAttemptSecrets")
            .field("redacted", &true)
            .finish()
    }
}

impl OAuthAttemptSecrets {
    pub fn generate() -> Result<Self, ObserverAuthError> {
        let mut state = [0; OAUTH_SECRET_BYTES];
        let mut nonce = [0; OAUTH_SECRET_BYTES];
        let mut verifier = [0; OAUTH_SECRET_BYTES];
        let mut browser_binding = [0; OAUTH_SECRET_BYTES];
        for value in [&mut state, &mut nonce, &mut verifier, &mut browser_binding] {
            fill(value).map_err(|_| ObserverAuthError::EntropyUnavailable)?;
        }
        Ok(Self {
            state,
            nonce,
            verifier,
            browser_binding,
        })
    }

    #[must_use]
    pub fn state(&self) -> String {
        hex::encode(self.state)
    }

    #[must_use]
    pub fn nonce(&self) -> String {
        hex::encode(self.nonce)
    }

    #[must_use]
    pub fn code_verifier(&self) -> String {
        hex::encode(self.verifier)
    }

    #[must_use]
    pub fn browser_binding(&self) -> String {
        hex::encode(self.browser_binding)
    }

    #[must_use]
    pub fn code_challenge(&self) -> String {
        URL_SAFE_NO_PAD.encode(Digest::sha256(self.code_verifier().as_bytes()).as_bytes())
    }

    #[must_use]
    pub fn attempt(
        &self,
        provider: IdentityProvider,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> OAuthAttempt {
        OAuthAttempt {
            provider,
            state_digest: Digest::sha256(self.state().as_bytes()),
            nonce_digest: Digest::sha256(self.nonce().as_bytes()),
            verifier_digest: Digest::sha256(self.code_verifier().as_bytes()),
            browser_binding_digest: Digest::sha256(self.browser_binding().as_bytes()),
            created_at,
            expires_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuthAttempt {
    pub provider: IdentityProvider,
    pub state_digest: Digest,
    pub nonce_digest: Digest,
    pub verifier_digest: Digest,
    pub browser_binding_digest: Digest,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl OAuthAttempt {
    pub fn validate(&self) -> Result<(), ObserverAuthError> {
        let digests = [
            self.state_digest,
            self.nonce_digest,
            self.verifier_digest,
            self.browser_binding_digest,
        ];
        if digests.contains(&Digest::ZERO)
            || digests
                .iter()
                .enumerate()
                .any(|(index, value)| digests.iter().skip(index + 1).any(|other| other == value))
            || self.expires_at <= self.created_at
        {
            return Err(ObserverAuthError::InvalidOAuthAttempt);
        }
        Ok(())
    }
}

#[async_trait]
pub trait OAuthAttemptStore: Send + Sync {
    async fn create_oauth_attempt(
        &self,
        attempt: &OAuthAttempt,
    ) -> Result<(), ObserverAuthStoreError>;

    async fn load_oauth_attempt(
        &self,
        state_digest: Digest,
        browser_binding_digest: Digest,
        now: DateTime<Utc>,
    ) -> Result<Option<OAuthAttempt>, ObserverAuthStoreError>;

    async fn consume_oauth_attempt(
        &self,
        state_digest: Digest,
        now: DateTime<Utc>,
    ) -> Result<bool, ObserverAuthStoreError>;
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
    #[error("OAuth attempt is invalid")]
    InvalidOAuthAttempt,
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

    #[test]
    fn oauth_secrets_are_independent_redacted_and_pkce_bound() {
        let secrets = OAuthAttemptSecrets::generate().expect("OS entropy");
        assert_eq!(secrets.state().len(), 64);
        assert_eq!(secrets.nonce().len(), 64);
        assert_eq!(secrets.code_verifier().len(), 64);
        assert_eq!(secrets.browser_binding().len(), 64);
        assert_eq!(secrets.code_challenge().len(), 43);
        let now = Utc::now();
        secrets
            .attempt(
                IdentityProvider::Google,
                now,
                now + chrono::Duration::minutes(10),
            )
            .validate()
            .expect("valid attempt");
        let debug = format!("{secrets:?}");
        assert!(!debug.contains(&secrets.state()));
        assert!(!debug.contains(&secrets.code_verifier()));
    }
}
