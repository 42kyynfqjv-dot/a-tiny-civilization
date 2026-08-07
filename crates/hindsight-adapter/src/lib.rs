//! HTTP adapter for the pinned Hindsight subjective-memory service.

use std::{collections::BTreeMap, fmt, time::Duration};

use application::{
    AgentMemory, MEMORY_PAYLOAD_VERSION, MemoryAdapterError, MemoryFactKind, MemoryRecallOutcome,
    MemoryRecallRequest, MemoryRetain, MemoryRetainReceipt, RecallUnavailableReason,
    RecalledMemory,
};
use async_trait::async_trait;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{Digest, EventSequence, SimTick};

pub const HINDSIGHT_ADAPTER_VERSION: &str = "hindsight-http-v1/0.8.6";

#[derive(Clone)]
pub struct HindsightMemory {
    client: Client,
    base_url: Url,
    api_key: Option<String>,
}

impl fmt::Debug for HindsightMemory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HindsightMemory")
            .field("base_url", &self.base_url)
            .field("has_api_key", &self.api_key.is_some())
            .finish_non_exhaustive()
    }
}

impl HindsightMemory {
    pub fn new(
        base_url: &str,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self, HindsightConfigError> {
        let mut base_url = Url::parse(base_url)
            .map_err(|error| HindsightConfigError::BaseUrl(error.to_string()))?;
        if base_url.cannot_be_a_base() {
            return Err(HindsightConfigError::BaseUrl(
                "URL cannot be used as an HTTP base".to_owned(),
            ));
        }
        if !base_url.path().ends_with('/') {
            let mut path = base_url.path().to_owned();
            path.push('/');
            base_url.set_path(&path);
        }
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| HindsightConfigError::Client(error.to_string()))?;
        let api_key = api_key.filter(|key| !key.trim().is_empty());
        Ok(Self {
            client,
            base_url,
            api_key,
        })
    }

    fn endpoint(&self, bank_id: &str, suffix: &[&str]) -> Result<Url, MemoryAdapterError> {
        let mut url = self.base_url.clone();
        let mut segments = url.path_segments_mut().map_err(|()| {
            MemoryAdapterError::InvalidResponse("invalid Hindsight base URL".to_owned())
        })?;
        segments.pop_if_empty();
        segments.extend(["v1", "default", "banks", bank_id]);
        segments.extend(suffix.iter().copied());
        drop(segments);
        Ok(url)
    }

    fn post(&self, url: Url) -> reqwest::RequestBuilder {
        let request = self.client.post(url);
        match &self.api_key {
            Some(api_key) => request.bearer_auth(api_key),
            None => request,
        }
    }
}

#[async_trait]
impl AgentMemory for HindsightMemory {
    async fn retain(
        &self,
        memory: &MemoryRetain,
    ) -> Result<MemoryRetainReceipt, MemoryAdapterError> {
        memory
            .validate()
            .map_err(|error| MemoryAdapterError::Rejected(error.to_string()))?;
        let url = self.endpoint(&memory.bank_id, &["memories"])?;
        let request = RetainRequest::from(memory);
        let response = self
            .post(url)
            .json(&request)
            .send()
            .await
            .map_err(network_error)?;
        let response = require_success(response).await?;
        let response = response
            .json::<RetainResponse>()
            .await
            .map_err(|error| MemoryAdapterError::InvalidResponse(error.to_string()))?;
        let remote_operation_id = response.operation_id.ok_or_else(|| {
            MemoryAdapterError::InvalidResponse(
                "async retain response omitted operation_id".to_owned(),
            )
        })?;
        let parsed_operation_id = Uuid::parse_str(&remote_operation_id).map_err(|error| {
            MemoryAdapterError::InvalidResponse(format!(
                "retain operation_id is not a UUID: {error}"
            ))
        })?;
        if !response.success
            || response.bank_id != memory.bank_id
            || response.items_count != 1
            || !response.async_processing
            || parsed_operation_id != memory.operation_id
        {
            return Err(MemoryAdapterError::InvalidResponse(
                "retain acknowledgement differs from the submitted operation".to_owned(),
            ));
        }

        Ok(MemoryRetainReceipt {
            operation_id: memory.operation_id,
            remote_operation_id,
            adapter_version: HINDSIGHT_ADAPTER_VERSION.to_owned(),
        })
    }

    async fn recall(&self, request: &MemoryRecallRequest) -> MemoryRecallOutcome {
        if request.validate().is_err() {
            return unavailable_outcome(request, RecallUnavailableReason::InvalidResponse);
        }
        let Ok(url) = self.endpoint(&request.bank_id, &["memories", "recall"]) else {
            return unavailable_outcome(request, RecallUnavailableReason::InvalidResponse);
        };
        let response = match self
            .post(url)
            .json(&RecallRequest::from(request))
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => {
                return unavailable_outcome(request, RecallUnavailableReason::AdapterUnavailable);
            }
        };
        let response = match require_success(response).await {
            Ok(response) => response,
            Err(_) => {
                return unavailable_outcome(request, RecallUnavailableReason::AdapterUnavailable);
            }
        };
        let raw_response = match response.json::<Value>().await {
            Ok(response) => response,
            Err(_) => {
                return unavailable_outcome(request, RecallUnavailableReason::InvalidResponse);
            }
        };
        let parsed = match serde_json::from_value::<RecallResponse>(raw_response.clone()) {
            Ok(response) => response,
            Err(_) => {
                return unavailable_outcome(request, RecallUnavailableReason::InvalidResponse);
            }
        };
        let mut results = Vec::with_capacity(parsed.results.len());
        for (rank, result) in parsed.results.into_iter().enumerate() {
            let Ok(rank) = u16::try_from(rank) else {
                return unavailable_outcome(request, RecallUnavailableReason::InvalidResponse);
            };
            let Ok(result) = normalize_recall_result(result, request, rank) else {
                return unavailable_outcome(request, RecallUnavailableReason::InvalidResponse);
            };
            results.push(result);
        }
        let Ok(response_hash) = Digest::canonical(&raw_response) else {
            return unavailable_outcome(request, RecallUnavailableReason::InvalidResponse);
        };

        MemoryRecallOutcome::available(request, HINDSIGHT_ADAPTER_VERSION, response_hash, results)
            .unwrap_or_else(|_| {
                unavailable_outcome(request, RecallUnavailableReason::InvalidResponse)
            })
    }
}

#[derive(Debug, Error)]
pub enum HindsightConfigError {
    #[error("invalid Hindsight base URL: {0}")]
    BaseUrl(String),
    #[error("could not construct Hindsight HTTP client: {0}")]
    Client(String),
}

#[derive(Serialize)]
struct RetainRequest {
    items: Vec<RetainItem>,
    #[serde(rename = "async")]
    async_processing: bool,
    operation_id: Uuid,
}

impl From<&MemoryRetain> for RetainRequest {
    fn from(memory: &MemoryRetain) -> Self {
        let metadata = BTreeMap::from([
            ("world_id".to_owned(), memory.world_id.to_string()),
            ("agent_id".to_owned(), memory.agent_id.to_string()),
            (
                "source_sequence".to_owned(),
                memory.source_sequence.to_string(),
            ),
            ("sim_tick".to_owned(), memory.sim_tick.to_string()),
            ("ordinal".to_owned(), memory.ordinal.to_string()),
            (
                "payload_version".to_owned(),
                memory.payload_version.to_string(),
            ),
        ]);
        Self {
            items: vec![RetainItem {
                content: memory.content.clone(),
                context: memory.context.clone(),
                document_id: memory.document_id.to_string(),
                timestamp: "unset",
                update_mode: "replace",
                metadata,
            }],
            async_processing: true,
            operation_id: memory.operation_id,
        }
    }
}

#[derive(Serialize)]
struct RetainItem {
    content: String,
    context: String,
    document_id: String,
    timestamp: &'static str,
    update_mode: &'static str,
    metadata: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct RetainResponse {
    success: bool,
    bank_id: String,
    items_count: usize,
    #[serde(rename = "async")]
    async_processing: bool,
    operation_id: Option<String>,
}

#[derive(Serialize)]
struct RecallRequest {
    query: String,
    types: [&'static str; 1],
    budget: &'static str,
    max_tokens: u32,
    trace: bool,
}

impl From<&MemoryRecallRequest> for RecallRequest {
    fn from(request: &MemoryRecallRequest) -> Self {
        Self {
            query: request.query.clone(),
            types: ["experience"],
            budget: "low",
            max_tokens: request.max_tokens,
            trace: false,
        }
    }
}

#[derive(Deserialize)]
struct RecallResponse {
    results: Vec<RecallResult>,
}

#[derive(Deserialize)]
struct RecallResult {
    id: String,
    text: String,
    #[serde(rename = "type")]
    kind: MemoryFactKind,
    context: Option<String>,
    document_id: Option<String>,
    metadata: Option<BTreeMap<String, String>>,
    chunk_id: Option<String>,
    #[serde(default)]
    entities: Vec<String>,
}

fn normalize_recall_result(
    result: RecallResult,
    request: &MemoryRecallRequest,
    rank: u16,
) -> Result<RecalledMemory, ()> {
    if result.kind != MemoryFactKind::Experience {
        return Err(());
    }
    let document_id = result
        .document_id
        .as_deref()
        .ok_or(())?
        .parse::<Uuid>()
        .map_err(|_| ())?;
    let metadata = result.metadata.ok_or(())?;
    let world_id = metadata
        .get("world_id")
        .ok_or(())?
        .parse::<world_domain::WorldId>()
        .map_err(|_| ())?;
    let agent_id = metadata
        .get("agent_id")
        .ok_or(())?
        .parse::<world_domain::EntityId>()
        .map_err(|_| ())?;
    let source_sequence = metadata
        .get("source_sequence")
        .ok_or(())?
        .parse::<u64>()
        .map(EventSequence::new)
        .map_err(|_| ())?;
    let sim_tick = metadata
        .get("sim_tick")
        .ok_or(())?
        .parse::<u64>()
        .map(SimTick::new)
        .map_err(|_| ())?;
    let ordinal = metadata
        .get("ordinal")
        .ok_or(())?
        .parse::<u32>()
        .map_err(|_| ())?;
    let payload_version = metadata
        .get("payload_version")
        .ok_or(())?
        .parse::<u16>()
        .map_err(|_| ())?;
    if world_id != request.world_id
        || agent_id != request.agent_id
        || payload_version != MEMORY_PAYLOAD_VERSION
    {
        return Err(());
    }
    let _ = result.chunk_id;
    let _ = result.entities;
    Ok(RecalledMemory {
        rank,
        remote_memory_id: result.id,
        document_id,
        source_sequence,
        sim_tick,
        ordinal,
        text: result.text,
        kind: result.kind,
        context: result.context.ok_or(())?,
    })
}

async fn require_success(
    response: reqwest::Response,
) -> Result<reqwest::Response, MemoryAdapterError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "response body unavailable".to_owned());
    let body = body.chars().take(1_024).collect::<String>();
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        Err(MemoryAdapterError::Unavailable(format!(
            "Hindsight returned {status}: {body}"
        )))
    } else {
        Err(MemoryAdapterError::Rejected(format!(
            "Hindsight returned {status}: {body}"
        )))
    }
}

fn network_error(error: reqwest::Error) -> MemoryAdapterError {
    MemoryAdapterError::Unavailable(error.to_string())
}

fn unavailable_outcome(
    request: &MemoryRecallRequest,
    reason: RecallUnavailableReason,
) -> MemoryRecallOutcome {
    MemoryRecallOutcome::unavailable(request, reason).unwrap_or(MemoryRecallOutcome::Unavailable {
        request_id: request.request_id,
        request_hash: Digest::ZERO,
        reason: RecallUnavailableReason::InvalidResponse,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        Json, Router,
        extract::{Path, State},
        routing::post,
    };
    use serde_json::json;
    use tokio::net::TcpListener;
    use world_domain::{EntityId, EventSequence, SimTick, WorldId};

    use super::*;

    #[derive(Clone)]
    struct TestState {
        retain: MemoryRetain,
    }

    async fn retain_handler(
        State(state): State<Arc<TestState>>,
        Path(bank_id): Path<String>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(bank_id, state.retain.bank_id);
        assert_eq!(body["async"], true);
        assert_eq!(body["operation_id"], state.retain.operation_id.to_string());
        assert_eq!(body["items"][0]["timestamp"], "unset");
        Json(json!({
            "success": true,
            "bank_id": state.retain.bank_id,
            "items_count": 1,
            "async": true,
            "operation_id": state.retain.operation_id,
        }))
    }

    async fn recall_handler(
        State(state): State<Arc<TestState>>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(body["trace"], false);
        assert_eq!(body["types"], json!(["experience"]));
        assert!(body.get("query_timestamp").is_none());
        Json(json!({
            "results": [{
                "id": "memory-1",
                "text": state.retain.content,
                "type": "experience",
                "context": state.retain.context,
                "document_id": state.retain.document_id,
                "metadata": {
                    "world_id": state.retain.world_id.to_string(),
                    "agent_id": state.retain.agent_id.to_string(),
                    "source_sequence": state.retain.source_sequence.to_string(),
                    "sim_tick": state.retain.sim_tick.to_string(),
                    "ordinal": state.retain.ordinal.to_string(),
                    "payload_version": state.retain.payload_version.to_string()
                },
                "chunk_id": "chunk-1",
                "entities": []
            }]
        }))
    }

    #[tokio::test]
    async fn sends_idempotent_retain_and_normalizes_recall() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(31));
        let agent_id = EntityId::deterministic(world_id, b"adapter-test-agent");
        let retain = MemoryRetain::new(
            world_id,
            agent_id,
            EventSequence::new(2),
            SimTick::new(1),
            0,
            "A cold gust preceded discomfort.",
            "direct perception",
        )
        .expect("valid retain");
        let state = Arc::new(TestState {
            retain: retain.clone(),
        });
        let app = Router::new()
            .route("/v1/default/banks/{bank_id}/memories", post(retain_handler))
            .route(
                "/v1/default/banks/{bank_id}/memories/recall",
                post(recall_handler),
            )
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let address = listener.local_addr().expect("read test address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test API");
        });
        let adapter =
            HindsightMemory::new(&format!("http://{address}"), None, Duration::from_secs(2))
                .expect("valid adapter");

        let receipt = adapter.retain(&retain).await.expect("retain accepted");
        assert_eq!(receipt.operation_id, retain.operation_id);
        let recall_request = MemoryRecallRequest::new(
            world_id,
            agent_id,
            SimTick::new(3),
            SimTick::new(5),
            0,
            "cold gust",
            256,
        )
        .expect("valid recall request");
        let recall = adapter.recall(&recall_request).await;
        assert!(matches!(
            recall,
            MemoryRecallOutcome::Available { results, .. }
                if results.len() == 1
                    && results[0].kind == MemoryFactKind::Experience
                    && results[0].document_id == retain.document_id
        ));
        server.abort();
    }

    #[tokio::test]
    #[ignore = "requires a live pinned Hindsight service"]
    async fn live_keyless_retain_then_recall() {
        let base_url = std::env::var("HINDSIGHT_LIVE_URL").expect("set HINDSIGHT_LIVE_URL");
        let world_id =
            WorldId::from_uuid(Uuid::from_u128(0x019f_d4a9_b7f9_7891_ab51_cdf7_1d2b_7702));
        let agent_id = EntityId::deterministic(world_id, b"live-hindsight-smoke-agent");
        let retain = MemoryRetain::new(
            world_id,
            agent_id,
            EventSequence::new(1),
            SimTick::ZERO,
            0,
            "A cold wind crossed the open ground before the body shivered.",
            "non-production live adapter smoke",
        )
        .expect("valid live retain");
        let adapter = HindsightMemory::new(&base_url, None, Duration::from_secs(15))
            .expect("valid live adapter");
        let receipt = adapter.retain(&retain).await.expect("live retain accepted");
        let status_url = adapter
            .endpoint(
                &retain.bank_id,
                &["operations", &receipt.remote_operation_id],
            )
            .expect("valid status URL");

        let mut completed = false;
        for _ in 0..120 {
            let status = adapter
                .client
                .get(status_url.clone())
                .send()
                .await
                .expect("read live operation")
                .json::<Value>()
                .await
                .expect("decode live operation");
            match status.get("status").and_then(Value::as_str) {
                Some("completed") => {
                    completed = true;
                    break;
                }
                Some("failed" | "cancelled") => {
                    panic!("live retain operation did not complete: {status}");
                }
                _ => tokio::time::sleep(Duration::from_millis(250)).await,
            }
        }
        assert!(completed, "live retain operation timed out");

        let recall_request = MemoryRecallRequest::new(
            world_id,
            agent_id,
            SimTick::new(2),
            SimTick::new(4),
            0,
            "cold wind and shivering",
            256,
        )
        .expect("valid live recall");
        assert!(matches!(
            adapter.recall(&recall_request).await,
            MemoryRecallOutcome::Available { results, .. } if !results.is_empty()
        ));
    }
}
