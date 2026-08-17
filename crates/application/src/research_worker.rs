use std::{collections::BTreeMap, sync::Arc};

use thiserror::Error;

use crate::{
    CANCER_RESEARCH_MODEL_CONTRACT_VERSION, CancerResearchAttemptPersistenceState,
    CancerResearchJobEntry, CancerResearchJobStore, CancerResearchLadderResult,
    CancerResearchModel, CancerResearchModelError, CancerResearchPaidReservationDecision,
    CognitionBillingClass, CognitionProviderId, CognitionRouteAttempt, CognitionRouteAttemptStatus,
    CognitionRouteRegistry, DEFAULT_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD,
    MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD, StoreError,
};

#[derive(Clone, Debug)]
pub struct CancerResearchWorkerConfiguration {
    pub claim_lease_seconds: u32,
    pub retry_after_seconds: u32,
    pub paid_reservation_micro_usd: u64,
    pub paid_enabled: bool,
}

impl Default for CancerResearchWorkerConfiguration {
    fn default() -> Self {
        Self {
            claim_lease_seconds: 900,
            retry_after_seconds: 30,
            paid_reservation_micro_usd: DEFAULT_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD,
            paid_enabled: false,
        }
    }
}

impl CancerResearchWorkerConfiguration {
    pub fn validate(&self) -> Result<(), CancerResearchWorkerError> {
        if self.claim_lease_seconds == 0
            || self.claim_lease_seconds > 3_600
            || self.retry_after_seconds == 0
            || self.retry_after_seconds > 3_600
            || self.paid_reservation_micro_usd == 0
            || self.paid_reservation_micro_usd > MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD
        {
            return Err(CancerResearchWorkerError::Configuration(
                "research worker lease, retry, or paid reservation is outside its bound".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CancerResearchWorkerOutcome {
    Idle,
    Completed {
        request_id: uuid::Uuid,
        succeeded: bool,
    },
}

#[derive(Debug, Error)]
pub enum CancerResearchWorkerError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("cancer-research worker configuration is invalid: {0}")]
    Configuration(String),
    #[error("cancer-research worker found corrupt durable state: {0}")]
    Corrupt(String),
}

pub type CancerResearchModelAdapters = BTreeMap<CognitionProviderId, Arc<dyn CancerResearchModel>>;

pub async fn process_next_cancer_research_job<S: CancerResearchJobStore + ?Sized>(
    store: &S,
    adapters: &CancerResearchModelAdapters,
    worker_id: &str,
    configuration: &CancerResearchWorkerConfiguration,
) -> Result<CancerResearchWorkerOutcome, CancerResearchWorkerError> {
    configuration.validate()?;
    let Some(entry) = store
        .claim_next_cancer_research_request(worker_id, configuration.claim_lease_seconds)
        .await?
    else {
        return Ok(CancerResearchWorkerOutcome::Idle);
    };
    match process_claimed_job(store, adapters, worker_id, configuration, &entry).await {
        Ok(outcome) => Ok(outcome),
        Err(error @ CancerResearchWorkerError::Store(_)) => {
            store
                .reschedule_cancer_research_request(
                    worker_id,
                    &entry,
                    &error.to_string(),
                    configuration.retry_after_seconds,
                )
                .await?;
            Err(error)
        }
        Err(error) => Err(error),
    }
}

async fn process_claimed_job<S: CancerResearchJobStore + ?Sized>(
    store: &S,
    adapters: &CancerResearchModelAdapters,
    worker_id: &str,
    configuration: &CancerResearchWorkerConfiguration,
    entry: &CancerResearchJobEntry,
) -> Result<CancerResearchWorkerOutcome, CancerResearchWorkerError> {
    entry
        .validate()
        .map_err(|error| CancerResearchWorkerError::Corrupt(error.to_string()))?;
    let registry = match entry.request.route_purpose() {
        crate::CognitionRoutePurpose::CancerResearchExploration => {
            CognitionRouteRegistry::cancer_research_exploration()
        }
        crate::CognitionRoutePurpose::CancerResearchEscalation => {
            CognitionRouteRegistry::cancer_research_escalation()
        }
        _ => {
            return Err(CancerResearchWorkerError::Corrupt(
                "research request selected a non-research route purpose".to_owned(),
            ));
        }
    };
    registry
        .validate(entry.request.route_purpose())
        .map_err(|error| CancerResearchWorkerError::Corrupt(error.to_string()))?;

    let mut records = store.list_cancer_research_route_attempts(entry).await?;
    if records.len() > registry.routes.len() {
        return Err(CancerResearchWorkerError::Corrupt(
            "durable research attempts exceed the route registry".to_owned(),
        ));
    }
    if let Some(in_flight) = records.iter().find(|record| {
        record.persistence_state == CancerResearchAttemptPersistenceState::Dispatched
    }) {
        let attempt = CognitionRouteAttempt {
            route_index: in_flight.route_index,
            provider: in_flight.route.provider.clone(),
            requested_model: in_flight.route.requested_model.clone(),
            billing_class: in_flight.route.billing_class,
            status: CognitionRouteAttemptStatus::Unavailable,
        };
        store
            .finish_cancer_research_route_attempt(worker_id, entry, &attempt, None)
            .await?;
        if in_flight.route.billing_class == CognitionBillingClass::PaidApproved
            && let Some(authorization) =
                store.load_paid_cancer_research_authorization(entry).await?
        {
            store
                .mark_paid_cancer_research_indeterminate(worker_id, entry, &authorization)
                .await?;
        }
        records = store.list_cancer_research_route_attempts(entry).await?;
        return finalize_from_records(store, worker_id, entry, &registry, &records).await;
    }

    if records.iter().any(|record| {
        record
            .attempt
            .as_ref()
            .is_some_and(|attempt| attempt.status == CognitionRouteAttemptStatus::Succeeded)
    }) {
        settle_recovered_success_if_needed(store, worker_id, entry, &records).await?;
        return finalize_from_records(store, worker_id, entry, &registry, &records).await;
    }

    let mut attempts = records
        .iter()
        .map(|record| {
            record.attempt.clone().ok_or_else(|| {
                CancerResearchWorkerError::Corrupt(
                    "completed research route omitted its attempt".to_owned(),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut paid_authorization = store.load_paid_cancer_research_authorization(entry).await?;

    for (position, route) in registry.routes.iter().enumerate().skip(attempts.len()) {
        let route_index = u16::try_from(position).map_err(|_| {
            CancerResearchWorkerError::Corrupt("research route index exceeds u16".to_owned())
        })?;
        let Some(adapter) = adapters.get(&route.provider) else {
            attempts.push(CognitionRouteAttempt {
                route_index,
                provider: route.provider.clone(),
                requested_model: route.requested_model.clone(),
                billing_class: route.billing_class,
                status: CognitionRouteAttemptStatus::SkippedUnconfigured,
            });
            continue;
        };

        if route.billing_class == CognitionBillingClass::PaidApproved && !configuration.paid_enabled
        {
            for (remaining_position, remaining) in registry.routes.iter().enumerate().skip(position)
            {
                attempts.push(CognitionRouteAttempt {
                    route_index: u16::try_from(remaining_position).map_err(|_| {
                        CancerResearchWorkerError::Corrupt(
                            "research route index exceeds u16".to_owned(),
                        )
                    })?,
                    provider: remaining.provider.clone(),
                    requested_model: remaining.requested_model.clone(),
                    billing_class: remaining.billing_class,
                    status: CognitionRouteAttemptStatus::SkippedPaidUnauthorized,
                });
            }
            return finalize_result(store, worker_id, entry, &registry, attempts, None).await;
        }
        if route.billing_class == CognitionBillingClass::PaidApproved
            && paid_authorization.is_none()
        {
            match store
                .reserve_paid_cancer_research(
                    worker_id,
                    entry,
                    route,
                    configuration.paid_reservation_micro_usd,
                )
                .await?
            {
                CancerResearchPaidReservationDecision::Authorized(authorization) => {
                    paid_authorization = Some(authorization);
                }
                CancerResearchPaidReservationDecision::DeniedHardStop => {
                    for (remaining_position, remaining) in
                        registry.routes.iter().enumerate().skip(position)
                    {
                        attempts.push(CognitionRouteAttempt {
                            route_index: u16::try_from(remaining_position).map_err(|_| {
                                CancerResearchWorkerError::Corrupt(
                                    "research route index exceeds u16".to_owned(),
                                )
                            })?,
                            provider: remaining.provider.clone(),
                            requested_model: remaining.requested_model.clone(),
                            billing_class: remaining.billing_class,
                            status: CognitionRouteAttemptStatus::SkippedPaidUnauthorized,
                        });
                    }
                    return finalize_result(store, worker_id, entry, &registry, attempts, None)
                        .await;
                }
            }
        }

        store
            .begin_cancer_research_route_attempt(worker_id, entry, route_index, route)
            .await?;
        let inference = adapter.infer_research(route, &entry.request).await;
        let (status, receipt) = normalize_model_result(inference);
        let attempt = CognitionRouteAttempt {
            route_index,
            provider: route.provider.clone(),
            requested_model: route.requested_model.clone(),
            billing_class: route.billing_class,
            status,
        };
        store
            .finish_cancer_research_route_attempt(worker_id, entry, &attempt, receipt.as_ref())
            .await?;
        attempts.push(attempt);

        if let Some(receipt) = receipt {
            if let Some(authorization) = paid_authorization.as_ref() {
                store
                    .settle_paid_cancer_research(worker_id, entry, authorization, &receipt)
                    .await?;
            }
            return finalize_result(store, worker_id, entry, &registry, attempts, Some(receipt))
                .await;
        }
        if route.billing_class == CognitionBillingClass::PaidApproved
            && matches!(
                status,
                CognitionRouteAttemptStatus::Unavailable
                    | CognitionRouteAttemptStatus::InvalidResponse
            )
        {
            if let Some(authorization) = paid_authorization.as_ref() {
                store
                    .mark_paid_cancer_research_indeterminate(worker_id, entry, authorization)
                    .await?;
            }
            return finalize_result(store, worker_id, entry, &registry, attempts, None).await;
        }
    }

    if let Some(authorization) = paid_authorization.as_ref() {
        store
            .release_paid_cancer_research(worker_id, entry, authorization)
            .await?;
    }
    finalize_result(store, worker_id, entry, &registry, attempts, None).await
}

fn normalize_model_result(
    result: Result<crate::CancerResearchModelReceipt, CancerResearchModelError>,
) -> (
    CognitionRouteAttemptStatus,
    Option<crate::CancerResearchModelReceipt>,
) {
    match result {
        Ok(receipt) => (CognitionRouteAttemptStatus::Succeeded, Some(receipt)),
        Err(error) => {
            let status = match &error {
                CancerResearchModelError::Unavailable(_) => {
                    CognitionRouteAttemptStatus::Unavailable
                }
                CancerResearchModelError::Rejected(_) => CognitionRouteAttemptStatus::Rejected,
                CancerResearchModelError::InvalidResponse(_) => {
                    CognitionRouteAttemptStatus::InvalidResponse
                }
            };
            tracing::warn!(
                error = %error,
                ?status,
                "Cancer World research model attempt did not produce a valid receipt"
            );
            (status, None)
        }
    }
}

async fn settle_recovered_success_if_needed<S: CancerResearchJobStore + ?Sized>(
    store: &S,
    worker_id: &str,
    entry: &CancerResearchJobEntry,
    records: &[crate::CancerResearchRouteAttemptRecord],
) -> Result<(), CancerResearchWorkerError> {
    let Some(success) = records.iter().find(|record| record.receipt.is_some()) else {
        return Err(CancerResearchWorkerError::Corrupt(
            "successful durable research attempt omitted its receipt".to_owned(),
        ));
    };
    let receipt = success
        .receipt
        .as_ref()
        .expect("record was selected because it contains a receipt");
    if success.route.billing_class == CognitionBillingClass::PaidApproved
        && let Some(authorization) = store.load_paid_cancer_research_authorization(entry).await?
    {
        store
            .settle_paid_cancer_research(worker_id, entry, &authorization, receipt)
            .await?;
    }
    Ok(())
}

async fn finalize_from_records<S: CancerResearchJobStore + ?Sized>(
    store: &S,
    worker_id: &str,
    entry: &CancerResearchJobEntry,
    registry: &CognitionRouteRegistry,
    records: &[crate::CancerResearchRouteAttemptRecord],
) -> Result<CancerResearchWorkerOutcome, CancerResearchWorkerError> {
    let attempts = records
        .iter()
        .map(|record| {
            record.attempt.clone().ok_or_else(|| {
                CancerResearchWorkerError::Corrupt(
                    "terminal research attempt omitted its payload".to_owned(),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let receipt = records.iter().find_map(|record| record.receipt.clone());
    finalize_result(store, worker_id, entry, registry, attempts, receipt).await
}

async fn finalize_result<S: CancerResearchJobStore + ?Sized>(
    store: &S,
    worker_id: &str,
    entry: &CancerResearchJobEntry,
    registry: &CognitionRouteRegistry,
    attempts: Vec<CognitionRouteAttempt>,
    receipt: Option<crate::CancerResearchModelReceipt>,
) -> Result<CancerResearchWorkerOutcome, CancerResearchWorkerError> {
    let result = CancerResearchLadderResult {
        contract_version: CANCER_RESEARCH_MODEL_CONTRACT_VERSION,
        request_id: entry.request.request_id,
        route_policy_version: registry.policy_version,
        route_registry_hash: registry
            .canonical_hash(entry.request.route_purpose())
            .map_err(|error| CancerResearchWorkerError::Corrupt(error.to_string()))?,
        attempts,
        receipt,
    };
    result
        .validate_against(registry, &entry.request)
        .map_err(|error| CancerResearchWorkerError::Corrupt(error.to_string()))?;
    let succeeded = result.receipt.is_some();
    store
        .complete_cancer_research_request(worker_id, entry, registry, &result)
        .await?;
    Ok(CancerResearchWorkerOutcome::Completed {
        request_id: entry.request.request_id,
        succeeded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paid_research_is_an_explicit_worker_switch() {
        let default = CancerResearchWorkerConfiguration::default();
        assert!(!default.paid_enabled);
        assert!(default.validate().is_ok());

        let invalid = CancerResearchWorkerConfiguration {
            paid_reservation_micro_usd: MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD
                .saturating_add(1),
            ..default
        };
        assert!(invalid.validate().is_err());
    }
}
