//! Strict Google OpenID Connect authorization-code adapter.

use std::{fmt, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use observer_auth::{
    IdentityProvider, OAuthAttempt, OAuthAttemptSecrets, VerifiedExternalIdentity,
};
use serde::Deserialize;
use thiserror::Error;
use url::Url;
use world_domain::Digest;

const MAX_PROVIDER_BODY_BYTES: usize = 1_048_576;

#[derive(Clone)]
pub struct GoogleOidcClient {
    client: reqwest::Client,
    authorization_endpoint: Url,
    token_endpoint: Url,
    jwks_uri: Url,
    client_id: String,
    client_secret: String,
    redirect_uri: Url,
}

impl fmt::Debug for GoogleOidcClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleOidcClient")
            .field("authorization_endpoint", &self.authorization_endpoint)
            .field("token_endpoint", &self.token_endpoint)
            .field("jwks_uri", &self.jwks_uri)
            .field("client_id", &self.client_id)
            .field("has_client_secret", &true)
            .field("redirect_uri", &self.redirect_uri)
            .finish()
    }
}

impl GoogleOidcClient {
    pub fn new(
        authorization_endpoint: &str,
        token_endpoint: &str,
        jwks_uri: &str,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: &str,
        timeout: Duration,
    ) -> Result<Self, OidcError> {
        let authorization_endpoint = secure_url(authorization_endpoint)?;
        let token_endpoint = secure_url(token_endpoint)?;
        let jwks_uri = secure_url(jwks_uri)?;
        let redirect_uri = secure_url(redirect_uri)?;
        let client_id = client_id.into();
        let client_secret = client_secret.into();
        if client_id.is_empty() || client_id.len() > 255 || client_secret.is_empty() {
            return Err(OidcError::Configuration(
                "Google client ID and secret are required".to_owned(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(timeout.max(Duration::from_secs(1)))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| OidcError::Configuration(error.to_string()))?;
        Ok(Self {
            client,
            authorization_endpoint,
            token_endpoint,
            jwks_uri,
            client_id,
            client_secret,
            redirect_uri,
        })
    }

    pub fn authorization_url(&self, secrets: &OAuthAttemptSecrets) -> Url {
        let mut url = self.authorization_endpoint.clone();
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("response_type", "code")
            .append_pair("scope", "openid email")
            .append_pair("redirect_uri", self.redirect_uri.as_str())
            .append_pair("state", &secrets.state())
            .append_pair("nonce", &secrets.nonce())
            .append_pair("code_challenge", &secrets.code_challenge())
            .append_pair("code_challenge_method", "S256");
        url
    }

    pub async fn complete(
        &self,
        code: &str,
        code_verifier: &str,
        attempt: &OAuthAttempt,
        now: DateTime<Utc>,
    ) -> Result<VerifiedExternalIdentity, OidcError> {
        if attempt.provider != IdentityProvider::Google
            || Digest::sha256(code_verifier.as_bytes()) != attempt.verifier_digest
            || code.is_empty()
            || code.len() > 4096
        {
            return Err(OidcError::AttemptMismatch);
        }
        // Finish the non-Send form serializer before crossing the network await;
        // Axum requires callback futures to remain Send.
        let request_body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("code", code)
            .append_pair("client_id", &self.client_id)
            .append_pair("client_secret", &self.client_secret)
            .append_pair("redirect_uri", self.redirect_uri.as_str())
            .append_pair("grant_type", "authorization_code")
            .append_pair("code_verifier", code_verifier)
            .finish();
        let token = self
            .client
            .post(self.token_endpoint.clone())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(request_body)
            .send()
            .await
            .map_err(network)?;
        let token: TokenResponse = checked_json(token).await?;
        if !token.token_type.eq_ignore_ascii_case("bearer") || token.id_token.len() > 32_768 {
            return Err(OidcError::InvalidTokenResponse);
        }
        let jwks = self
            .client
            .get(self.jwks_uri.clone())
            .send()
            .await
            .map_err(network)?;
        let jwks: JwkSet = checked_json(jwks).await?;
        self.verify_id_token(&token.id_token, &jwks, attempt, now)
    }

    fn verify_id_token(
        &self,
        token: &str,
        jwks: &JwkSet,
        attempt: &OAuthAttempt,
        now: DateTime<Utc>,
    ) -> Result<VerifiedExternalIdentity, OidcError> {
        let header = decode_header(token).map_err(|_| OidcError::InvalidIdToken)?;
        if header.alg != Algorithm::RS256 {
            return Err(OidcError::InvalidIdToken);
        }
        let kid = header.kid.ok_or(OidcError::InvalidIdToken)?;
        let candidates = jwks
            .keys
            .iter()
            .filter(|key| key.kid == kid && key.kty == "RSA" && key.alg.as_deref() == Some("RS256"))
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err(OidcError::InvalidIdToken);
        }
        let key = DecodingKey::from_rsa_components(&candidates[0].n, &candidates[0].e)
            .map_err(|_| OidcError::InvalidIdToken)?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.client_id]);
        validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);
        validation.set_required_spec_claims(&["exp", "iat", "iss", "aud", "sub"]);
        validation.validate_exp = false;
        validation.leeway = 0;
        let claims = decode::<GoogleClaims>(token, &key, &validation)
            .map_err(|_| OidcError::InvalidIdToken)?
            .claims;
        let now_seconds = now.timestamp();
        if claims.exp <= now_seconds
            || claims.iat > now_seconds.saturating_add(60)
            || claims.iat > claims.exp
            || claims
                .nonce
                .as_deref()
                .map(|nonce| Digest::sha256(nonce.as_bytes()))
                != Some(attempt.nonce_digest)
            || claims
                .azp
                .as_ref()
                .is_some_and(|azp| azp != &self.client_id)
        {
            return Err(OidcError::InvalidIdToken);
        }
        let authenticated_at = Utc
            .timestamp_opt(claims.iat, 0)
            .single()
            .ok_or(OidcError::InvalidIdToken)?;
        let identity = VerifiedExternalIdentity {
            provider: IdentityProvider::Google,
            subject: claims.sub,
            email: claims.email,
            email_verified: claims.email_verified.unwrap_or(false),
            authenticated_at,
        };
        identity.validate().map_err(|_| OidcError::InvalidIdToken)?;
        Ok(identity)
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
    token_type: String,
}

#[derive(Deserialize)]
struct JwkSet {
    keys: Vec<RsaJwk>,
}

#[derive(Deserialize)]
struct RsaJwk {
    kid: String,
    kty: String,
    alg: Option<String>,
    n: String,
    e: String,
}

#[derive(Deserialize)]
struct GoogleClaims {
    sub: String,
    exp: i64,
    iat: i64,
    nonce: Option<String>,
    azp: Option<String>,
    email: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_bool")]
    email_verified: Option<bool>,
}

fn deserialize_optional_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolValue {
        Bool(bool),
        Text(String),
    }
    Option::<BoolValue>::deserialize(deserializer)?.map_or(Ok(None), |value| match value {
        BoolValue::Bool(value) => Ok(Some(value)),
        BoolValue::Text(value) if value == "true" => Ok(Some(true)),
        BoolValue::Text(value) if value == "false" => Ok(Some(false)),
        BoolValue::Text(_) => Err(serde::de::Error::custom("email_verified must be a boolean")),
    })
}

async fn checked_json<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T, OidcError> {
    if !response.status().is_success() {
        return Err(OidcError::ProviderRejected(response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_PROVIDER_BODY_BYTES as u64)
    {
        return Err(OidcError::InvalidTokenResponse);
    }
    let bytes = response.bytes().await.map_err(network)?;
    if bytes.len() > MAX_PROVIDER_BODY_BYTES {
        return Err(OidcError::InvalidTokenResponse);
    }
    serde_json::from_slice(&bytes).map_err(|_| OidcError::InvalidTokenResponse)
}

fn secure_url(value: &str) -> Result<Url, OidcError> {
    let url = Url::parse(value).map_err(|error| OidcError::Configuration(error.to_string()))?;
    let local =
        url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"));
    if (url.scheme() != "https" && !local)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(OidcError::Configuration(
            "OIDC URLs must use HTTPS outside localhost and cannot contain credentials or fragments"
                .to_owned(),
        ));
    }
    Ok(url)
}

fn network(error: reqwest::Error) -> OidcError {
    OidcError::Unavailable(error.to_string())
}

#[derive(Debug, Error)]
pub enum OidcError {
    #[error("invalid OIDC configuration: {0}")]
    Configuration(String),
    #[error("OIDC attempt secrets do not match")]
    AttemptMismatch,
    #[error("OIDC provider is unavailable: {0}")]
    Unavailable(String),
    #[error("OIDC provider rejected the request with HTTP {0}")]
    ProviderRejected(reqwest::StatusCode),
    #[error("OIDC provider returned an invalid token response")]
    InvalidTokenResponse,
    #[error("OIDC ID token verification failed")]
    InvalidIdToken,
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use axum::{
        Json, Router,
        body::Bytes,
        extract::State,
        routing::{get, post},
    };
    use chrono::Duration as ChronoDuration;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde::Serialize;
    use serde_json::{Value, json};
    use tokio::{net::TcpListener, sync::Mutex};

    use super::*;

    const CLIENT_ID: &str = "atiny-google-client.apps.example.test";
    const TEST_KID: &str = "atiny-test-key";
    const TEST_MODULUS: &str = "shK__VNxvUBqngYYhpbgEdCnlB4XKkotzUEbiYgdupM97zJ2PoQHxtChrLfQO_yJFp4Em-QPlUYwdq-XLRblasR2-6X2hkMWV4OseoOMRclT6_m6hAIuKsYF2LEnoJbkQjwHNC7LHIQPe8tGtRi3jKYLLpJ1nvaEGxMk5Dxy2POpR_M3DmzbcuyoyWKNZzZRT8av3WGuC-PptUHzp6QIcLJHs3LLedSvd1UJcrLZZCO5BdTQgKvIZM6MQ-e2f26uG7gY8vqrhzbGxo3EaZP2w1ouYU41l9sMw978gFK4bR6Bg5q3Cpxvrc_X_An3KaZ7aTPg7_bUCzSK0WJb7J9cSQ";
    const TEST_PRIVATE_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCyEr/9U3G9QGqe
BhiGluAR0KeUHhcqSi3NQRuJiB26kz3vMnY+hAfG0KGst9A7/IkWngSb5A+VRjB2
r5ctFuVqxHb7pfaGQxZXg6x6g4xFyVPr+bqEAi4qxgXYsSegluRCPAc0LsschA97
y0a1GLeMpgsuknWe9oQbEyTkPHLY86lH8zcObNty7KjJYo1nNlFPxq/dYa4L4+m1
QfOnpAhwskezcst51K93VQlystlkI7kF1NCAq8hkzoxD57Z/bq4buBjy+quHNsbG
jcRpk/bDWi5hTjWX2wzD3vyAUrhtHoGDmrcKnG+tz9f8CfcppntpM+Dv9tQLNIrR
Ylvsn1xJAgMBAAECggEAATNzLAg2Ciq3DV3JKI8376bSmoMhcN2TNYEs6F6MpMd9
cXGLrpxBWSvrCzkqJF1tp0NlkI+XM1CH3yUMBffTkEbG7qeN5BXrvqdasHDWr1nO
QRcsFNvW597yByYauFCMNY4mFkoCgKy7VgBm9620/zPXe3btaCcNeQlGBGwL8j6f
kToc3nSSTOKXFI9rYmmS8cvXFSOOsBfdCFDuYdEVEsttrwHtKRXAB5r/AjtOVBb/
ahoW1VCLZcDMYpohuARQK2snEcpNd76/9WHLP8t0kR7M/Veod9qzSF9ID9BIDMUj
0VTrLiDw5sgPpzIPROd3AeKbvpMc/TBUDYYvjSqSAQKBgQDhv5dma6aAvPr5q0W5
XmrO7I5LrPbAhRG7p5jOKx0U3gqLm3JkLRXFSJcYcKXgtF/v+hg3/KtjEDqqBIYY
NTlaFCPS/gcstGcZsidl9WRmLB7sg1KSV6Qlq177+CgDtjelc9BvtjvwDd28tx7X
xNAePT1d2Rxfsfxh2XDydYEAKQKBgQDJ76OUEk1yMV+/06/5Mxl+HmwCUaPJ0cu8
bdOaDHmxnfLAeYfV6upJitFHLLpnFO7uopzHMh4Z3SbqtBPSVJ15QLtgDaP/YAGo
Xq3Sh3hVLi+cIxwuiaxl5TR05KNBbTw9F3jla3li04PPP0r/j0WearzO20eYFUVX
Y4iJHdp/IQKBgHq2tC0nrX3jvKYFVUR7r6HF81/tqMBkVYxlgWnpA8j5HlBfcqJD
48a3O/M8IN9yDYicsZeVkPCrvMf454+3NvLhacvi7LF/a2ALeOEysJ3ds/2rMTJ9
06vqaRqc/dturPcSaqafMBvA3d0cyfZOdTdK4NXoFEVssh3ankweVb5pAoGBALGC
0J63QBEjyfGMmmJLQxuUjomzTnF41Mm9GYePc+Jo4B3GN1wadv1S5AjXDrzSr/5i
P8LzEXbW6wDib5IzA4K1HoGfPAyfTpW9NLuejm8CfKOaUYmvSDcCNwySd9hpt8xU
N9gkk74GBRZHoxvny+EoHvUP2W2dNSlOu5UdAxdBAoGAdLTfIRXJk+morkS3ZBkQ
0CvV0AIqyR0SRwV2N19uwYSidRoW9lDMsDi+LZATwhGC9k8rW/nWtXUO9ozlxJMI
heIhZRnKiUQT2FZZLknSOahdNbGzLDFTr5WOqdqBsFpEmlemA33N/W4Ypx/eSoqB
paXmFb3rA4aS2+/7Q60jjnM=
-----END PRIVATE KEY-----"#;

    #[derive(Clone, Serialize)]
    struct TestClaims {
        iss: String,
        aud: String,
        sub: String,
        exp: i64,
        iat: i64,
        nonce: String,
        azp: String,
        email: String,
        email_verified: bool,
    }

    #[derive(Clone)]
    struct ProviderState {
        id_token: String,
        token_requests: Arc<Mutex<Vec<HashMap<String, String>>>>,
    }

    fn client(base: &str) -> GoogleOidcClient {
        GoogleOidcClient::new(
            &format!("{base}/authorize"),
            &format!("{base}/token"),
            &format!("{base}/jwks"),
            CLIENT_ID,
            "test-client-secret",
            "http://localhost/callback",
            Duration::from_secs(5),
        )
        .expect("client")
    }

    fn claims(now: DateTime<Utc>, nonce: String) -> TestClaims {
        TestClaims {
            iss: "https://accounts.google.com".to_owned(),
            aud: CLIENT_ID.to_owned(),
            sub: "google-subject-123".to_owned(),
            exp: (now + ChronoDuration::minutes(5)).timestamp(),
            iat: now.timestamp(),
            nonce,
            azp: CLIENT_ID.to_owned(),
            email: "observer@example.test".to_owned(),
            email_verified: true,
        }
    }

    fn signed(claims: &TestClaims, algorithm: Algorithm) -> String {
        let mut header = Header::new(algorithm);
        header.kid = Some(TEST_KID.to_owned());
        encode(
            &header,
            claims,
            &EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY.as_bytes()).expect("test RSA key"),
        )
        .expect("signed JWT")
    }

    fn jwks() -> JwkSet {
        JwkSet {
            keys: vec![RsaJwk {
                kid: TEST_KID.to_owned(),
                kty: "RSA".to_owned(),
                alg: Some("RS256".to_owned()),
                n: TEST_MODULUS.to_owned(),
                e: "AQAB".to_owned(),
            }],
        }
    }

    async fn token_endpoint(State(state): State<ProviderState>, body: Bytes) -> Json<Value> {
        let fields = url::form_urlencoded::parse(&body)
            .into_owned()
            .collect::<HashMap<_, _>>();
        state.token_requests.lock().await.push(fields);
        Json(json!({"id_token": state.id_token, "token_type": "Bearer"}))
    }

    async fn jwks_endpoint() -> Json<Value> {
        Json(json!({"keys": [{
            "kid": TEST_KID,
            "kty": "RSA",
            "alg": "RS256",
            "n": TEST_MODULUS,
            "e": "AQAB"
        }]}))
    }

    #[test]
    fn authorization_url_binds_state_nonce_pkce_and_exact_redirect() {
        let secrets = OAuthAttemptSecrets::generate().expect("secrets");
        let url = client("http://127.0.0.1:9999").authorization_url(&secrets);
        let parameters = url.query_pairs().into_owned().collect::<HashMap<_, _>>();
        assert_eq!(
            parameters.get("response_type").map(String::as_str),
            Some("code")
        );
        assert_eq!(
            parameters.get("scope").map(String::as_str),
            Some("openid email")
        );
        assert_eq!(parameters.get("state"), Some(&secrets.state()));
        assert_eq!(parameters.get("nonce"), Some(&secrets.nonce()));
        assert_eq!(
            parameters.get("code_challenge"),
            Some(&secrets.code_challenge())
        );
        assert_eq!(
            parameters.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(
            parameters.get("redirect_uri").map(String::as_str),
            Some("http://localhost/callback")
        );
    }

    #[tokio::test]
    async fn authorization_code_exchange_verifies_identity_and_exact_request() {
        let now = Utc::now();
        let secrets = OAuthAttemptSecrets::generate().expect("secrets");
        let attempt = secrets.attempt(
            IdentityProvider::Google,
            now - ChronoDuration::seconds(1),
            now + ChronoDuration::minutes(10),
        );
        let state = ProviderState {
            id_token: signed(&claims(now, secrets.nonce()), Algorithm::RS256),
            token_requests: Arc::new(Mutex::new(Vec::new())),
        };
        let app = Router::new()
            .route("/token", post(token_endpoint))
            .route("/jwks", get(jwks_endpoint))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        tokio::spawn(async move { axum::serve(listener, app).await.expect("provider") });

        let identity = client(&base)
            .complete("single-use-code", &secrets.code_verifier(), &attempt, now)
            .await
            .expect("verified identity");
        assert_eq!(identity.provider, IdentityProvider::Google);
        assert_eq!(identity.subject, "google-subject-123");
        assert_eq!(identity.email.as_deref(), Some("observer@example.test"));
        assert!(identity.email_verified);

        let requests = state.token_requests.lock().await;
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(
            request.get("code").map(String::as_str),
            Some("single-use-code")
        );
        assert_eq!(
            request.get("client_id").map(String::as_str),
            Some(CLIENT_ID)
        );
        assert_eq!(
            request.get("client_secret").map(String::as_str),
            Some("test-client-secret")
        );
        assert_eq!(request.get("code_verifier"), Some(&secrets.code_verifier()));
        assert_eq!(
            request.get("grant_type").map(String::as_str),
            Some("authorization_code")
        );
    }

    #[test]
    fn rejects_wrong_nonce_audience_expiry_and_algorithm() {
        let now = Utc::now();
        let secrets = OAuthAttemptSecrets::generate().expect("secrets");
        let attempt = secrets.attempt(
            IdentityProvider::Google,
            now - ChronoDuration::seconds(1),
            now + ChronoDuration::minutes(10),
        );
        let client = client("http://127.0.0.1:9999");

        let wrong_nonce = signed(&claims(now, "wrong-nonce".to_owned()), Algorithm::RS256);
        assert!(matches!(
            client.verify_id_token(&wrong_nonce, &jwks(), &attempt, now),
            Err(OidcError::InvalidIdToken)
        ));

        let mut wrong_audience = claims(now, secrets.nonce());
        wrong_audience.aud = "attacker-client".to_owned();
        assert!(matches!(
            client.verify_id_token(
                &signed(&wrong_audience, Algorithm::RS256),
                &jwks(),
                &attempt,
                now
            ),
            Err(OidcError::InvalidIdToken)
        ));

        let mut expired = claims(now, secrets.nonce());
        expired.exp = now.timestamp();
        assert!(matches!(
            client.verify_id_token(&signed(&expired, Algorithm::RS256), &jwks(), &attempt, now),
            Err(OidcError::InvalidIdToken)
        ));

        let wrong_algorithm = signed(&claims(now, secrets.nonce()), Algorithm::PS256);
        assert!(matches!(
            client.verify_id_token(&wrong_algorithm, &jwks(), &attempt, now),
            Err(OidcError::InvalidIdToken)
        ));
    }

    #[tokio::test]
    async fn rejects_changed_pkce_verifier_before_contacting_provider() {
        let now = Utc::now();
        let secrets = OAuthAttemptSecrets::generate().expect("secrets");
        let attempt = secrets.attempt(
            IdentityProvider::Google,
            now,
            now + ChronoDuration::minutes(10),
        );
        let error = client("http://127.0.0.1:9")
            .complete("code", "different-verifier", &attempt, now)
            .await
            .expect_err("must reject locally");
        assert!(matches!(error, OidcError::AttemptMismatch));
    }
}
