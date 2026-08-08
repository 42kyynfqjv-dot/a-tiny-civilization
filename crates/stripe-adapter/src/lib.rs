//! Minimal, raw-body Stripe webhook verification for observer-only supporter payments.

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use thiserror::Error;
use uuid::Uuid;
use world_domain::Digest;

const SUPPORTED_EVENT_TYPES: [&str; 2] = [
    "checkout.session.completed",
    "checkout.session.async_payment_succeeded",
];

#[derive(Clone)]
pub struct StripeWebhookVerifier {
    endpoint_secret: Vec<u8>,
    tolerance_seconds: i64,
    expected_live_mode: bool,
    expected_currency: String,
    expected_amount_minor: u64,
}

impl std::fmt::Debug for StripeWebhookVerifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StripeWebhookVerifier")
            .field("has_endpoint_secret", &true)
            .field("tolerance_seconds", &self.tolerance_seconds)
            .field("expected_live_mode", &self.expected_live_mode)
            .field("expected_currency", &self.expected_currency)
            .field("expected_amount_minor", &self.expected_amount_minor)
            .finish()
    }
}

impl StripeWebhookVerifier {
    pub fn new(
        endpoint_secret: impl Into<Vec<u8>>,
        tolerance_seconds: i64,
        expected_live_mode: bool,
        expected_currency: impl Into<String>,
        expected_amount_minor: u64,
    ) -> Result<Self, StripeWebhookError> {
        let endpoint_secret = endpoint_secret.into();
        let expected_currency = expected_currency.into();
        if endpoint_secret.is_empty() {
            return Err(StripeWebhookError::Configuration(
                "endpoint secret cannot be empty".to_owned(),
            ));
        }
        if tolerance_seconds <= 0 {
            return Err(StripeWebhookError::Configuration(
                "signature tolerance must be positive".to_owned(),
            ));
        }
        if expected_amount_minor == 0 {
            return Err(StripeWebhookError::Configuration(
                "expected amount must be positive".to_owned(),
            ));
        }
        if !valid_currency(&expected_currency) {
            return Err(StripeWebhookError::Configuration(
                "expected currency must be a lowercase three-letter code".to_owned(),
            ));
        }
        Ok(Self {
            endpoint_secret,
            tolerance_seconds,
            expected_live_mode,
            expected_currency,
            expected_amount_minor,
        })
    }

    /// Verifies the signature over the exact request bytes before parsing JSON.
    pub fn verify(
        &self,
        signature_header: &str,
        raw_body: &[u8],
        now_unix_seconds: i64,
    ) -> Result<VerifiedStripeEvent, StripeWebhookError> {
        let signature = ParsedSignature::parse(signature_header)?;
        if now_unix_seconds.abs_diff(signature.timestamp) > self.tolerance_seconds as u64 {
            return Err(StripeWebhookError::StaleTimestamp);
        }

        let mut signed_payload = signature.timestamp.to_string().into_bytes();
        signed_payload.push(b'.');
        signed_payload.extend_from_slice(raw_body);
        let verified = signature.v1.iter().any(|candidate| {
            let Ok(bytes) = hex::decode(candidate) else {
                return false;
            };
            let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(&self.endpoint_secret) else {
                return false;
            };
            mac.update(&signed_payload);
            mac.verify_slice(&bytes).is_ok()
        });
        if !verified {
            return Err(StripeWebhookError::InvalidSignature);
        }

        let event: StripeEvent =
            serde_json::from_slice(raw_body).map_err(|_| StripeWebhookError::InvalidPayload)?;
        validate_identifier(&event.id, "event")?;
        if event.livemode != self.expected_live_mode {
            return Err(StripeWebhookError::LiveModeMismatch);
        }
        let payload_hash = Digest::sha256(raw_body);
        if !SUPPORTED_EVENT_TYPES.contains(&event.event_type.as_str()) {
            return Ok(VerifiedStripeEvent::Ignored(VerifiedIgnoredEvent {
                event_id: event.id,
                event_type: event.event_type,
                payload_hash,
            }));
        }

        let object = event.data.object;
        validate_identifier(&object.id, "checkout session")?;
        if object.payment_status != "paid" {
            return Ok(VerifiedStripeEvent::Ignored(VerifiedIgnoredEvent {
                event_id: event.id,
                event_type: event.event_type,
                payload_hash,
            }));
        }
        if object.amount_total != Some(self.expected_amount_minor) {
            return Err(StripeWebhookError::AmountMismatch);
        }
        if object.currency.as_deref() != Some(self.expected_currency.as_str()) {
            return Err(StripeWebhookError::CurrencyMismatch);
        }
        let reservation_id = object
            .metadata
            .reservation_id
            .parse::<Uuid>()
            .map_err(|_| StripeWebhookError::InvalidReservationId)?;
        Ok(VerifiedStripeEvent::Paid(VerifiedCheckoutPayment {
            event_id: event.id,
            event_type: event.event_type,
            checkout_session_id: object.id,
            reservation_id,
            amount_minor: self.expected_amount_minor,
            currency: self.expected_currency.clone(),
            live_mode: event.livemode,
            payload_hash,
        }))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerifiedStripeEvent {
    Paid(VerifiedCheckoutPayment),
    Ignored(VerifiedIgnoredEvent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedCheckoutPayment {
    pub event_id: String,
    pub event_type: String,
    pub checkout_session_id: String,
    pub reservation_id: Uuid,
    pub amount_minor: u64,
    pub currency: String,
    pub live_mode: bool,
    pub payload_hash: Digest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedIgnoredEvent {
    pub event_id: String,
    pub event_type: String,
    pub payload_hash: Digest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StripeWebhookDisposition {
    PaymentRecorded,
    Duplicate,
    Ignored,
}

/// Observer-side durable admission port. Implementations must atomically deduplicate the event and,
/// for a paid event, transition only the pre-existing reservation named in signed metadata.
#[async_trait]
pub trait StripeWebhookStore: Send + Sync {
    async fn record_verified_stripe_event(
        &self,
        event: &VerifiedStripeEvent,
    ) -> Result<StripeWebhookDisposition, StripeWebhookStoreError>;
}

#[derive(Debug, Error)]
pub enum StripeWebhookStoreError {
    #[error("Stripe webhook references an unknown reservation: {0}")]
    ReservationNotFound(Uuid),
    #[error("Stripe webhook conflicts with durable payment evidence: {0}")]
    Conflict(String),
    #[error("Stripe webhook persistence is unavailable: {0}")]
    Unavailable(String),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum StripeWebhookError {
    #[error("invalid Stripe webhook configuration: {0}")]
    Configuration(String),
    #[error("Stripe-Signature header is malformed")]
    MalformedSignatureHeader,
    #[error("Stripe webhook timestamp is outside the accepted tolerance")]
    StaleTimestamp,
    #[error("Stripe webhook signature is invalid")]
    InvalidSignature,
    #[error("Stripe webhook payload is invalid")]
    InvalidPayload,
    #[error("Stripe webhook live mode differs from the configured environment")]
    LiveModeMismatch,
    #[error("Stripe checkout amount differs from the configured product")]
    AmountMismatch,
    #[error("Stripe checkout currency differs from the configured product")]
    CurrencyMismatch,
    #[error("Stripe checkout metadata has no valid reservation ID")]
    InvalidReservationId,
}

struct ParsedSignature<'a> {
    timestamp: i64,
    v1: Vec<&'a str>,
}

impl<'a> ParsedSignature<'a> {
    fn parse(header: &'a str) -> Result<Self, StripeWebhookError> {
        let mut timestamp = None;
        let mut v1 = Vec::new();
        for item in header.split(',') {
            let Some((key, value)) = item.trim().split_once('=') else {
                return Err(StripeWebhookError::MalformedSignatureHeader);
            };
            match key {
                "t" if timestamp.is_none() => {
                    timestamp = value.parse::<i64>().ok();
                }
                "v1" if !value.is_empty() => v1.push(value),
                _ => {}
            }
        }
        let timestamp = timestamp.ok_or(StripeWebhookError::MalformedSignatureHeader)?;
        if timestamp < 0 || v1.is_empty() {
            return Err(StripeWebhookError::MalformedSignatureHeader);
        }
        Ok(Self { timestamp, v1 })
    }
}

#[derive(Deserialize)]
struct StripeEvent {
    id: String,
    #[serde(rename = "type")]
    event_type: String,
    livemode: bool,
    data: StripeEventData,
}

#[derive(Deserialize)]
struct StripeEventData {
    object: CheckoutSession,
}

#[derive(Deserialize)]
struct CheckoutSession {
    id: String,
    payment_status: String,
    amount_total: Option<u64>,
    currency: Option<String>,
    #[serde(default)]
    metadata: CheckoutMetadata,
}

#[derive(Default, Deserialize)]
struct CheckoutMetadata {
    #[serde(default)]
    reservation_id: String,
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn validate_identifier(value: &str, kind: &str) -> Result<(), StripeWebhookError> {
    if value.is_empty()
        || value.len() > 255
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(StripeWebhookError::Configuration(format!(
            "invalid {kind} identifier"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_fixture";
    const NOW: i64 = 1_800_000_000;
    const RESERVATION: &str = "00000000-0000-4000-8000-000000000123";

    fn body(event_type: &str, paid: bool, live: bool) -> Vec<u8> {
        serde_json::json!({
            "id": "evt_fixture_1",
            "type": event_type,
            "livemode": live,
            "data": {"object": {
                "id": "cs_fixture_1",
                "payment_status": if paid { "paid" } else { "unpaid" },
                "amount_total": 500,
                "currency": "usd",
                "metadata": {"reservation_id": RESERVATION}
            }}
        })
        .to_string()
        .into_bytes()
    }

    fn signature(raw: &[u8], timestamp: i64, secret: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC key");
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(raw);
        format!(
            "t={timestamp},v1={}",
            hex::encode(mac.finalize().into_bytes())
        )
    }

    fn verifier() -> StripeWebhookVerifier {
        StripeWebhookVerifier::new(SECRET, 300, false, "usd", 500).expect("valid verifier")
    }

    #[test]
    fn verifies_exact_raw_body_and_extracts_paid_reservation() {
        let raw = body("checkout.session.completed", true, false);
        let verified = verifier()
            .verify(&signature(&raw, NOW, SECRET), &raw, NOW)
            .expect("valid webhook");
        let VerifiedStripeEvent::Paid(payment) = verified else {
            panic!("expected paid event");
        };
        assert_eq!(payment.reservation_id.to_string(), RESERVATION);
        assert_eq!(payment.amount_minor, 500);
        assert_eq!(payment.payload_hash, Digest::sha256(&raw));
    }

    #[test]
    fn accepts_any_valid_v1_signature_during_secret_rotation() {
        let raw = body("checkout.session.completed", true, false);
        let valid = signature(&raw, NOW, SECRET);
        let valid_digest = valid.split_once("v1=").expect("signature").1;
        let header = format!("t={NOW},v1=00,v1={valid_digest},v0=ignored");
        assert!(verifier().verify(&header, &raw, NOW).is_ok());
    }

    #[test]
    fn rejects_mutation_stale_delivery_wrong_mode_and_wrong_product() {
        let raw = body("checkout.session.completed", true, false);
        let sig = signature(&raw, NOW, SECRET);
        let mut altered = raw.clone();
        altered.push(b' ');
        assert_eq!(
            verifier().verify(&sig, &altered, NOW),
            Err(StripeWebhookError::InvalidSignature)
        );
        assert_eq!(
            verifier().verify(&sig, &raw, NOW + 301),
            Err(StripeWebhookError::StaleTimestamp)
        );

        let live = body("checkout.session.completed", true, true);
        assert_eq!(
            verifier().verify(&signature(&live, NOW, SECRET), &live, NOW),
            Err(StripeWebhookError::LiveModeMismatch)
        );

        let mut wrong_amount: serde_json::Value = serde_json::from_slice(&raw).expect("JSON");
        wrong_amount["data"]["object"]["amount_total"] = serde_json::json!(501);
        let wrong_amount = serde_json::to_vec(&wrong_amount).expect("JSON");
        assert_eq!(
            verifier().verify(&signature(&wrong_amount, NOW, SECRET), &wrong_amount, NOW),
            Err(StripeWebhookError::AmountMismatch)
        );
    }

    #[test]
    fn verified_irrelevant_and_unpaid_events_are_acknowledgeable_but_not_entitlements() {
        for raw in [
            body("customer.created", true, false),
            body("checkout.session.completed", false, false),
        ] {
            assert!(matches!(
                verifier().verify(&signature(&raw, NOW, SECRET), &raw, NOW),
                Ok(VerifiedStripeEvent::Ignored(_))
            ));
        }
    }
}
