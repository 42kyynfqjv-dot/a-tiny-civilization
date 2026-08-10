use async_trait::async_trait;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    ActionValueState, BodilyNeedState, CognitionDeadlineInput, CognitionRequestSelection, Digest,
    EntityId, EventId, EventSequence, PerceptionChannel, PrimitiveActionKind, SimTick, WorldId,
};
pub use world_domain::{CognitionReading as CognitionInputReading, cognition_request_id};

pub const COGNITION_MODEL_CONTRACT_VERSION: u16 = 1;
pub const COGNITION_ROUTE_POLICY_VERSION: u16 = 1;
pub const CANCER_RESEARCH_EXPLORATION_ROUTE_POLICY_VERSION: u16 = 3;
pub const CANCER_RESEARCH_ESCALATION_ROUTE_POLICY_VERSION: u16 = 4;
pub const MAX_COGNITION_ROUTES: usize = 256;
pub const COGNITION_TARGET_MICRO_USD_PER_MONTH: u64 = 2_500_000;
pub const COGNITION_HARD_STOP_MICRO_USD_PER_MONTH: u64 = 3_000_000;
pub const CANCER_RESEARCH_TARGET_MICRO_USD_PER_MONTH: u64 = 2_500_000;
pub const CANCER_RESEARCH_HARD_STOP_MICRO_USD_PER_MONTH: u64 = 2_850_000;
pub const MAX_PAID_COGNITION_RESERVATION_MICRO_USD: u64 = 50_000;
const MAX_INPUT_READINGS: usize = 32;
pub const MAX_COGNITION_RECALLED_MEMORIES: usize = 8;
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
    pub fn openrouter_cancer() -> Self {
        Self::known("openrouter_cancer")
    }

    #[must_use]
    pub fn cerebras() -> Self {
        Self::known("cerebras")
    }

    #[must_use]
    pub fn nvidia_build() -> Self {
        Self::known("nvidia_build")
    }

    #[must_use]
    pub fn local_openai() -> Self {
        Self::known("local_openai")
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
pub enum CognitionBillingScope {
    Production,
    CancerResearch,
}

impl CognitionBillingScope {
    #[must_use]
    pub fn for_route(route: &CognitionModelRoute) -> Self {
        if route.provider == CognitionProviderId::openrouter_cancer() {
            Self::CancerResearch
        } else {
            Self::Production
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::CancerResearch => "cancer_research",
        }
    }

    #[must_use]
    pub const fn monthly_limits_micro_usd(self) -> (u64, u64) {
        match self {
            Self::Production => (
                COGNITION_TARGET_MICRO_USD_PER_MONTH,
                COGNITION_HARD_STOP_MICRO_USD_PER_MONTH,
            ),
            Self::CancerResearch => (
                CANCER_RESEARCH_TARGET_MICRO_USD_PER_MONTH,
                CANCER_RESEARCH_HARD_STOP_MICRO_USD_PER_MONTH,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CognitionRoutePurpose {
    ProductionWorld,
    CancerResearchExploration,
    CancerResearchEscalation,
    Development,
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
    pub fn local_qwen2_5_1_5b() -> Self {
        Self {
            provider: CognitionProviderId::local_openai(),
            requested_model: "qwen2.5:1.5b".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn local_gpt_oss_20b() -> Self {
        Self {
            provider: CognitionProviderId::local_openai(),
            requested_model: "gpt-oss-20b".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

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
    pub fn openrouter_cancer_nemotron_3_ultra_free() -> Self {
        Self {
            provider: CognitionProviderId::openrouter_cancer(),
            requested_model: "nvidia/nemotron-3-ultra-550b-a55b:free".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn cerebras_gpt_oss_120b() -> Self {
        Self {
            provider: CognitionProviderId::cerebras(),
            requested_model: "gpt-oss-120b".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
        }
    }

    #[must_use]
    pub fn cerebras_llama3_1_8b() -> Self {
        Self {
            provider: CognitionProviderId::cerebras(),
            requested_model: "llama3.1-8b".to_owned(),
            billing_class: CognitionBillingClass::FreeAllocation,
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

    #[must_use]
    pub fn openrouter_cancer_deepseek_v4_pro() -> Self {
        Self {
            provider: CognitionProviderId::openrouter_cancer(),
            requested_model: "deepseek/deepseek-v4-pro".to_owned(),
            billing_class: CognitionBillingClass::PaidApproved,
        }
    }

    #[must_use]
    pub fn openrouter_cancer_deepseek_v4_flash() -> Self {
        Self {
            provider: CognitionProviderId::openrouter_cancer(),
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
            ("local_openai", CognitionBillingClass::FreeAllocation) => {
                matches!(
                    self.requested_model.as_str(),
                    "qwen2.5:1.5b" | "gpt-oss-20b"
                )
            }
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
            ("cerebras", CognitionBillingClass::FreeAllocation) => matches!(
                self.requested_model.as_str(),
                "gpt-oss-120b" | "llama3.1-8b"
            ),
            ("nvidia_build", CognitionBillingClass::DevelopmentOnly) => {
                self.requested_model == "nvidia/nemotron-3-ultra-550b-a55b"
            }
            ("openrouter", CognitionBillingClass::PaidApproved) => {
                self.requested_model == "deepseek/deepseek-v4-flash"
            }
            ("openrouter_cancer", CognitionBillingClass::PaidApproved) => matches!(
                self.requested_model.as_str(),
                "deepseek/deepseek-v4-pro" | "deepseek/deepseek-v4-flash"
            ),
            ("openrouter_cancer", CognitionBillingClass::FreeAllocation) => {
                self.requested_model == "nvidia/nemotron-3-ultra-550b-a55b:free"
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
                CognitionModelRoute::local_qwen2_5_1_5b(),
                CognitionModelRoute::local_gpt_oss_20b(),
                CognitionModelRoute::cloudflare_gpt_oss_20b(),
                CognitionModelRoute::cloudflare_gpt_oss_120b(),
                CognitionModelRoute::groq_gpt_oss_20b(),
                CognitionModelRoute::groq_gpt_oss_120b(),
                CognitionModelRoute::cerebras_llama3_1_8b(),
                CognitionModelRoute::cerebras_gpt_oss_120b(),
                CognitionModelRoute::openrouter_free(),
                CognitionModelRoute::openrouter_gpt_oss_20b_free(),
                CognitionModelRoute::openrouter_gpt_oss_120b_free(),
                CognitionModelRoute::openrouter_deepseek_v4_flash(),
            ],
        }
    }

    #[must_use]
    pub fn development_default() -> Self {
        let mut registry = Self::production_default();
        let insert_at = registry.routes.len().saturating_sub(1);
        registry.routes.insert(
            insert_at,
            CognitionModelRoute::nvidia_nemotron_3_ultra_development(),
        );
        registry
    }

    #[must_use]
    pub fn cancer_research_exploration() -> Self {
        Self {
            policy_version: CANCER_RESEARCH_EXPLORATION_ROUTE_POLICY_VERSION,
            routes: vec![CognitionModelRoute::openrouter_cancer_nemotron_3_ultra_free()],
        }
    }

    #[must_use]
    pub fn cancer_research_escalation() -> Self {
        Self {
            policy_version: CANCER_RESEARCH_ESCALATION_ROUTE_POLICY_VERSION,
            routes: vec![
                CognitionModelRoute::openrouter_cancer_deepseek_v4_pro(),
                CognitionModelRoute::openrouter_cancer_deepseek_v4_flash(),
            ],
        }
    }

    pub fn validate(&self, purpose: CognitionRoutePurpose) -> Result<(), CognitionContractError> {
        let expected_policy_version = match purpose {
            CognitionRoutePurpose::CancerResearchExploration => {
                CANCER_RESEARCH_EXPLORATION_ROUTE_POLICY_VERSION
            }
            CognitionRoutePurpose::CancerResearchEscalation => {
                CANCER_RESEARCH_ESCALATION_ROUTE_POLICY_VERSION
            }
            CognitionRoutePurpose::ProductionWorld | CognitionRoutePurpose::Development => {
                COGNITION_ROUTE_POLICY_VERSION
            }
        };
        if self.policy_version != expected_policy_version {
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
            if purpose != CognitionRoutePurpose::Development
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
                if purpose == CognitionRoutePurpose::ProductionWorld
                    && (index + 1 != self.routes.len()
                        || route != &CognitionModelRoute::openrouter_deepseek_v4_flash())
                {
                    return Err(CognitionContractError::InvalidPaidRoute);
                }
            }
        }
        match purpose {
            CognitionRoutePurpose::ProductionWorld => {
                if paid_count != 1
                    || self.routes.last()
                        != Some(&CognitionModelRoute::openrouter_deepseek_v4_flash())
                {
                    return Err(CognitionContractError::InvalidPaidRoute);
                }
            }
            CognitionRoutePurpose::CancerResearchExploration => {
                if self.routes
                    != vec![CognitionModelRoute::openrouter_cancer_nemotron_3_ultra_free()]
                {
                    return Err(CognitionContractError::InvalidRouteRegistry);
                }
            }
            CognitionRoutePurpose::CancerResearchEscalation => {
                if self.routes
                    != vec![
                        CognitionModelRoute::openrouter_cancer_deepseek_v4_pro(),
                        CognitionModelRoute::openrouter_cancer_deepseek_v4_flash(),
                    ]
                {
                    return Err(CognitionContractError::InvalidPaidRoute);
                }
            }
            CognitionRoutePurpose::Development => {}
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
    pub ordinal: u32,
    pub selected_at_tick: SimTick,
    pub deadline_tick: SimTick,
    pub bodily_needs: BodilyNeedState,
    pub readings: Vec<CognitionInputReading>,
    pub action_values: Vec<ActionValueState>,
    pub recalled_memories: Vec<CognitionMemoryInput>,
    pub max_output_tokens: u16,
}

impl ModelCognitionRequest {
    pub fn from_selection(
        selection: &CognitionRequestSelection,
        recalled_memories: Vec<CognitionMemoryInput>,
    ) -> Result<Self, CognitionContractError> {
        selection
            .validate()
            .map_err(|error| CognitionContractError::InvalidJob(error.to_string()))?;
        let request = Self {
            contract_version: COGNITION_MODEL_CONTRACT_VERSION,
            request_id: selection.request_id,
            world_id: selection.world_id,
            agent_id: selection.organism_id,
            ordinal: selection.ordinal,
            selected_at_tick: selection.selected_at_tick,
            deadline_tick: selection.deadline_tick,
            bodily_needs: selection.bodily_needs,
            readings: selection.readings.clone(),
            action_values: selection.action_values.clone(),
            recalled_memories,
            max_output_tokens: selection.model_max_output_tokens,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), CognitionContractError> {
        if self.contract_version != COGNITION_MODEL_CONTRACT_VERSION {
            return Err(CognitionContractError::UnsupportedContractVersion(
                self.contract_version,
            ));
        }
        if self.request_id
            != cognition_request_id(
                self.world_id,
                self.agent_id,
                self.selected_at_tick,
                self.ordinal,
            )
        {
            return Err(CognitionContractError::InvalidDeterministicRequestId);
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
        if self.recalled_memories.len() > MAX_COGNITION_RECALLED_MEMORIES
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_region: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_intensity: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement_direction: Option<u8>,
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
            || self.contact_region.is_some_and(|region| {
                self.action_kind != PrimitiveActionKind::ApplyForce || region >= 8
            })
            || self.signal_intensity.is_some_and(|intensity| {
                self.action_kind != PrimitiveActionKind::EmitSignal
                    || !(1..=world_domain::SIGNAL_FORM_VARIANT_COUNT).contains(&intensity)
            })
            || self.movement_direction.is_some_and(|direction| {
                self.action_kind != PrimitiveActionKind::Move || direction >= 4
            })
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
    #[error("cognition request ID does not match its world, life, tick, and ordinal")]
    InvalidDeterministicRequestId,
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
    #[error("cognition job is invalid: {0}")]
    InvalidJob(String),
    #[error("persisted cognition route attempt is invalid")]
    InvalidAttemptRecord,
    #[error("paid cognition authorization is invalid")]
    InvalidPaidAuthorization,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionJobEntry {
    pub selection: CognitionRequestSelection,
    pub source_sequence: EventSequence,
    pub source_event_id: EventId,
    pub source_event_index: u32,
    pub claim_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionRecallRecord {
    pub request: crate::MemoryRecallRequest,
    pub outcome: crate::MemoryRecallOutcome,
    pub admitted_memories: Vec<CognitionMemoryInput>,
}

impl CognitionRecallRecord {
    pub fn from_outcome(
        selection: &CognitionRequestSelection,
        outcome: crate::MemoryRecallOutcome,
    ) -> Result<Self, CognitionContractError> {
        let request = crate::MemoryRecallRequest::from_cognition_selection(selection)
            .map_err(|error| CognitionContractError::InvalidJob(error.to_string()))?;
        outcome
            .validate_against(&request)
            .map_err(|error| CognitionContractError::InvalidJob(error.to_string()))?;
        let admitted_memories = admitted_memories_from_outcome(&outcome);
        let record = Self {
            request,
            outcome,
            admitted_memories,
        };
        ModelCognitionRequest::from_selection(selection, record.admitted_memories.clone())?;
        Ok(record)
    }

    pub fn validate_against(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<(), CognitionContractError> {
        entry.validate()?;
        let expected_request =
            crate::MemoryRecallRequest::from_cognition_selection(&entry.selection)
                .map_err(|error| CognitionContractError::InvalidJob(error.to_string()))?;
        if self.request != expected_request || self.outcome.validate_against(&self.request).is_err()
        {
            return Err(CognitionContractError::InvalidJob(
                "recall request or outcome differs from its cognition selection".to_owned(),
            ));
        }
        let expected_memories = admitted_memories_from_outcome(&self.outcome);
        if self.admitted_memories != expected_memories {
            return Err(CognitionContractError::InvalidJob(
                "admitted memories differ from the normalized recall outcome".to_owned(),
            ));
        }
        ModelCognitionRequest::from_selection(&entry.selection, self.admitted_memories.clone())?;
        Ok(())
    }
}

fn admitted_memories_from_outcome(
    outcome: &crate::MemoryRecallOutcome,
) -> Vec<CognitionMemoryInput> {
    match outcome {
        crate::MemoryRecallOutcome::Available { results, .. } => {
            let mut memories = results
                .iter()
                .take(MAX_COGNITION_RECALLED_MEMORIES)
                .map(|memory| CognitionMemoryInput {
                    document_id: memory.document_id,
                    source_sequence: memory.source_sequence,
                    sim_tick: memory.sim_tick,
                    content: memory.text.clone(),
                    context: memory.context.clone(),
                })
                .collect::<Vec<_>>();
            memories.sort_by_key(|memory| memory.document_id);
            memories
        }
        crate::MemoryRecallOutcome::Unavailable { .. } => Vec::new(),
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CognitionAttemptPersistenceState {
    Skipped,
    Dispatched,
    Completed,
    Abandoned,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionRouteAttemptRecord {
    pub route_index: u16,
    pub route: CognitionModelRoute,
    pub persistence_state: CognitionAttemptPersistenceState,
    pub attempt: Option<CognitionRouteAttempt>,
    pub receipt: Option<ModelCognitionReceipt>,
}

impl CognitionRouteAttemptRecord {
    pub fn validate(&self) -> Result<(), CognitionContractError> {
        self.route.validate()?;
        let terminal = self.attempt.as_ref();
        if terminal.is_some_and(|attempt| {
            attempt.route_index != self.route_index
                || attempt.provider != self.route.provider
                || attempt.requested_model != self.route.requested_model
                || attempt.billing_class != self.route.billing_class
        }) {
            return Err(CognitionContractError::InvalidAttemptRecord);
        }
        let valid = match self.persistence_state {
            CognitionAttemptPersistenceState::Dispatched => {
                self.attempt.is_none() && self.receipt.is_none()
            }
            CognitionAttemptPersistenceState::Skipped => terminal
                .is_some_and(|attempt| is_skip_status(attempt.status) && self.receipt.is_none()),
            CognitionAttemptPersistenceState::Completed => terminal.is_some_and(|attempt| {
                is_network_terminal_status(attempt.status)
                    && ((attempt.status == CognitionRouteAttemptStatus::Succeeded)
                        == self.receipt.is_some())
            }),
            CognitionAttemptPersistenceState::Abandoned => terminal.is_some_and(|attempt| {
                is_network_terminal_status(attempt.status)
                    && attempt.status == CognitionRouteAttemptStatus::Unavailable
                    && self.receipt.is_none()
            }),
        };
        if valid {
            Ok(())
        } else {
            Err(CognitionContractError::InvalidAttemptRecord)
        }
    }
}

#[must_use]
pub const fn is_skip_status(status: CognitionRouteAttemptStatus) -> bool {
    matches!(
        status,
        CognitionRouteAttemptStatus::SkippedUnconfigured
            | CognitionRouteAttemptStatus::SkippedCooldown
            | CognitionRouteAttemptStatus::SkippedQuotaExhausted
            | CognitionRouteAttemptStatus::SkippedDisabled
            | CognitionRouteAttemptStatus::SkippedPaidUnauthorized
            | CognitionRouteAttemptStatus::StoppedAttemptLimit
    )
}

#[must_use]
pub const fn is_network_terminal_status(status: CognitionRouteAttemptStatus) -> bool {
    matches!(
        status,
        CognitionRouteAttemptStatus::Succeeded
            | CognitionRouteAttemptStatus::Unavailable
            | CognitionRouteAttemptStatus::Rejected
            | CognitionRouteAttemptStatus::InvalidResponse
    )
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaidCognitionAuthorization {
    pub request_id: Uuid,
    pub billing_scope: CognitionBillingScope,
    pub billing_month: NaiveDate,
    pub reserved_micro_usd: u64,
}

impl PaidCognitionAuthorization {
    pub fn validate_against(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<(), CognitionContractError> {
        entry.validate()?;
        if self.request_id != entry.selection.request_id
            || self.billing_month.day() != 1
            || self.reserved_micro_usd == 0
            || self.reserved_micro_usd > MAX_PAID_COGNITION_RESERVATION_MICRO_USD
        {
            return Err(CognitionContractError::InvalidPaidAuthorization);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaidCognitionReservationDecision {
    Authorized(PaidCognitionAuthorization),
    DeniedHardStop,
}

impl CognitionJobEntry {
    pub fn validate(&self) -> Result<(), CognitionContractError> {
        self.selection
            .validate()
            .map_err(|error| CognitionContractError::InvalidJob(error.to_string()))?;
        if self.source_sequence == EventSequence::ZERO
            || self.source_event_id
                != EventId::for_position(
                    self.selection.world_id,
                    self.source_sequence.get(),
                    self.source_event_index,
                )
            || self.claim_count == 0
        {
            return Err(CognitionContractError::InvalidJob(
                "source event identity or claim count is invalid".to_owned(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
pub trait CognitionJobStore: Send + Sync {
    /// Freeze every request due in the target transition. A repeated call for the
    /// same world/sequence returns byte-identical inputs and never re-reads a model.
    async fn latch_due_cognition_inputs(
        &self,
        world_id: WorldId,
        target_sequence: EventSequence,
        target_tick: SimTick,
    ) -> Result<Vec<CognitionDeadlineInput>, crate::StoreError>;

    async fn claim_next_cognition_request(
        &self,
        worker_id: &str,
        claim_lease_seconds: u32,
    ) -> Result<Option<CognitionJobEntry>, crate::StoreError>;

    async fn reschedule_cognition_request(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        error: &str,
        retry_after_seconds: u32,
    ) -> Result<(), crate::StoreError>;

    /// Returns whether the canonical deadline has already been frozen. This is
    /// an operational state query only; workers must never use it as simulated
    /// input or attempt to replace the immutable latch.
    async fn cognition_deadline_is_latched(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<bool, crate::StoreError>;

    async fn record_cognition_recall(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        recall: &CognitionRecallRecord,
    ) -> Result<(), crate::StoreError>;

    async fn load_cognition_recall(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<Option<CognitionRecallRecord>, crate::StoreError>;

    async fn begin_cognition_route_attempt(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        route_index: u16,
        route: &CognitionModelRoute,
    ) -> Result<(), crate::StoreError>;

    async fn record_cognition_route_skip(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        attempt: &CognitionRouteAttempt,
    ) -> Result<(), crate::StoreError>;

    async fn finish_cognition_route_attempt(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        request: &ModelCognitionRequest,
        attempt: &CognitionRouteAttempt,
        receipt: Option<&ModelCognitionReceipt>,
    ) -> Result<(), crate::StoreError>;

    async fn abandon_cognition_route_attempt(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        route_index: u16,
    ) -> Result<(), crate::StoreError>;

    async fn list_cognition_route_attempts(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<Vec<CognitionRouteAttemptRecord>, crate::StoreError>;

    #[allow(clippy::too_many_arguments)]
    async fn complete_cognition_request(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        registry: &CognitionRouteRegistry,
        purpose: CognitionRoutePurpose,
        request: &ModelCognitionRequest,
        result: &ModelCognitionLadderResult,
    ) -> Result<(), crate::StoreError>;

    async fn reserve_paid_cognition(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        route: &CognitionModelRoute,
        reserved_micro_usd: u64,
    ) -> Result<PaidCognitionReservationDecision, crate::StoreError>;

    async fn load_paid_cognition_authorization(
        &self,
        entry: &CognitionJobEntry,
    ) -> Result<Option<PaidCognitionAuthorization>, crate::StoreError>;

    async fn settle_paid_cognition(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        authorization: &PaidCognitionAuthorization,
        receipt: &ModelCognitionReceipt,
    ) -> Result<(), crate::StoreError>;

    async fn release_paid_cognition(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        authorization: &PaidCognitionAuthorization,
    ) -> Result<(), crate::StoreError>;

    async fn mark_paid_cognition_indeterminate(
        &self,
        worker_id: &str,
        entry: &CognitionJobEntry,
        authorization: &PaidCognitionAuthorization,
    ) -> Result<(), crate::StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_allowlist_is_exact() {
        for route in [
            CognitionModelRoute::local_qwen2_5_1_5b(),
            CognitionModelRoute::local_gpt_oss_20b(),
            CognitionModelRoute::cloudflare_gpt_oss_20b(),
            CognitionModelRoute::cloudflare_gpt_oss_120b(),
            CognitionModelRoute::groq_gpt_oss_20b(),
            CognitionModelRoute::groq_gpt_oss_120b(),
            CognitionModelRoute::openrouter_free(),
            CognitionModelRoute::openrouter_gpt_oss_20b_free(),
            CognitionModelRoute::openrouter_gpt_oss_120b_free(),
            CognitionModelRoute::openrouter_cancer_nemotron_3_ultra_free(),
            CognitionModelRoute::cerebras_gpt_oss_120b(),
            CognitionModelRoute::cerebras_llama3_1_8b(),
            CognitionModelRoute::nvidia_nemotron_3_ultra_development(),
            CognitionModelRoute::openrouter_deepseek_v4_flash(),
            CognitionModelRoute::openrouter_cancer_deepseek_v4_pro(),
            CognitionModelRoute::openrouter_cancer_deepseek_v4_flash(),
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
            registry.routes[0],
            CognitionModelRoute::local_qwen2_5_1_5b()
        );
        assert_eq!(
            registry.validate(CognitionRoutePurpose::ProductionWorld),
            Ok(())
        );
        assert!(registry.routes.len() < MAX_COGNITION_ROUTES);

        let development = CognitionRouteRegistry::development_default();
        assert_eq!(
            development.validate(CognitionRoutePurpose::Development),
            Ok(())
        );
        assert_eq!(
            development.validate(CognitionRoutePurpose::ProductionWorld),
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

    #[test]
    fn cancer_registries_separate_free_exploration_from_paid_escalation() {
        let exploration = CognitionRouteRegistry::cancer_research_exploration();
        assert_eq!(
            exploration.validate(CognitionRoutePurpose::CancerResearchExploration),
            Ok(())
        );
        assert_eq!(
            exploration.routes,
            vec![CognitionModelRoute::openrouter_cancer_nemotron_3_ultra_free()]
        );
        let registry = CognitionRouteRegistry::cancer_research_escalation();
        assert_eq!(
            registry.validate(CognitionRoutePurpose::CancerResearchEscalation),
            Ok(())
        );
        assert_eq!(
            registry.routes,
            vec![
                CognitionModelRoute::openrouter_cancer_deepseek_v4_pro(),
                CognitionModelRoute::openrouter_cancer_deepseek_v4_flash(),
            ]
        );
        assert!(
            registry
                .routes
                .iter()
                .all(|route| route.provider == CognitionProviderId::openrouter_cancer())
        );
        assert!(matches!(
            registry.validate(CognitionRoutePurpose::ProductionWorld),
            Err(CognitionContractError::UnsupportedRoutePolicyVersion(_))
        ));
    }

    #[test]
    fn cancer_paid_routes_have_a_smaller_isolated_treasury() {
        assert_eq!(
            CognitionBillingScope::for_route(
                &CognitionModelRoute::openrouter_cancer_deepseek_v4_pro()
            ),
            CognitionBillingScope::CancerResearch
        );
        assert_eq!(
            CognitionBillingScope::for_route(&CognitionModelRoute::openrouter_deepseek_v4_flash()),
            CognitionBillingScope::Production
        );
        assert_eq!(
            CognitionBillingScope::CancerResearch.monthly_limits_micro_usd(),
            (
                CANCER_RESEARCH_TARGET_MICRO_USD_PER_MONTH,
                CANCER_RESEARCH_HARD_STOP_MICRO_USD_PER_MONTH
            )
        );
        assert!(
            CANCER_RESEARCH_HARD_STOP_MICRO_USD_PER_MONTH < COGNITION_HARD_STOP_MICRO_USD_PER_MONTH
        );
    }
}
