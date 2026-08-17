use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CANCER_TISSUE_REFINEMENT_METHOD_VERSION, CancerTissueRefinementProtocol,
    CancerTissueRefinementResult, Digest, WorldId,
};

use crate::{
    CancerTissueRefinementCandidate, CancerTissueRefinementError, StoreError,
    execute_cancer_tissue_refinement, prepare_cancer_tissue_refinement_protocol,
};

pub const MAX_CANCER_TISSUE_REFINEMENT_WORKER_ID_BYTES: usize = 128;
pub const MIN_CANCER_TISSUE_REFINEMENT_LEASE_SECONDS: u32 = 30;
pub const MAX_CANCER_TISSUE_REFINEMENT_LEASE_SECONDS: u32 = 3_600;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancerTissueRefinementJob {
    pub worker_id: String,
    pub claim_token: Uuid,
    pub claim_count: u32,
    pub candidate: CancerTissueRefinementCandidate,
    pub protocol: CancerTissueRefinementProtocol,
}

impl CancerTissueRefinementJob {
    pub fn validate(&self) -> Result<(), CancerTissueRefinementWorkerError> {
        validate_tissue_worker_id(&self.worker_id)?;
        self.candidate.validate_survivor()?;
        let expected = prepare_cancer_tissue_refinement_protocol(&self.candidate)?;
        if self.claim_token.is_nil()
            || self.claim_count == 0
            || self.protocol != expected
            || self.protocol.method_version != CANCER_TISSUE_REFINEMENT_METHOD_VERSION
        {
            return Err(CancerTissueRefinementWorkerError::InvalidClaim);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CancerTissueRefinementWorkerStep {
    Idle,
    Completed {
        refinement_id: Uuid,
        result_hash: Digest,
    },
}

#[async_trait]
pub trait CancerTissueRefinementJobStore: Send + Sync {
    /// Atomically admits an eligible survivor, freezes its exact protocol, and
    /// claims at most one job globally. A durable implementation must reject a
    /// second live claim even when another world contains eligible work.
    async fn claim_next_cancer_tissue_refinement(
        &self,
        world_id: WorldId,
        worker_id: &str,
        lease_seconds: u32,
    ) -> Result<Option<CancerTissueRefinementJob>, StoreError>;

    /// Appends the exact result and releases the singleton lease atomically.
    /// Repeating an identical completion is a read-only success; conflicting
    /// bytes are corruption.
    async fn complete_cancer_tissue_refinement(
        &self,
        job: &CancerTissueRefinementJob,
        result: &CancerTissueRefinementResult,
    ) -> Result<(), StoreError>;

    /// Releases a held claim after a local execution failure. Protocol and
    /// claim history remain durable; only operational lease fields change.
    async fn fail_cancer_tissue_refinement(
        &self,
        job: &CancerTissueRefinementJob,
        error: &str,
        retry_after_seconds: u32,
    ) -> Result<(), StoreError>;
}

pub async fn process_next_cancer_tissue_refinement<S>(
    store: &S,
    world_id: WorldId,
    worker_id: &str,
    lease_seconds: u32,
) -> Result<CancerTissueRefinementWorkerStep, CancerTissueRefinementWorkerError>
where
    S: CancerTissueRefinementJobStore,
{
    validate_tissue_worker_id(worker_id)?;
    if !(MIN_CANCER_TISSUE_REFINEMENT_LEASE_SECONDS..=MAX_CANCER_TISSUE_REFINEMENT_LEASE_SECONDS)
        .contains(&lease_seconds)
    {
        return Err(CancerTissueRefinementWorkerError::InvalidConfiguration);
    }
    let Some(job) = store
        .claim_next_cancer_tissue_refinement(world_id, worker_id, lease_seconds)
        .await?
    else {
        return Ok(CancerTissueRefinementWorkerStep::Idle);
    };
    job.validate()?;
    let result = match execute_cancer_tissue_refinement(&job.candidate, &job.protocol) {
        Ok(result) => result,
        Err(error) => {
            let retry_after = retry_delay_seconds(job.claim_count);
            store
                .fail_cancer_tissue_refinement(&job, &error.to_string(), retry_after)
                .await?;
            return Err(error.into());
        }
    };
    let result_hash = result.canonical_hash(&job.protocol)?;
    store
        .complete_cancer_tissue_refinement(&job, &result)
        .await?;
    Ok(CancerTissueRefinementWorkerStep::Completed {
        refinement_id: result.refinement_id,
        result_hash,
    })
}

pub fn validate_tissue_worker_id(worker_id: &str) -> Result<(), CancerTissueRefinementWorkerError> {
    if worker_id.trim() != worker_id
        || worker_id.is_empty()
        || worker_id.len() > MAX_CANCER_TISSUE_REFINEMENT_WORKER_ID_BYTES
    {
        Err(CancerTissueRefinementWorkerError::InvalidConfiguration)
    } else {
        Ok(())
    }
}

fn retry_delay_seconds(claim_count: u32) -> u32 {
    let shift = claim_count.saturating_sub(1).min(8);
    (1_u32 << shift).min(300)
}

#[derive(Debug, Error)]
pub enum CancerTissueRefinementWorkerError {
    #[error("the tissue-refinement worker configuration is invalid")]
    InvalidConfiguration,
    #[error("the tissue-refinement job claim is invalid")]
    InvalidClaim,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Tissue(#[from] CancerTissueRefinementError),
    #[error(transparent)]
    TissueContract(#[from] world_domain::CancerTissueRefinementContractError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct IdleStore;

    #[async_trait]
    impl CancerTissueRefinementJobStore for IdleStore {
        async fn claim_next_cancer_tissue_refinement(
            &self,
            _world_id: WorldId,
            _worker_id: &str,
            _lease_seconds: u32,
        ) -> Result<Option<CancerTissueRefinementJob>, StoreError> {
            Ok(None)
        }

        async fn complete_cancer_tissue_refinement(
            &self,
            _job: &CancerTissueRefinementJob,
            _result: &CancerTissueRefinementResult,
        ) -> Result<(), StoreError> {
            panic!("idle store cannot complete")
        }

        async fn fail_cancer_tissue_refinement(
            &self,
            _job: &CancerTissueRefinementJob,
            _error: &str,
            _retry_after_seconds: u32,
        ) -> Result<(), StoreError> {
            panic!("idle store cannot fail")
        }
    }

    #[tokio::test]
    async fn idle_is_explicit_and_does_not_write() {
        let step = process_next_cancer_tissue_refinement(
            &IdleStore,
            WorldId::from_uuid(Uuid::from_u128(1)),
            "tissue-worker-test",
            300,
        )
        .await
        .expect("idle");
        assert_eq!(step, CancerTissueRefinementWorkerStep::Idle);
    }

    #[tokio::test]
    async fn invalid_configuration_fails_before_store_access() {
        assert!(matches!(
            process_next_cancer_tissue_refinement(
                &IdleStore,
                WorldId::from_uuid(Uuid::from_u128(1)),
                " tissue-worker-test",
                300,
            )
            .await,
            Err(CancerTissueRefinementWorkerError::InvalidConfiguration)
        ));
        assert!(matches!(
            process_next_cancer_tissue_refinement(
                &IdleStore,
                WorldId::from_uuid(Uuid::from_u128(1)),
                "tissue-worker-test",
                1,
            )
            .await,
            Err(CancerTissueRefinementWorkerError::InvalidConfiguration)
        ));
    }

    #[test]
    fn retry_delay_is_bounded() {
        let observed = Mutex::new(Vec::new());
        for claim_count in 1..=1_000 {
            observed
                .lock()
                .expect("test mutex")
                .push(retry_delay_seconds(claim_count));
        }
        let observed = observed.into_inner().expect("test mutex");
        assert_eq!(observed[0], 1);
        assert_eq!(*observed.last().expect("last"), 256);
        assert!(observed.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(observed.iter().all(|delay| *delay <= 300));
    }
}
