use std::{collections::BTreeMap, sync::Arc};

use thiserror::Error;
use uuid::Uuid;

use crate::{
    AgentMemory, COGNITION_MODEL_CONTRACT_VERSION, CognitionAttemptPersistenceState,
    CognitionBillingClass, CognitionContractError, CognitionJobEntry, CognitionJobStore,
    CognitionModel, CognitionModelError, CognitionProviderId, CognitionRecallRecord,
    CognitionRouteAttempt, CognitionRouteAttemptRecord, CognitionRouteAttemptStatus,
    CognitionRoutePurpose, CognitionRouteRegistry, MAX_PAID_COGNITION_RESERVATION_MICRO_USD,
    MemoryRecallOutcome, MemoryRecallRequest, ModelCognitionLadderResult, ModelCognitionRequest,
    PaidCognitionReservationDecision, RecallUnavailableReason, StoreError,
};

pub const DEFAULT_COGNITION_NETWORK_ATTEMPT_LIMIT: u16 = 16;

#[derive(Clone, Debug)]
pub struct CognitionWorkerConfiguration {
    pub registry: CognitionRouteRegistry,
    pub purpose: CognitionRoutePurpose,
    pub max_network_attempts: u16,
    pub paid_enabled: bool,
    pub paid_reservation_micro_usd: u64,
}

impl CognitionWorkerConfiguration {
    #[must_use]
    pub fn production(paid_enabled: bool) -> Self {
        Self {
            registry: CognitionRouteRegistry::production_default(),
            purpose: CognitionRoutePurpose::ProductionWorld,
            max_network_attempts: DEFAULT_COGNITION_NETWORK_ATTEMPT_LIMIT,
            paid_enabled,
            paid_reservation_micro_usd: MAX_PAID_COGNITION_RESERVATION_MICRO_USD,
        }
    }

    pub fn validate(&self) -> Result<(), CognitionWorkerError> {
        self.registry
            .validate(self.purpose)
            .map_err(CognitionWorkerError::Contract)?;
        if self.max_network_attempts == 0
            || self.max_network_attempts > DEFAULT_COGNITION_NETWORK_ATTEMPT_LIMIT
            || self.paid_reservation_micro_usd == 0
            || self.paid_reservation_micro_usd > MAX_PAID_COGNITION_RESERVATION_MICRO_USD
        {
            return Err(CognitionWorkerError::InvalidConfiguration);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CognitionWorkerStep {
    Idle,
    Completed { request_id: Uuid, used_model: bool },
    DeadlineElapsed { request_id: Uuid },
}

pub async fn process_next_cognition_job<S, M>(
    store: &S,
    memory: &M,
    adapters: &BTreeMap<CognitionProviderId, Arc<dyn CognitionModel>>,
    worker_id: &str,
    claim_lease_seconds: u32,
    configuration: &CognitionWorkerConfiguration,
) -> Result<CognitionWorkerStep, CognitionWorkerError>
where
    S: CognitionJobStore + ?Sized,
    M: AgentMemory + ?Sized,
{
    configuration.validate()?;
    let Some(entry) = store
        .claim_next_cognition_request(worker_id, claim_lease_seconds)
        .await?
    else {
        return Ok(CognitionWorkerStep::Idle);
    };
    match process_claimed_cognition_job(store, memory, adapters, worker_id, &entry, configuration)
        .await
    {
        Ok(step) => Ok(step),
        Err(error) => {
            if store.cognition_deadline_is_latched(&entry).await? {
                finalize_after_deadline(store, worker_id, &entry).await?;
                return Ok(CognitionWorkerStep::DeadlineElapsed {
                    request_id: entry.selection.request_id,
                });
            }
            let _ = store
                .reschedule_cognition_request(worker_id, &entry, &error.to_string(), 1)
                .await;
            Err(error)
        }
    }
}

async fn finalize_after_deadline<S: CognitionJobStore + ?Sized>(
    store: &S,
    worker_id: &str,
    entry: &CognitionJobEntry,
) -> Result<(), CognitionWorkerError> {
    let records = store.list_cognition_route_attempts(entry).await?;
    let in_flight = records
        .last()
        .filter(|record| record.persistence_state == CognitionAttemptPersistenceState::Dispatched);
    if let Some(authorization) = store.load_paid_cognition_authorization(entry).await? {
        if in_flight
            .is_some_and(|record| record.route.billing_class == CognitionBillingClass::PaidApproved)
        {
            store
                .mark_paid_cognition_indeterminate(worker_id, entry, &authorization)
                .await?;
        } else {
            store
                .release_paid_cognition(worker_id, entry, &authorization)
                .await?;
        }
    }
    if let Some(in_flight) = in_flight {
        store
            .abandon_cognition_route_attempt(worker_id, entry, in_flight.route_index)
            .await?;
    }
    Ok(())
}

async fn process_claimed_cognition_job<S, M>(
    store: &S,
    memory: &M,
    adapters: &BTreeMap<CognitionProviderId, Arc<dyn CognitionModel>>,
    worker_id: &str,
    entry: &CognitionJobEntry,
    configuration: &CognitionWorkerConfiguration,
) -> Result<CognitionWorkerStep, CognitionWorkerError>
where
    S: CognitionJobStore + ?Sized,
    M: AgentMemory + ?Sized,
{
    entry.validate().map_err(CognitionWorkerError::Contract)?;
    let recall = prepare_recall(store, memory, worker_id, entry).await?;
    let request =
        ModelCognitionRequest::from_selection(&entry.selection, recall.admitted_memories.clone())
            .map_err(CognitionWorkerError::Contract)?;

    recover_interrupted_attempt(store, worker_id, entry).await?;
    reconcile_paid_reservation(store, worker_id, entry).await?;

    loop {
        let records = store.list_cognition_route_attempts(entry).await?;
        validate_attempt_prefix(&records, &configuration.registry)?;
        if prefix_has_terminated(&records) || records.len() == configuration.registry.routes.len() {
            return complete_from_records(
                store,
                worker_id,
                entry,
                configuration,
                &request,
                records,
            )
            .await;
        }

        let route_index =
            u16::try_from(records.len()).map_err(|_| CognitionWorkerError::InvalidAttemptPrefix)?;
        let route = &configuration.registry.routes[records.len()];
        let network_attempts = records
            .iter()
            .filter(|record| record.persistence_state != CognitionAttemptPersistenceState::Skipped)
            .count();
        if network_attempts >= usize::from(configuration.max_network_attempts) {
            store
                .record_cognition_route_skip(
                    worker_id,
                    entry,
                    &attempt(
                        route_index,
                        route,
                        CognitionRouteAttemptStatus::StoppedAttemptLimit,
                    ),
                )
                .await?;
            continue;
        }

        let Some(adapter) = adapters.get(&route.provider) else {
            if route.billing_class == CognitionBillingClass::PaidApproved
                && let Some(authorization) = store.load_paid_cognition_authorization(entry).await?
            {
                store
                    .release_paid_cognition(worker_id, entry, &authorization)
                    .await?;
            }
            store
                .record_cognition_route_skip(
                    worker_id,
                    entry,
                    &attempt(
                        route_index,
                        route,
                        CognitionRouteAttemptStatus::SkippedUnconfigured,
                    ),
                )
                .await?;
            continue;
        };

        let authorization = if route.billing_class == CognitionBillingClass::PaidApproved {
            if !configuration.paid_enabled {
                if let Some(authorization) = store.load_paid_cognition_authorization(entry).await? {
                    store
                        .release_paid_cognition(worker_id, entry, &authorization)
                        .await?;
                }
                store
                    .record_cognition_route_skip(
                        worker_id,
                        entry,
                        &attempt(
                            route_index,
                            route,
                            CognitionRouteAttemptStatus::SkippedPaidUnauthorized,
                        ),
                    )
                    .await?;
                continue;
            }
            match store
                .reserve_paid_cognition(
                    worker_id,
                    entry,
                    route,
                    configuration.paid_reservation_micro_usd,
                )
                .await?
            {
                PaidCognitionReservationDecision::Authorized(authorization) => Some(authorization),
                PaidCognitionReservationDecision::DeniedHardStop => {
                    store
                        .record_cognition_route_skip(
                            worker_id,
                            entry,
                            &attempt(
                                route_index,
                                route,
                                CognitionRouteAttemptStatus::SkippedPaidUnauthorized,
                            ),
                        )
                        .await?;
                    continue;
                }
            }
        } else {
            None
        };

        // This durable row is committed before the request leaves the process.
        store
            .begin_cognition_route_attempt(worker_id, entry, route_index, route)
            .await?;
        match adapter.infer(route, &request).await {
            Ok(receipt) => {
                store
                    .finish_cognition_route_attempt(
                        worker_id,
                        entry,
                        &request,
                        &attempt(route_index, route, CognitionRouteAttemptStatus::Succeeded),
                        Some(&receipt),
                    )
                    .await?;
                if let Some(authorization) = authorization {
                    store
                        .settle_paid_cognition(worker_id, entry, &authorization, &receipt)
                        .await?;
                }
            }
            Err(error) => {
                let status = match error {
                    CognitionModelError::Unavailable(_) => CognitionRouteAttemptStatus::Unavailable,
                    CognitionModelError::Rejected(_) => CognitionRouteAttemptStatus::Rejected,
                    CognitionModelError::InvalidResponse(_) => {
                        CognitionRouteAttemptStatus::InvalidResponse
                    }
                };
                store
                    .finish_cognition_route_attempt(
                        worker_id,
                        entry,
                        &request,
                        &attempt(route_index, route, status),
                        None,
                    )
                    .await?;
                if let Some(authorization) = authorization {
                    store
                        .mark_paid_cognition_indeterminate(worker_id, entry, &authorization)
                        .await?;
                }
            }
        }
    }
}

async fn prepare_recall<S, M>(
    store: &S,
    memory: &M,
    worker_id: &str,
    entry: &CognitionJobEntry,
) -> Result<CognitionRecallRecord, CognitionWorkerError>
where
    S: CognitionJobStore + ?Sized,
    M: AgentMemory + ?Sized,
{
    if let Some(recorded) = store.load_cognition_recall(entry).await? {
        return Ok(recorded);
    }
    let request = MemoryRecallRequest::from_cognition_selection(&entry.selection)
        .map_err(|error| CognitionWorkerError::Memory(error.to_string()))?;
    let outcome = memory.recall(&request).await;
    let candidate =
        CognitionRecallRecord::from_outcome(&entry.selection, outcome).unwrap_or_else(|_| {
            unavailable_recall(&entry.selection, RecallUnavailableReason::InvalidResponse)
        });
    match store
        .record_cognition_recall(worker_id, entry, &candidate)
        .await
    {
        Ok(()) => Ok(candidate),
        Err(StoreError::Conflict(_)) => {
            if let Some(recorded) = store.load_cognition_recall(entry).await? {
                return Ok(recorded);
            }
            let fallback =
                unavailable_recall(&entry.selection, RecallUnavailableReason::InvalidResponse);
            store
                .record_cognition_recall(worker_id, entry, &fallback)
                .await?;
            Ok(fallback)
        }
        Err(error) => Err(error.into()),
    }
}

fn unavailable_recall(
    selection: &world_domain::CognitionRequestSelection,
    reason: RecallUnavailableReason,
) -> CognitionRecallRecord {
    let request = MemoryRecallRequest::from_cognition_selection(selection)
        .expect("a claimed cognition selection was already validated");
    let outcome = MemoryRecallOutcome::unavailable(&request, reason)
        .expect("a validated recall request accepts an unavailable outcome");
    CognitionRecallRecord::from_outcome(selection, outcome)
        .expect("an unavailable outcome has no recalled memory payload")
}

async fn recover_interrupted_attempt<S: CognitionJobStore + ?Sized>(
    store: &S,
    worker_id: &str,
    entry: &CognitionJobEntry,
) -> Result<(), CognitionWorkerError> {
    let records = store.list_cognition_route_attempts(entry).await?;
    let Some(in_flight) = records
        .last()
        .filter(|record| record.persistence_state == CognitionAttemptPersistenceState::Dispatched)
    else {
        return Ok(());
    };
    if in_flight.route.billing_class == CognitionBillingClass::PaidApproved
        && let Some(authorization) = store.load_paid_cognition_authorization(entry).await?
    {
        store
            .mark_paid_cognition_indeterminate(worker_id, entry, &authorization)
            .await?;
    }
    store
        .abandon_cognition_route_attempt(worker_id, entry, in_flight.route_index)
        .await?;
    Ok(())
}

async fn reconcile_paid_reservation<S: CognitionJobStore + ?Sized>(
    store: &S,
    worker_id: &str,
    entry: &CognitionJobEntry,
) -> Result<(), CognitionWorkerError> {
    let Some(authorization) = store.load_paid_cognition_authorization(entry).await? else {
        return Ok(());
    };
    let records = store.list_cognition_route_attempts(entry).await?;
    let paid = records.iter().find(|record| {
        record.route.billing_class == CognitionBillingClass::PaidApproved
            && record.persistence_state != CognitionAttemptPersistenceState::Skipped
    });
    match paid {
        Some(record) if record.receipt.is_some() => {
            store
                .settle_paid_cognition(
                    worker_id,
                    entry,
                    &authorization,
                    record.receipt.as_ref().expect("receipt presence checked"),
                )
                .await?;
        }
        Some(_) => {
            store
                .mark_paid_cognition_indeterminate(worker_id, entry, &authorization)
                .await?;
        }
        None => {}
    }
    Ok(())
}

fn validate_attempt_prefix(
    records: &[CognitionRouteAttemptRecord],
    registry: &CognitionRouteRegistry,
) -> Result<(), CognitionWorkerError> {
    for (index, record) in records.iter().enumerate() {
        let Some(route) = registry.routes.get(index) else {
            return Err(CognitionWorkerError::InvalidAttemptPrefix);
        };
        if usize::from(record.route_index) != index
            || &record.route != route
            || record.persistence_state == CognitionAttemptPersistenceState::Dispatched
        {
            return Err(CognitionWorkerError::InvalidAttemptPrefix);
        }
        record.validate().map_err(CognitionWorkerError::Contract)?;
    }
    Ok(())
}

fn prefix_has_terminated(records: &[CognitionRouteAttemptRecord]) -> bool {
    records.last().is_some_and(|record| {
        record.attempt.as_ref().is_some_and(|attempt| {
            matches!(
                attempt.status,
                CognitionRouteAttemptStatus::Succeeded
                    | CognitionRouteAttemptStatus::StoppedAttemptLimit
            )
        })
    })
}

async fn complete_from_records<S: CognitionJobStore + ?Sized>(
    store: &S,
    worker_id: &str,
    entry: &CognitionJobEntry,
    configuration: &CognitionWorkerConfiguration,
    request: &ModelCognitionRequest,
    records: Vec<CognitionRouteAttemptRecord>,
) -> Result<CognitionWorkerStep, CognitionWorkerError> {
    let attempts = records
        .iter()
        .map(|record| {
            record
                .attempt
                .clone()
                .ok_or(CognitionWorkerError::InvalidAttemptPrefix)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let receipt = records
        .iter()
        .find_map(|record| record.receipt.as_ref())
        .cloned();
    let result = ModelCognitionLadderResult {
        contract_version: COGNITION_MODEL_CONTRACT_VERSION,
        request_id: request.request_id,
        route_policy_version: configuration.registry.policy_version,
        route_registry_hash: configuration
            .registry
            .canonical_hash(configuration.purpose)
            .map_err(CognitionWorkerError::Contract)?,
        attempts,
        receipt,
    };
    result
        .validate_against(&configuration.registry, configuration.purpose, request)
        .map_err(CognitionWorkerError::Contract)?;
    store
        .complete_cognition_request(
            worker_id,
            entry,
            &configuration.registry,
            configuration.purpose,
            request,
            &result,
        )
        .await?;
    Ok(CognitionWorkerStep::Completed {
        request_id: request.request_id,
        used_model: result.receipt.is_some(),
    })
}

fn attempt(
    route_index: u16,
    route: &crate::CognitionModelRoute,
    status: CognitionRouteAttemptStatus,
) -> CognitionRouteAttempt {
    CognitionRouteAttempt {
        route_index,
        provider: route.provider.clone(),
        requested_model: route.requested_model.clone(),
        billing_class: route.billing_class,
        status,
    }
}

#[derive(Debug, Error)]
pub enum CognitionWorkerError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("cognition worker contract failed: {0}")]
    Contract(#[source] CognitionContractError),
    #[error("cognition worker memory boundary failed: {0}")]
    Memory(String),
    #[error("cognition worker configuration is outside its fixed bounds")]
    InvalidConfiguration,
    #[error("durable cognition attempts are not an exact terminal registry prefix")]
    InvalidAttemptPrefix,
}
