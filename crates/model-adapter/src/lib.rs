//! Strict OpenAI-compatible adapter for bounded, replay-recorded cognition.

use std::{collections::BTreeMap, fmt, sync::Arc, time::Duration};

use application::{
    COGNITION_MODEL_CONTRACT_VERSION, CognitionBillingClass, CognitionModel, CognitionModelError,
    CognitionModelRoute, CognitionProviderId, CognitionRouteAttempt, CognitionRouteAttemptStatus,
    CognitionRoutePurpose, CognitionRouteRegistry, ModelCognitionLadderResult,
    ModelCognitionReceipt, ModelCognitionRequest, ModelTokenUsage,
};
use async_trait::async_trait;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use world_domain::{Digest, PrimitiveActionKind};

pub const MODEL_ADAPTER_VERSION: &str = "openai-compatible-bounded-cognition-v4";
pub const MAX_NETWORK_ATTEMPTS_PER_COGNITION_JOB: u16 = 16;
const MAX_ERROR_BODY_BYTES: usize = 2_048;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CognitionRouteKey {
    pub provider: CognitionProviderId,
    pub requested_model: String,
}

impl From<&CognitionModelRoute> for CognitionRouteKey {
    fn from(route: &CognitionModelRoute) -> Self {
        Self {
            provider: route.provider.clone(),
            requested_model: route.requested_model.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CognitionRouteAvailability {
    Ready,
    Cooldown,
    QuotaExhausted,
    Disabled,
}

#[derive(Clone, Debug)]
pub struct CognitionLadderExecution {
    pub max_network_attempts: u16,
    pub paid_authorized: bool,
    pub availability: BTreeMap<CognitionRouteKey, CognitionRouteAvailability>,
}

impl CognitionLadderExecution {
    #[must_use]
    pub fn free_only(max_network_attempts: u16) -> Self {
        Self {
            max_network_attempts,
            paid_authorized: false,
            availability: BTreeMap::new(),
        }
    }

    fn validate(&self) -> Result<(), CognitionModelError> {
        if self.max_network_attempts == 0
            || self.max_network_attempts > MAX_NETWORK_ATTEMPTS_PER_COGNITION_JOB
        {
            return Err(CognitionModelError::Rejected(format!(
                "route ladder network-attempt limit must be between 1 and {MAX_NETWORK_ATTEMPTS_PER_COGNITION_JOB}"
            )));
        }
        Ok(())
    }
}

pub struct CognitionRouteLadder {
    registry: CognitionRouteRegistry,
    purpose: CognitionRoutePurpose,
    adapters: BTreeMap<CognitionProviderId, Arc<dyn CognitionModel>>,
}

impl fmt::Debug for CognitionRouteLadder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CognitionRouteLadder")
            .field("registry", &self.registry)
            .field("purpose", &self.purpose)
            .field(
                "configured_providers",
                &self.adapters.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl CognitionRouteLadder {
    pub fn new(
        registry: CognitionRouteRegistry,
        purpose: CognitionRoutePurpose,
        adapters: BTreeMap<CognitionProviderId, Arc<dyn CognitionModel>>,
    ) -> Result<Self, ModelAdapterConfigError> {
        registry
            .validate(purpose)
            .map_err(|error| ModelAdapterConfigError::RouteRegistry(error.to_string()))?;
        Ok(Self {
            registry,
            purpose,
            adapters,
        })
    }

    pub async fn infer(
        &self,
        request: &ModelCognitionRequest,
        execution: &CognitionLadderExecution,
    ) -> Result<ModelCognitionLadderResult, CognitionModelError> {
        request
            .validate()
            .map_err(|error| CognitionModelError::Rejected(error.to_string()))?;
        execution.validate()?;
        let registry_hash = self
            .registry
            .canonical_hash(self.purpose)
            .map_err(|error| CognitionModelError::Rejected(error.to_string()))?;
        let mut attempts = Vec::new();
        let mut receipt = None;
        let mut network_attempts = 0_u16;

        for (index, route) in self.registry.routes.iter().enumerate() {
            let route_index = u16::try_from(index)
                .map_err(|_| CognitionModelError::Rejected("route index exceeds u16".to_owned()))?;
            let status = if route.billing_class == CognitionBillingClass::PaidApproved
                && !execution.paid_authorized
            {
                CognitionRouteAttemptStatus::SkippedPaidUnauthorized
            } else if network_attempts >= execution.max_network_attempts {
                CognitionRouteAttemptStatus::StoppedAttemptLimit
            } else {
                let availability = execution
                    .availability
                    .get(&CognitionRouteKey::from(route))
                    .copied()
                    .unwrap_or(CognitionRouteAvailability::Ready);
                match availability {
                    CognitionRouteAvailability::Cooldown => {
                        CognitionRouteAttemptStatus::SkippedCooldown
                    }
                    CognitionRouteAvailability::QuotaExhausted => {
                        CognitionRouteAttemptStatus::SkippedQuotaExhausted
                    }
                    CognitionRouteAvailability::Disabled => {
                        CognitionRouteAttemptStatus::SkippedDisabled
                    }
                    CognitionRouteAvailability::Ready => match self.adapters.get(&route.provider) {
                        None => CognitionRouteAttemptStatus::SkippedUnconfigured,
                        Some(adapter) => {
                            network_attempts = network_attempts.saturating_add(1);
                            match adapter.infer(route, request).await {
                                Ok(candidate) => {
                                    if candidate.validate_against(route, request).is_ok() {
                                        receipt = Some(candidate);
                                        CognitionRouteAttemptStatus::Succeeded
                                    } else {
                                        CognitionRouteAttemptStatus::InvalidResponse
                                    }
                                }
                                Err(CognitionModelError::Unavailable(_)) => {
                                    CognitionRouteAttemptStatus::Unavailable
                                }
                                Err(CognitionModelError::Rejected(_)) => {
                                    CognitionRouteAttemptStatus::Rejected
                                }
                                Err(CognitionModelError::InvalidResponse(_)) => {
                                    CognitionRouteAttemptStatus::InvalidResponse
                                }
                            }
                        }
                    },
                }
            };

            attempts.push(CognitionRouteAttempt {
                route_index,
                provider: route.provider.clone(),
                requested_model: route.requested_model.clone(),
                billing_class: route.billing_class,
                status,
            });
            if matches!(
                status,
                CognitionRouteAttemptStatus::Succeeded
                    | CognitionRouteAttemptStatus::StoppedAttemptLimit
            ) {
                break;
            }
        }

        let result = ModelCognitionLadderResult {
            contract_version: COGNITION_MODEL_CONTRACT_VERSION,
            request_id: request.request_id,
            route_policy_version: self.registry.policy_version,
            route_registry_hash: registry_hash,
            attempts,
            receipt,
        };
        result
            .validate_against(&self.registry, self.purpose, request)
            .map_err(|error| CognitionModelError::InvalidResponse(error.to_string()))?;
        Ok(result)
    }
}

#[derive(Clone)]
pub struct OpenAiCompatibleCognition {
    client: Client,
    provider: CognitionProviderId,
    endpoint: Url,
    api_key: String,
}

impl fmt::Debug for OpenAiCompatibleCognition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleCognition")
            .field("provider", &self.provider)
            .field("endpoint", &self.endpoint)
            .field("has_api_key", &true)
            .finish()
    }
}

impl OpenAiCompatibleCognition {
    pub fn new(
        provider: CognitionProviderId,
        base_url: &str,
        api_key: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, ModelAdapterConfigError> {
        let mut endpoint = Url::parse(base_url)
            .map_err(|error| ModelAdapterConfigError::BaseUrl(error.to_string()))?;
        if endpoint.cannot_be_a_base() {
            return Err(ModelAdapterConfigError::BaseUrl(
                "URL cannot be used as a base".to_owned(),
            ));
        }
        {
            let mut segments = endpoint.path_segments_mut().map_err(|()| {
                ModelAdapterConfigError::BaseUrl("URL has no path segments".to_owned())
            })?;
            segments.pop_if_empty();
            segments.extend(["chat", "completions"]);
        }
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ModelAdapterConfigError::MissingApiKey);
        }
        let client = Client::builder()
            .timeout(timeout.max(Duration::from_secs(1)))
            .build()
            .map_err(|error| ModelAdapterConfigError::Client(error.to_string()))?;
        Ok(Self {
            client,
            provider,
            endpoint,
            api_key,
        })
    }
}

#[async_trait]
impl CognitionModel for OpenAiCompatibleCognition {
    async fn infer(
        &self,
        route: &CognitionModelRoute,
        request: &ModelCognitionRequest,
    ) -> Result<ModelCognitionReceipt, CognitionModelError> {
        route
            .validate()
            .map_err(|error| CognitionModelError::Rejected(error.to_string()))?;
        request
            .validate()
            .map_err(|error| CognitionModelError::Rejected(error.to_string()))?;
        if route.provider != self.provider {
            return Err(CognitionModelError::Rejected(
                "route provider differs from configured adapter".to_owned(),
            ));
        }

        let payload = api_request(&self.provider, route, request)?;
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(network_error)?;
        if !response.status().is_success() {
            return Err(http_error(response).await);
        }
        let raw = response
            .json::<Value>()
            .await
            .map_err(|error| CognitionModelError::InvalidResponse(error.to_string()))?;
        parse_response(&self.provider, route, request, raw)
    }
}

fn api_request(
    provider: &CognitionProviderId,
    route: &CognitionModelRoute,
    request: &ModelCognitionRequest,
) -> Result<Value, CognitionModelError> {
    let request_json = serde_json::to_string(request)
        .map_err(|error| CognitionModelError::Rejected(error.to_string()))?;
    let mut payload = json!({
        "model": route.requested_model,
        "messages": [
            {
                "role": "system",
                "content": "You are one bounded decision process inside a simple organism. You receive only numeric bodily pressures, direct property readings, bounded action-outcome values, and recalled direct observations. Select exactly one use-neutral primitive action kind to weakly bias. For apply_force only, contact_region may be 0 through 7. For emit_signal only, signal_intensity may be 1 through 8. For move only, movement_direction may be 0 through 3. Every other motor coordinate must be null. These are physical motor coordinates only, never symbols, words, maps, place names, or named uses. Do not infer or describe identities, technologies, language, writing, social roles, goals, or uses. Return only the required JSON object."
            },
            {
                "role": "user",
                "content": request_json
            }
        ],
        "max_tokens": request.max_output_tokens,
        "temperature": 0,
        "seed": request_seed(request),
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "bounded_primitive_action",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": {
                        "action_kind": {
                            "type": "string",
                            "enum": [
                                "move", "orient", "reach", "grasp", "release",
                                "apply_force", "bite", "chew", "swallow", "rest",
                                "emit_signal"
                            ]
                        },
                        "contact_region": {
                            "anyOf": [
                                {"type": "integer", "minimum": 0, "maximum": 7},
                                {"type": "null"}
                            ]
                        },
                        "signal_intensity": {
                            "anyOf": [
                                {"type": "integer", "minimum": 1, "maximum": 8},
                                {"type": "null"}
                            ]
                        },
                        "movement_direction": {
                            "anyOf": [
                                {"type": "integer", "minimum": 0, "maximum": 3},
                                {"type": "null"}
                            ]
                        }
                    },
                    "required": ["action_kind", "contact_region", "signal_intensity", "movement_direction"],
                    "additionalProperties": false
                }
            }
        }
    });
    if provider.as_str() == "openrouter" {
        payload["provider"] = json!({
            "require_parameters": true,
            "allow_fallbacks": true
        });
        payload["include_reasoning"] = Value::Bool(false);
    }
    Ok(payload)
}

fn request_seed(request: &ModelCognitionRequest) -> u64 {
    let bytes = request.request_id.as_bytes();
    let first: [u8; 8] = bytes[..8]
        .try_into()
        .expect("UUID always contains sixteen bytes");
    u64::from_be_bytes(first).max(1)
}

#[derive(Deserialize)]
struct ChatCompletion {
    id: String,
    model: String,
    choices: Vec<Choice>,
    usage: Usage,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: u64,
    completion_tokens: u64,
    #[serde(default)]
    cost: Option<serde_json::Number>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundedAction {
    action_kind: PrimitiveActionKind,
    #[serde(default)]
    contact_region: Option<u8>,
    #[serde(default)]
    signal_intensity: Option<u8>,
    #[serde(default)]
    movement_direction: Option<u8>,
}

fn parse_response(
    provider: &CognitionProviderId,
    route: &CognitionModelRoute,
    request: &ModelCognitionRequest,
    raw: Value,
) -> Result<ModelCognitionReceipt, CognitionModelError> {
    let response_hash = Digest::canonical(&raw)
        .map_err(|error| CognitionModelError::InvalidResponse(error.to_string()))?;
    let parsed: ChatCompletion = serde_json::from_value(raw)
        .map_err(|error| CognitionModelError::InvalidResponse(error.to_string()))?;
    if parsed.id.trim().is_empty() || parsed.model.trim().is_empty() || parsed.choices.len() != 1 {
        return Err(CognitionModelError::InvalidResponse(
            "completion omitted a unique response, model, or choice".to_owned(),
        ));
    }
    let content = parsed.choices[0]
        .message
        .content
        .as_deref()
        .ok_or_else(|| {
            CognitionModelError::InvalidResponse("completion omitted message content".to_owned())
        })?;
    let action: BoundedAction = serde_json::from_str(content)
        .map_err(|error| CognitionModelError::InvalidResponse(error.to_string()))?;
    let prompt_tokens = u32::try_from(parsed.usage.prompt_tokens).map_err(|_| {
        CognitionModelError::InvalidResponse("prompt token count exceeds u32".to_owned())
    })?;
    let completion_tokens = u32::try_from(parsed.usage.completion_tokens).map_err(|_| {
        CognitionModelError::InvalidResponse("completion token count exceeds u32".to_owned())
    })?;
    let billed_micro_usd = match (route.billing_class, parsed.usage.cost) {
        (CognitionBillingClass::PaidApproved, None) => {
            return Err(CognitionModelError::InvalidResponse(
                "paid completion omitted an explicit provider-reported cost".to_owned(),
            ));
        }
        (_, Some(cost)) => decimal_dollars_to_micro_usd(&cost.to_string())?,
        (_, None) => 0,
    };
    if route.billing_class != CognitionBillingClass::PaidApproved && billed_micro_usd != 0 {
        return Err(CognitionModelError::InvalidResponse(
            "OpenRouter free route reported a non-zero cost".to_owned(),
        ));
    }

    let receipt = ModelCognitionReceipt {
        contract_version: COGNITION_MODEL_CONTRACT_VERSION,
        request_id: request.request_id,
        request_hash: request
            .canonical_hash()
            .map_err(|error| CognitionModelError::Rejected(error.to_string()))?,
        provider: provider.clone(),
        requested_model: route.requested_model.clone(),
        resolved_model: parsed.model,
        provider_response_id: parsed.id,
        usage: ModelTokenUsage {
            prompt_tokens,
            completion_tokens,
        },
        billed_micro_usd,
        action_kind: action.action_kind,
        contact_region: action.contact_region,
        signal_intensity: action.signal_intensity,
        movement_direction: action.movement_direction,
        provider_response_hash: response_hash,
        adapter_version: MODEL_ADAPTER_VERSION.to_owned(),
    };
    receipt
        .validate_against(route, request)
        .map_err(|error| CognitionModelError::InvalidResponse(error.to_string()))?;
    Ok(receipt)
}

fn decimal_dollars_to_micro_usd(value: &str) -> Result<u64, CognitionModelError> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') || value.contains(['e', 'E']) {
        return Err(CognitionModelError::InvalidResponse(
            "cost is not a non-negative fixed decimal".to_owned(),
        ));
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(CognitionModelError::InvalidResponse(
            "cost is not a non-negative fixed decimal".to_owned(),
        ));
    }
    let whole = whole
        .parse::<u64>()
        .map_err(|_| CognitionModelError::InvalidResponse("cost exceeds u64".to_owned()))?;
    let mut micros = whole
        .checked_mul(1_000_000)
        .ok_or_else(|| CognitionModelError::InvalidResponse("cost exceeds u64".to_owned()))?;
    let retained = fraction.chars().take(6).collect::<String>();
    let retained_value = if retained.is_empty() {
        0
    } else {
        retained
            .parse::<u64>()
            .map_err(|_| CognitionModelError::InvalidResponse("invalid cost fraction".to_owned()))?
            * 10_u64.pow(u32::try_from(6 - retained.len()).expect("length is at most six"))
    };
    micros = micros
        .checked_add(retained_value)
        .ok_or_else(|| CognitionModelError::InvalidResponse("cost exceeds u64".to_owned()))?;
    if fraction.len() > 6 && fraction.as_bytes()[6..].iter().any(|byte| *byte != b'0') {
        micros = micros
            .checked_add(1)
            .ok_or_else(|| CognitionModelError::InvalidResponse("cost exceeds u64".to_owned()))?;
    }
    Ok(micros)
}

async fn http_error(response: reqwest::Response) -> CognitionModelError {
    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "response body unavailable".to_owned());
    let body = body.chars().take(MAX_ERROR_BODY_BYTES).collect::<String>();
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        CognitionModelError::Unavailable(format!("model endpoint returned {status}: {body}"))
    } else {
        CognitionModelError::Rejected(format!("model endpoint returned {status}: {body}"))
    }
}

fn network_error(error: reqwest::Error) -> CognitionModelError {
    CognitionModelError::Unavailable(error.to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum ModelAdapterConfigError {
    #[error("invalid model API base URL: {0}")]
    BaseUrl(String),
    #[error("model API key is empty")]
    MissingApiKey,
    #[error("could not construct model API client: {0}")]
    Client(String),
    #[error("invalid cognition route registry: {0}")]
    RouteRegistry(String),
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{Json, Router, extract::State, routing::post};
    use serde_json::json;
    use tokio::net::TcpListener;
    use uuid::Uuid;
    use world_domain::{BodilyNeedState, EntityId, SimTick, WorldId};

    use super::*;

    #[derive(Clone, Copy)]
    enum FakeBehavior {
        Unavailable,
        Succeed(PrimitiveActionKind),
    }

    struct FakeModel {
        behavior: FakeBehavior,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl CognitionModel for FakeModel {
        async fn infer(
            &self,
            route: &CognitionModelRoute,
            request: &ModelCognitionRequest,
        ) -> Result<ModelCognitionReceipt, CognitionModelError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.behavior {
                FakeBehavior::Unavailable => {
                    Err(CognitionModelError::Unavailable("test outage".to_owned()))
                }
                FakeBehavior::Succeed(action_kind) => Ok(ModelCognitionReceipt {
                    contract_version: COGNITION_MODEL_CONTRACT_VERSION,
                    request_id: request.request_id,
                    request_hash: request
                        .canonical_hash()
                        .map_err(|error| CognitionModelError::Rejected(error.to_string()))?,
                    provider: route.provider.clone(),
                    requested_model: route.requested_model.clone(),
                    resolved_model: route.requested_model.clone(),
                    provider_response_id: "fake-response".to_owned(),
                    usage: ModelTokenUsage {
                        prompt_tokens: 12,
                        completion_tokens: 4,
                    },
                    billed_micro_usd: 0,
                    action_kind,
                    contact_region: None,
                    signal_intensity: None,
                    movement_direction: None,
                    provider_response_hash: Digest::sha256(b"fake-response"),
                    adapter_version: "fake-v1".to_owned(),
                }),
            }
        }
    }

    fn fake_adapter(behavior: FakeBehavior, calls: &Arc<AtomicUsize>) -> Arc<dyn CognitionModel> {
        Arc::new(FakeModel {
            behavior,
            calls: Arc::clone(calls),
        })
    }

    #[derive(Clone)]
    struct TestState {
        response: Value,
        seen: Arc<Mutex<Option<Value>>>,
    }

    async fn completion_handler(
        State(state): State<TestState>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        *state.seen.lock().expect("test lock") = Some(body);
        Json(state.response)
    }

    fn request() -> ModelCognitionRequest {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x1234));
        let agent_id = EntityId::deterministic(world_id, b"model-adapter-test-agent");
        let selected_at_tick = SimTick::new(20);
        let ordinal = 0;
        ModelCognitionRequest {
            contract_version: COGNITION_MODEL_CONTRACT_VERSION,
            request_id: application::cognition_request_id(
                world_id,
                agent_id,
                selected_at_tick,
                ordinal,
            ),
            world_id,
            agent_id,
            ordinal,
            selected_at_tick,
            deadline_tick: SimTick::new(32),
            bodily_needs: BodilyNeedState::default(),
            readings: Vec::new(),
            action_values: Vec::new(),
            recalled_memories: Vec::new(),
            max_output_tokens: 32,
        }
    }

    async fn adapter_for(
        provider: CognitionProviderId,
        response: Value,
    ) -> (OpenAiCompatibleCognition, Arc<Mutex<Option<Value>>>) {
        let seen = Arc::new(Mutex::new(None));
        let app = Router::new()
            .route("/v1/chat/completions", post(completion_handler))
            .with_state(TestState {
                response,
                seen: Arc::clone(&seen),
            });
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let address = listener.local_addr().expect("read test address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test API");
        });
        let adapter = OpenAiCompatibleCognition::new(
            provider,
            &format!("http://{address}/v1"),
            "test-secret",
            Duration::from_secs(2),
        )
        .expect("valid adapter");
        (adapter, seen)
    }

    #[tokio::test]
    async fn openrouter_free_is_typed_cost_free_and_records_resolved_model() {
        let response = json!({
            "id": "generation-1",
            "model": "openai/gpt-oss-120b:free",
            "choices": [{"message": {"content": "{\"action_kind\":\"orient\"}"}}],
            "usage": {"prompt_tokens": 91, "completion_tokens": 7, "cost": 0}
        });
        let (adapter, seen) = adapter_for(CognitionProviderId::openrouter(), response).await;
        let route = CognitionModelRoute::openrouter_free();
        let request = request();
        let receipt = adapter
            .infer(&route, &request)
            .await
            .expect("valid completion");
        assert_eq!(receipt.action_kind, PrimitiveActionKind::Orient);
        assert_eq!(receipt.resolved_model, "openai/gpt-oss-120b:free");
        assert_eq!(receipt.billed_micro_usd, 0);
        let seen = seen.lock().expect("test lock").clone().expect("request");
        assert_eq!(seen["provider"]["require_parameters"], true);
        assert_eq!(seen["response_format"]["json_schema"]["strict"], true);
        assert_eq!(seen["include_reasoning"], false);
    }

    #[tokio::test]
    async fn apply_force_may_select_one_bounded_motor_region() {
        let response = json!({
            "id": "generation-region",
            "model": "openai/gpt-oss-120b:free",
            "choices": [{"message": {"content": "{\"action_kind\":\"apply_force\",\"contact_region\":3}"}}],
            "usage": {"prompt_tokens": 91, "completion_tokens": 9, "cost": 0}
        });
        let (adapter, _) = adapter_for(CognitionProviderId::openrouter(), response).await;
        let receipt = adapter
            .infer(&CognitionModelRoute::openrouter_free(), &request())
            .await
            .expect("valid bounded region");
        assert_eq!(receipt.action_kind, PrimitiveActionKind::ApplyForce);
        assert_eq!(receipt.contact_region, Some(3));

        let invalid = json!({
            "id": "generation-invalid-region",
            "model": "openai/gpt-oss-120b:free",
            "choices": [{"message": {"content": "{\"action_kind\":\"move\",\"contact_region\":3}"}}],
            "usage": {"prompt_tokens": 91, "completion_tokens": 9, "cost": 0}
        });
        let (adapter, _) = adapter_for(CognitionProviderId::openrouter(), invalid).await;
        assert!(matches!(
            adapter
                .infer(&CognitionModelRoute::openrouter_free(), &request())
                .await,
            Err(CognitionModelError::InvalidResponse(_))
        ));

        let signal = json!({
            "id": "generation-signal",
            "model": "openai/gpt-oss-120b:free",
            "choices": [{"message": {"content": "{\"action_kind\":\"emit_signal\",\"contact_region\":null,\"signal_intensity\":8}"}}],
            "usage": {"prompt_tokens": 91, "completion_tokens": 9, "cost": 0}
        });
        let (adapter, _) = adapter_for(CognitionProviderId::openrouter(), signal).await;
        let receipt = adapter
            .infer(&CognitionModelRoute::openrouter_free(), &request())
            .await
            .expect("valid bounded signal intensity");
        assert_eq!(receipt.action_kind, PrimitiveActionKind::EmitSignal);
        assert_eq!(receipt.signal_intensity, Some(8));

        let movement = json!({
            "id": "generation-movement",
            "model": "openai/gpt-oss-120b:free",
            "choices": [{"message": {"content": "{\"action_kind\":\"move\",\"contact_region\":null,\"signal_intensity\":null,\"movement_direction\":2}"}}],
            "usage": {"prompt_tokens": 91, "completion_tokens": 10, "cost": 0}
        });
        let (adapter, _) = adapter_for(CognitionProviderId::openrouter(), movement).await;
        let receipt = adapter
            .infer(&CognitionModelRoute::openrouter_free(), &request())
            .await
            .expect("valid bounded movement direction");
        assert_eq!(receipt.action_kind, PrimitiveActionKind::Move);
        assert_eq!(receipt.movement_direction, Some(2));
    }

    #[tokio::test]
    async fn cloudflare_route_uses_same_boundary_without_openrouter_fields() {
        let response = json!({
            "id": "cf-generation-1",
            "model": "@cf/openai/gpt-oss-20b",
            "choices": [{"message": {"content": "{\"action_kind\":\"rest\"}"}}],
            "usage": {"prompt_tokens": 55, "completion_tokens": 6}
        });
        let (adapter, seen) =
            adapter_for(CognitionProviderId::cloudflare_workers_ai(), response).await;
        let receipt = adapter
            .infer(&CognitionModelRoute::cloudflare_gpt_oss_20b(), &request())
            .await
            .expect("valid completion");
        assert_eq!(receipt.action_kind, PrimitiveActionKind::Rest);
        let seen = seen.lock().expect("test lock").clone().expect("request");
        assert!(seen.get("provider").is_none());
        assert!(seen.get("include_reasoning").is_none());
    }

    #[tokio::test]
    async fn free_route_rejects_reported_cost_and_extra_output_fields() {
        let charged = json!({
            "id": "generation-2",
            "model": "openai/gpt-oss-20b:free",
            "choices": [{"message": {"content": "{\"action_kind\":\"move\"}"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 4, "cost": 0.000001}
        });
        let (adapter, _) = adapter_for(CognitionProviderId::openrouter(), charged).await;
        assert!(matches!(
            adapter
                .infer(&CognitionModelRoute::openrouter_free(), &request())
                .await,
            Err(CognitionModelError::InvalidResponse(_))
        ));

        let extra = json!({
            "id": "generation-3",
            "model": "openai/gpt-oss-20b:free",
            "choices": [{"message": {"content": "{\"action_kind\":\"move\",\"explanation\":\"walk\"}"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 4, "cost": 0}
        });
        let (adapter, _) = adapter_for(CognitionProviderId::openrouter(), extra).await;
        assert!(matches!(
            adapter
                .infer(&CognitionModelRoute::openrouter_free(), &request())
                .await,
            Err(CognitionModelError::InvalidResponse(_))
        ));
    }

    #[tokio::test]
    async fn paid_adapter_rejects_a_response_without_explicit_cost() {
        let response = json!({
            "id": "paid-generation-1",
            "model": "deepseek/deepseek-v4-flash",
            "choices": [{"message": {"content": "{\"action_kind\":\"rest\"}"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 4}
        });
        let (adapter, _) = adapter_for(CognitionProviderId::openrouter(), response).await;
        assert!(matches!(
            adapter
                .infer(
                    &CognitionModelRoute::openrouter_deepseek_v4_flash(),
                    &request(),
                )
                .await,
            Err(CognitionModelError::InvalidResponse(_))
        ));
    }

    #[test]
    fn monetary_cost_rounds_up_to_one_micro_dollar() {
        assert_eq!(decimal_dollars_to_micro_usd("0").expect("zero"), 0);
        assert_eq!(
            decimal_dollars_to_micro_usd("0.0000001").expect("fraction"),
            1
        );
        assert_eq!(
            decimal_dollars_to_micro_usd("1.2345678").expect("fraction"),
            1_234_568
        );
    }

    #[tokio::test]
    async fn ladder_records_skips_and_failures_before_the_first_success() {
        let cloudflare_calls = Arc::new(AtomicUsize::new(0));
        let cerebras_calls = Arc::new(AtomicUsize::new(0));
        let mut adapters = BTreeMap::new();
        adapters.insert(
            CognitionProviderId::cloudflare_workers_ai(),
            fake_adapter(FakeBehavior::Unavailable, &cloudflare_calls),
        );
        adapters.insert(
            CognitionProviderId::cerebras(),
            fake_adapter(
                FakeBehavior::Succeed(PrimitiveActionKind::Orient),
                &cerebras_calls,
            ),
        );
        let registry = CognitionRouteRegistry::production_default();
        let ladder =
            CognitionRouteLadder::new(registry, CognitionRoutePurpose::ProductionWorld, adapters)
                .expect("valid ladder");
        let mut execution = CognitionLadderExecution::free_only(8);
        execution.availability.insert(
            CognitionRouteKey::from(&CognitionModelRoute::groq_gpt_oss_20b()),
            CognitionRouteAvailability::Cooldown,
        );

        let result = ladder
            .infer(&request(), &execution)
            .await
            .expect("ladder result");
        assert_eq!(
            result
                .attempts
                .iter()
                .map(|attempt| attempt.status)
                .collect::<Vec<_>>(),
            vec![
                CognitionRouteAttemptStatus::Unavailable,
                CognitionRouteAttemptStatus::Unavailable,
                CognitionRouteAttemptStatus::SkippedCooldown,
                CognitionRouteAttemptStatus::SkippedUnconfigured,
                CognitionRouteAttemptStatus::Succeeded,
            ]
        );
        assert_eq!(cloudflare_calls.load(Ordering::SeqCst), 2);
        assert_eq!(cerebras_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            result.receipt.map(|receipt| receipt.action_kind),
            Some(PrimitiveActionKind::Orient)
        );
    }

    #[tokio::test]
    async fn paid_tail_requires_per_job_authorization() {
        let paid_calls = Arc::new(AtomicUsize::new(0));
        let paid_route = CognitionModelRoute::openrouter_deepseek_v4_flash();
        let registry = CognitionRouteRegistry {
            policy_version: application::COGNITION_ROUTE_POLICY_VERSION,
            routes: vec![paid_route],
        };
        let mut adapters = BTreeMap::new();
        adapters.insert(
            CognitionProviderId::openrouter(),
            fake_adapter(
                FakeBehavior::Succeed(PrimitiveActionKind::Rest),
                &paid_calls,
            ),
        );
        let ladder =
            CognitionRouteLadder::new(registry, CognitionRoutePurpose::ProductionWorld, adapters)
                .expect("valid ladder");

        let denied = ladder
            .infer(&request(), &CognitionLadderExecution::free_only(1))
            .await
            .expect("denied result");
        assert_eq!(
            denied.attempts[0].status,
            CognitionRouteAttemptStatus::SkippedPaidUnauthorized
        );
        assert!(denied.receipt.is_none());
        assert_eq!(paid_calls.load(Ordering::SeqCst), 0);

        let allowed = CognitionLadderExecution {
            max_network_attempts: 1,
            paid_authorized: true,
            availability: BTreeMap::new(),
        };
        let succeeded = ladder
            .infer(&request(), &allowed)
            .await
            .expect("authorized result");
        assert_eq!(
            succeeded.attempts[0].status,
            CognitionRouteAttemptStatus::Succeeded
        );
        assert_eq!(paid_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ladder_stops_at_the_network_attempt_cap() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut adapters = BTreeMap::new();
        adapters.insert(
            CognitionProviderId::cloudflare_workers_ai(),
            fake_adapter(FakeBehavior::Unavailable, &calls),
        );
        let ladder = CognitionRouteLadder::new(
            CognitionRouteRegistry::production_default(),
            CognitionRoutePurpose::ProductionWorld,
            adapters,
        )
        .expect("valid ladder");
        let result = ladder
            .infer(&request(), &CognitionLadderExecution::free_only(1))
            .await
            .expect("bounded result");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.attempts.len(), 2);
        assert_eq!(
            result.attempts[1].status,
            CognitionRouteAttemptStatus::StoppedAttemptLimit
        );
    }
}
