use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    ActionValueState, BodilyNeedState, Digest, EntityId, EventSequence, PerceptionChannel,
    PrimitiveActionKind, SimTick, WorldId,
};

pub const COGNITION_MODEL_CONTRACT_VERSION: u16 = 1;
pub const COGNITION_ROUTE_POLICY_VERSION: u16 = 1;
pub const MAX_COGNITION_ROUTES: usize = 256;
const MAX_INPUT_READINGS: usize = 32;
const MAX_RECALLED_MEMORIES: usize = 8;
const MAX_MEMORY_CONTENT_BYTES: usize = 4 * 1024;
const MAX_MEMORY_CONTEXT_BYTES: usize = 512;
const MAX_PROVIDER_ID_BYTES: usize = 256;
const MAX_MODEL_ID_BYTES: usize = 256;
const MAX_ADAPTER_VERSION_BYTES: usize = 128;
const MAX_PROVIDER_SLUG_BYTES: usize = 64;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CognitionProviderId(String);

impl CognitionProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, CognitionContractError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_PROVIDER_SLUG_BYTES
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            });
        if !valid {
            return Err(CognitionContractError::InvalidProviderId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn known(value: &'static str) -> Self {
        Self(value.to_owned())
    }

    #[must_use]
    pub fn cloudflare_workers_ai() -> Self {
        Self::known("cloudflare_workers_ai")
    }

    #[must_use]
    pub fn groq() -> Self {
        Self::known("groq")
    }

    #[must_use]
    pub fn openrouter() -> Self {
        Self::known("openrouter")
    }

    #[must_use]
    pub fn sambanova() -> Self {
        Self::known("sambanova")
    }

    #[must_use]
    pub fn cerebras() -> Self {
        Self::known("cerebras")
    }

    #[must_use]
    pub fn nvidia_build() -> Self {
        Self::known("nvidia_build")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CognitionBillingClass {
    FreeAllocation,
    TrialCredit,
    DevelopmentOnly,
    PaidApproved,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CognitionRoutePurpose {
    ProductionWorld,
    PreGenesisSoak,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionModelRoute {
    pub provider: CognitionProviderId,
    pub requested_model: String,
    pub billing_class: CognitionBillingClass,
}

impl CognitionModelRoute {
    #[must_use]
    pub fn cloudflare_gpt_oss_20b() -> Self {
        Self {
            provider: CognitionProviderId::cloudflare_workers_ai(),
            requested_model: "@cf/openai/gpt-oss-20b".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn cloudflare_gpt_oss_120b() -> Self {
        Self {
            provider: CognitionProviderId::cloudflare_workers_ai(),
            requested_model: "@cf/openai/gpt-oss-120b".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn groq_gpt_oss_20b() -> Self {
        Self {
            provider: CognitionProviderId::groq(),
            requested_model: "openai/gpt-oss-20b".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn groq_gpt_oss_120b() -> Self {
        Self {
            provider: CognitionProviderId::groq(),
            requested_model: "openai/gpt-oss-120b".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn openrouter_free() -> Self {
        Self {
            provider: CognitionProviderId::openrouter(),
            requested_model: "openrouter/free".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn openrouter_gpt_oss_20b_free() -> Self {
        Self {
            provider: CognitionProviderId::openrouter(),
            requested_model: "openai/gpt-oss-20b:free".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn openrouter_gpt_oss_120b_free() -> Self {
        Self {
            provider: CognitionProviderId::openrouter(),
            requested_model: "openai/gpt-oss-120b:free".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn sambanova_gpt_oss_120b() -> Self {
        Self {
            provider: CognitionProviderId::sambanova(),
            requested_model: "gpt-oss-120b".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn cerebras_gpt_oss_120b_trial() -> Self {
        Self {
            provider: CognitionProviderId::cerebras(),
            requested_model: "gpt-oss-120b".to_owned(),
            billing_class: CognitionBillingClass::TrialCredit,
        }
    }

    #[must_use]
    pub fn nvidia_nemotron_3_ultra_development() -> Self {
        Self {
            provider: CognitionProviderId::nvidia_build(),
            requested_model: "nvidia/nemotron-3-ultra-550b-a55b".to_owned(),
            billing_class: CognitionBillingClass::DevelopmentOnly,
        }
    }

    #[must_use]
    pub fn openrouter_deepseek_v4_flash() -> Self {
        Self {
            provider: CognitionProviderId::openrouter(),
            requested_model: "deepseek/deepseek-v4-flash".to_owned(),
            billing_class: CognitionBillingClass::PaidApproved,
        }
    }

    pub fn validate(&self) -> Result<(), CognitionContractError> {
        CognitionProviderId::new(self.provider.as_str())?;
        if self.requested_model.trim() != self.requested_model
            || self.requested_model.is_empty()
            || self.requested_model.len() > MAX_MODEL_ID_BYTES
        {
            return Err(CognitionContractError::UnapprovedRoute);
        }
        let allowed = match (self.provider.as_str(), self.billing_class) {
            ("cloudflare_workers_ai", CognitionBillingClass::FreeAllocation) => {
                matches!(
                    self.requested_model.as_str(),
                    "@cf/openai/gpt-oss-20b" | "@cf/openai/gpt-oss-120b"
                )
            }
            ("groq", CognitionBillingClass::FreeAllocation) => matches!(
                self.requested_model.as_str(),
                "openai/gpt-oss-20b" | "openai/gpt-oss-120b"
            ),
            ("openrouter", CognitionBillingClass::FreeAllocation) => {
                self.requested_model == "openrouter/free"
                    || matches!(
                        self.requested_model.as_str(),
                        "openai/gpt-oss-20b:free" | "openai/gpt-oss-120b:free"
                    )
            }
            ("sambanova", CognitionBillingClass::FreeAllocation) => {
                self.requested_model == "gpt-oss-120b"
            }
            ("cerebras", CognitionBillingClass::TrialCredit) => {
                self.requested_model == "gpt-oss-120b"
            }
            ("nvidia_build", CognitionBillingClass::DevelopmentOnly) => {
                self.requested_model == "nvidia/nemotron-3-ultra-550b-a55b"
            }
            ("openrouter", CognitionBillingClass::PaidApproved) => {
                self.requested_model == "deepseek/deepseek-v4-flash"
            }
            _ => false,
        };
        if !allowed {
            return Err(CognitionContractError::UnapprovedRoute);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionRouteRegistry {
    pub policy_version: u16,
    pub routes: Vec<CognitionModelRoute>,
}

impl CognitionRouteRegistry {
    #[must_use]
    pub fn production_default() -> Self {
        Self {
            policy_version: COGNITION_ROUTE_POLICY_VERSION,
            routes: vec![
                CognitionModelRoute::cloudflare_gpt_oss_20b(),
                CognitionModelRoute::cloudflare_gpt_oss_120b(),
                CognitionModelRoute::groq_gpt_oss_20b(),
                CognitionModelRoute::groq_gpt_oss_120b(),
                CognitionModelRoute::sambanova_gpt_oss_120b(),
                CognitionModelRoute::openrouter_free(),
                CognitionModelRoute::openrouter_gpt_oss_20b_free(),
                CognitionModelRoute::openrouter_gpt_oss_120b_free(),
                CognitionModelRoute::openrouter_deepseek_v4_flash(),
            ],
        }
    }

    #[must_use]
    pub fn pre_genesis_soak_default() -> Self {
        let mut registry = Self::production_default();
        let insert_at = registry.routes.len().saturating_sub(1);
        registry.routes.insert(
            insert_at,
            CognitionModelRoute::cerebras_gpt_oss_120b_trial(),
        );
        registry.routes.insert(
            insert_at + 1,
            CognitionModelRoute::nvidia_nemotron_3_ultra_development(),
        );
        registry
    }

    pub fn validate(&self, purpose: CognitionRoutePurpose) -> Result<(), CognitionContractError> {
        if self.policy_version != COGNITION_ROUTE_POLICY_VERSION {
            return Err(CognitionContractError::UnsupportedRoutePolicyVersion(
                self.policy_version,
            ));
        }
        if self.routes.is_empty() || self.routes.len() > MAX_COGNITION_ROUTES {
            return Err(CognitionContractError::InvalidRouteRegistry);
        }

        let mut seen = BTreeSet::new();
        let mut paid_count = 0_usize;
        let mut previous_rank = 0_u8;
        for (index, route) in self.routes.iter().enumerate() {
            route.validate()?;
            if !seen.insert((route.provider.clone(), route.requested_model.clone())) {
                return Err(CognitionContractError::DuplicateRoute);
            }
            if purpose == CognitionRoutePurpose::ProductionWorld
                && matches!(
                    route.billing_class,
                    CognitionBillingClass::TrialCredit | CognitionBillingClass::DevelopmentOnly
                )
            {
                return Err(CognitionContractError::NonProductionRoute);
            }
            let rank = match route.billing_class {
                CognitionBillingClass::FreeAllocation => 0,
                CognitionBillingClass::TrialCredit | CognitionBillingClass::DevelopmentOnly => 1,
                CognitionBillingClass::PaidApproved => 2,
            };
            if rank < previous_rank {
                return Err(CognitionContractError::InvalidRouteOrder);
            }
            previous_rank = rank;
            if route.billing_class == CognitionBillingClass::PaidApproved {
                paid_count += 1;
                if paid_count > 1 || index + 1 != self.routes.len() {
                    return Err(CognitionContractError::InvalidPaidRoute);
                }
                if route != &CognitionModelRoute::openrouter_deepseek_v4_flash() {
                    return Err(CognitionContractError::InvalidPaidRoute);
                }
            }
        }
        Ok(())
    }

    pub fn canonical_hash(
        &self,
        purpose: CognitionRoutePurpose,
    ) -> Result<Digest, CognitionContractError> {
        self.validate(purpose)?;
        Digest::canonical(self).map_err(|error| CognitionContractError::Hash(error.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionInputReading {
    pub subject_id: Option<EntityId>,
    pub channel: PerceptionChannel,
    pub property_code: String,
    pub quantized_value: i32,
    pub uncertainty: u16,
    pub observed_at: SimTick,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionMemoryInput {
    pub document_id: Uuid,
    pub source_sequence: EventSequence,
    pub sim_tick: SimTick,
    pub content: String,
    pub context: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCognitionRequest {
    pub contract_version: u16,
    pub request_id: Uuid,
    pub world_id: WorldId,
    pub agent_id: EntityId,
    pub selected_at_tick: SimTick,
    pub deadline_tick: SimTick,
    pub bodily_needs: BodilyNeedState,
    pub readings: Vec<CognitionInputReading>,
    pub action_values: Vec<ActionValueState>,
    pub recalled_memories: Vec<CognitionMemoryInput>,
    pub max_output_tokens: u16,
}

impl ModelCognitionRequest {
    pub fn validate(&self) -> Result<(), CognitionContractError> {
        if self.contract_version != COGNITION_MODEL_CONTRACT_VERSION {
            return Err(CognitionContractError::UnsupportedContractVersion(
                self.contract_version,
            ));
        }
        if self.deadline_tick <= self.selected_at_tick {
            return Err(CognitionContractError::InvalidDeadline);
        }
        if self.max_output_tokens == 0 || self.max_output_tokens > 64 {
            return Err(CognitionContractError::InvalidOutputBudget);
        }
        if self.readings.len() > MAX_INPUT_READINGS
            || self
                .readings
                .windows(2)
                .any(|pair| reading_key(&pair[0]) >= reading_key(&pair[1]))
            || self
                .readings
                .iter()
                .any(|reading| reading.observed_at > self.selected_at_tick)
        {
            return Err(CognitionContractError::InvalidReadings);
        }
        if self
            .action_values
            .windows(2)
            .any(|pair| pair[0].action_kind >= pair[1].action_kind)
            || self
                .action_values
                .iter()
                .any(|value| value.validate().is_err())
        {
            return Err(CognitionContractError::InvalidActionValues);
        }
        if self.recalled_memories.len() > MAX_RECALLED_MEMORIES
            || self
                .recalled_memories
                .windows(2)
                .any(|pair| pair[0].document_id >= pair[1].document_id)
            || self.recalled_memories.iter().any(|memory| {
                memory.source_sequence == EventSequence::ZERO
                    || memory.sim_tick > self.selected_at_tick
                    || memory.content.trim().is_empty()
                    || memory.content.len() > MAX_MEMORY_CONTENT_BYTES
                    || memory.context.trim().is_empty()
                    || memory.context.len() > MAX_MEMORY_CONTEXT_BYTES
            })
        {
            return Err(CognitionContractError::InvalidMemories);
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<Digest, CognitionContractError> {
        self.validate()?;
        Digest::canonical(self).map_err(|error| CognitionContractError::Hash(error.to_string()))
    }
}

fn reading_key(reading: &CognitionInputReading) -> (Option<EntityId>, PerceptionChannel, &str) {
    (
        reading.subject_id,
        reading.channel,
        reading.property_code.as_str(),
    )
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCognitionReceipt {
    pub contract_version: u16,
    pub request_id: Uuid,
    pub request_hash: Digest,
    pub provider: CognitionProviderId,
    pub requested_model: String,
    pub resolved_model: String,
    pub provider_response_id: String,
    pub usage: ModelTokenUsage,
    pub billed_micro_usd: u64,
    pub action_kind: PrimitiveActionKind,
    pub provider_response_hash: Digest,
    pub adapter_version: String,
}

impl ModelCognitionReceipt {
    pub fn validate_against(
        &self,
        route: &CognitionModelRoute,
        request: &ModelCognitionRequest,
    ) -> Result<(), CognitionContractError> {
        route.validate()?;
        request.validate()?;
        if self.contract_version != COGNITION_MODEL_CONTRACT_VERSION {
            return Err(CognitionContractError::UnsupportedContractVersion(
                self.contract_version,
            ));
        }
        if self.request_id != request.request_id
            || self.request_hash != request.canonical_hash()?
            || self.provider != route.provider
            || self.requested_model != route.requested_model
            || self.resolved_model.trim().is_empty()
            || self.resolved_model.len() > MAX_MODEL_ID_BYTES
            || self.provider_response_id.trim().is_empty()
            || self.provider_response_id.len() > MAX_PROVIDER_ID_BYTES
            || self.provider_response_hash == Digest::ZERO
            || self.adapter_version.trim().is_empty()
            || self.adapter_version.len() > MAX_ADAPTER_VERSION_BYTES
            || self.usage.completion_tokens > u32::from(request.max_output_tokens)
        {
            return Err(CognitionContractError::InvalidReceipt);
        }
        match route.billing_class {
            CognitionBillingClass::FreeAllocation
            | CognitionBillingClass::TrialCredit
            | CognitionBillingClass::DevelopmentOnly
                if self.billed_micro_usd != 0 =>
            {
                return Err(CognitionContractError::FreeRouteReportedCost);
            }
            CognitionBillingClass::PaidApproved if self.resolved_model != route.requested_model => {
                return Err(CognitionContractError::PaidModelMismatch);
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CognitionRouteAttemptStatus {
    Succeeded,
    Unavailable,
    Rejected,
    InvalidResponse,
    SkippedUnconfigured,
    SkippedCooldown,
    SkippedQuotaExhausted,
    SkippedDisabled,
    SkippedPaidUnauthorized,
    StoppedAttemptLimit,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionRouteAttempt {
    pub route_index: u16,
    pub provider: CognitionProviderId,
    pub requested_model: String,
    pub billing_class: CognitionBillingClass,
    pub status: CognitionRouteAttemptStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCognitionLadderResult {
    pub contract_version: u16,
    pub request_id: Uuid,
    pub route_policy_version: u16,
    pub route_registry_hash: Digest,
    pub attempts: Vec<CognitionRouteAttempt>,
    pub receipt: Option<ModelCognitionReceipt>,
}

impl ModelCognitionLadderResult {
    pub fn validate_against(
        &self,
        registry: &CognitionRouteRegistry,
        purpose: CognitionRoutePurpose,
        request: &ModelCognitionRequest,
    ) -> Result<(), CognitionContractError> {
        registry.validate(purpose)?;
        request.validate()?;
        if self.contract_version != COGNITION_MODEL_CONTRACT_VERSION
            || self.request_id != request.request_id
            || self.route_policy_version != registry.policy_version
            || self.route_registry_hash != registry.canonical_hash(purpose)?
            || self.attempts.is_empty()
            || self.attempts.len() > registry.routes.len()
        {
            return Err(CognitionContractError::InvalidLadderResult);
        }

        let mut succeeded_route = None;
        for (position, attempt) in self.attempts.iter().enumerate() {
            let route = &registry.routes[position];
            if usize::from(attempt.route_index) != position
                || attempt.provider != route.provider
                || attempt.requested_model != route.requested_model
                || attempt.billing_class != route.billing_class
            {
                return Err(CognitionContractError::InvalidLadderResult);
            }
            let terminates = matches!(
                attempt.status,
                CognitionRouteAttemptStatus::Succeeded
                    | CognitionRouteAttemptStatus::StoppedAttemptLimit
            );
            if terminates && position + 1 != self.attempts.len() {
                return Err(CognitionContractError::InvalidLadderResult);
            }
            if attempt.status == CognitionRouteAttemptStatus::Succeeded {
                succeeded_route = Some(route);
            }
        }

        match (succeeded_route, &self.receipt) {
            (Some(route), Some(receipt)) => receipt.validate_against(route, request),
            (None, None) => Ok(()),
            _ => Err(CognitionContractError::InvalidLadderResult),
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CognitionContractError {
    #[error("unsupported cognition model contract version {0}")]
    UnsupportedContractVersion(u16),
    #[error("unsupported cognition route-policy version {0}")]
    UnsupportedRoutePolicyVersion(u16),
    #[error("cognition provider ID must be a short lowercase ASCII slug")]
    InvalidProviderId,
    #[error("cognition model route is not explicitly approved")]
    UnapprovedRoute,
    #[error("cognition route registry must contain between 1 and 256 routes")]
    InvalidRouteRegistry,
    #[error("cognition route registry contains a duplicate provider/model route")]
    DuplicateRoute,
    #[error("trial or development-only cognition routes cannot serve a public world")]
    NonProductionRoute,
    #[error("free, trial, development, and paid cognition routes are out of order")]
    InvalidRouteOrder,
    #[error("the sole approved paid cognition route must be the final route")]
    InvalidPaidRoute,
    #[error("cognition deadline must be after selection")]
    InvalidDeadline,
    #[error("cognition output budget must be between 1 and 64 tokens")]
    InvalidOutputBudget,
    #[error("cognition readings are oversized, reordered, duplicated, or from the future")]
    InvalidReadings,
    #[error("cognition action values are invalid or non-canonical")]
    InvalidActionValues,
    #[error("cognition memories are oversized, reordered, duplicated, or invalid")]
    InvalidMemories,
    #[error("cognition receipt does not match its route and request")]
    InvalidReceipt,
    #[error("cognition route-ladder result is not a canonical prefix of its registry")]
    InvalidLadderResult,
    #[error("a free cognition route reported non-zero cost")]
    FreeRouteReportedCost,
    #[error("a paid cognition response used a model other than the approved model")]
    PaidModelMismatch,
    #[error("cognition contract hashing failed: {0}")]
    Hash(String),
}

#[derive(Debug, Error)]
pub enum CognitionModelError {
    #[error("cognition model service is unavailable: {0}")]
    Unavailable(String),
    #[error("cognition model service rejected the request: {0}")]
    Rejected(String),
    #[error("cognition model service returned an invalid response: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait CognitionModel: Send + Sync {
    async fn infer(
        &self,
        route: &CognitionModelRoute,
        request: &ModelCognitionRequest,
    ) -> Result<ModelCognitionReceipt, CognitionModelError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_allowlist_is_exact() {
        for route in [
            CognitionModelRoute::cloudflare_gpt_oss_20b(),
            CognitionModelRoute::cloudflare_gpt_oss_120b(),
            CognitionModelRoute::groq_gpt_oss_20b(),
            CognitionModelRoute::groq_gpt_oss_120b(),
            CognitionModelRoute::openrouter_free(),
            CognitionModelRoute::openrouter_gpt_oss_20b_free(),
            CognitionModelRoute::openrouter_gpt_oss_120b_free(),
            CognitionModelRoute::sambanova_gpt_oss_120b(),
            CognitionModelRoute::cerebras_gpt_oss_120b_trial(),
            CognitionModelRoute::nvidia_nemotron_3_ultra_development(),
            CognitionModelRoute::openrouter_deepseek_v4_flash(),
        ] {
            assert_eq!(route.validate(), Ok(()));
        }
        let mut forged = CognitionModelRoute::openrouter_deepseek_v4_flash();
        forged.requested_model = "openai/gpt-5.5-pro".to_owned();
        assert_eq!(
            forged.validate(),
            Err(CognitionContractError::UnapprovedRoute)
        );
    }

    #[test]
    fn production_registry_has_capacity_without_relaxing_route_approval() {
        let registry = CognitionRouteRegistry::production_default();
        assert_eq!(
            registry.validate(CognitionRoutePurpose::ProductionWorld),
            Ok(())
        );
        assert!(registry.routes.len() < MAX_COGNITION_ROUTES);

        let soak = CognitionRouteRegistry::pre_genesis_soak_default();
        assert_eq!(soak.validate(CognitionRoutePurpose::PreGenesisSoak), Ok(()));
        assert_eq!(
            soak.validate(CognitionRoutePurpose::ProductionWorld),
            Err(CognitionContractError::NonProductionRoute)
        );
    }

    #[test]
    fn registry_rejects_duplicates_and_paid_routes_before_the_tail() {
        let mut duplicate = CognitionRouteRegistry::production_default();
        duplicate
            .routes
            .insert(1, CognitionModelRoute::cloudflare_gpt_oss_20b());
        assert_eq!(
            duplicate.validate(CognitionRoutePurpose::ProductionWorld),
            Err(CognitionContractError::DuplicateRoute)
        );

        let mut early_paid = CognitionRouteRegistry::production_default();
        let paid = early_paid.routes.pop().expect("paid tail");
        early_paid.routes.insert(0, paid);
        assert_eq!(
            early_paid.validate(CognitionRoutePurpose::ProductionWorld),
            Err(CognitionContractError::InvalidPaidRoute)
        );
    }
}
