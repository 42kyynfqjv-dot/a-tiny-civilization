use serde::{Deserialize, Serialize};

use crate::{CancerResearchTarget, CanonicalHashError, Digest, EntityId, SimTick, WorldSeed};

pub const CANCER_BURDEN_SCHEMA_VERSION: u16 = 1;
pub const CANCER_BURDEN_PARTS_PER_MILLION_MAX: u32 = 1_000_000;
pub const CANCER_TERMINAL_BURDEN_PARTS_PER_MILLION: u32 = 900_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerTrajectory {
    Growing,
    Stable,
    Shrinking,
    Spreading,
    Recurring,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerBurdenState {
    pub schema_version: u16,
    pub target: CancerResearchTarget,
    pub primary_burden_parts_per_million: u32,
    pub metastatic_burden_parts_per_million: u32,
    pub clone_diversity_units: u16,
    pub immune_engagement_units: u16,
    pub trajectory: CancerTrajectory,
    pub observed_at: SimTick,
}

impl CancerBurdenState {
    pub fn seeded_initial(
        seed: WorldSeed,
        resident_id: EntityId,
        target: CancerResearchTarget,
    ) -> Result<Self, CancerBurdenError> {
        let digest = Digest::canonical(&(
            "a-tiny-civilization:cancer-burden-initial:v1",
            seed,
            resident_id,
            target,
        ))?;
        let bytes = digest.as_bytes();
        let state = Self {
            schema_version: CANCER_BURDEN_SCHEMA_VERSION,
            target,
            primary_burden_parts_per_million: 8_000
                + u32::from(u16::from_be_bytes([bytes[0], bytes[1]])) % 12_001,
            metastatic_burden_parts_per_million: 0,
            clone_diversity_units: 1 + u16::from(bytes[2] % 8),
            immune_engagement_units: u16::from_be_bytes([bytes[3], bytes[4]]) % 1_001,
            trajectory: CancerTrajectory::Growing,
            observed_at: SimTick::ZERO,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn advance_one_day(
        &self,
        seed: WorldSeed,
        resident_id: EntityId,
        day_ordinal: u32,
        observed_at: SimTick,
    ) -> Result<Self, CancerBurdenError> {
        self.validate()?;
        if observed_at <= self.observed_at || day_ordinal == 0 {
            return Err(CancerBurdenError::InvalidTransition);
        }
        let base_growth_bps = match self.target {
            CancerResearchTarget::AdultGlioblastoma => 140_u32,
            CancerResearchTarget::PancreaticDuctalAdenocarcinoma => 105_u32,
            CancerResearchTarget::ExtensiveStageSmallCellLungCancer => 185_u32,
        };
        let variation = Digest::canonical(&(
            "a-tiny-civilization:cancer-burden-daily-variation:v1",
            seed,
            resident_id,
            day_ordinal,
        ))?;
        let signed_variation = i32::from(variation.as_bytes()[0] % 41) - 20;
        let growth_bps = u32::try_from(
            i32::try_from(base_growth_bps).expect("growth bound fits i32") + signed_variation,
        )
        .map_err(|_| CancerBurdenError::Arithmetic)?;
        let primary_growth = ceil_ratio(
            u64::from(self.primary_burden_parts_per_million),
            u64::from(growth_bps),
            10_000,
        )?;
        let primary = u64::from(self.primary_burden_parts_per_million)
            .checked_add(primary_growth)
            .ok_or(CancerBurdenError::Arithmetic)?
            .min(u64::from(CANCER_BURDEN_PARTS_PER_MILLION_MAX));
        let spread_threshold = 80_000_u32;
        let metastatic_growth = if self.primary_burden_parts_per_million >= spread_threshold {
            1_u64.max(ceil_ratio(
                u64::from(self.primary_burden_parts_per_million - spread_threshold + 1),
                u64::from(growth_bps),
                50_000,
            )?)
        } else {
            0
        };
        let metastatic = u64::from(self.metastatic_burden_parts_per_million)
            .checked_add(metastatic_growth)
            .ok_or(CancerBurdenError::Arithmetic)?
            .min(u64::from(CANCER_BURDEN_PARTS_PER_MILLION_MAX));
        let next = Self {
            schema_version: CANCER_BURDEN_SCHEMA_VERSION,
            target: self.target,
            primary_burden_parts_per_million: u32::try_from(primary)
                .map_err(|_| CancerBurdenError::Arithmetic)?,
            metastatic_burden_parts_per_million: u32::try_from(metastatic)
                .map_err(|_| CancerBurdenError::Arithmetic)?,
            clone_diversity_units: self
                .clone_diversity_units
                .saturating_add(u16::from(variation.as_bytes()[1] % 17 == 0))
                .min(10_000),
            immune_engagement_units: self.immune_engagement_units,
            trajectory: if metastatic > u64::from(self.metastatic_burden_parts_per_million) {
                CancerTrajectory::Spreading
            } else {
                CancerTrajectory::Growing
            },
            observed_at,
        };
        next.validate()?;
        Ok(next)
    }

    pub fn validate(&self) -> Result<(), CancerBurdenError> {
        if self.schema_version != CANCER_BURDEN_SCHEMA_VERSION {
            return Err(CancerBurdenError::UnsupportedSchema(self.schema_version));
        }
        if self.primary_burden_parts_per_million == 0
            || self.primary_burden_parts_per_million > CANCER_BURDEN_PARTS_PER_MILLION_MAX
            || self.metastatic_burden_parts_per_million > CANCER_BURDEN_PARTS_PER_MILLION_MAX
            || self.clone_diversity_units == 0
            || self.clone_diversity_units > 10_000
            || self.immune_engagement_units > 10_000
        {
            return Err(CancerBurdenError::InvalidState);
        }
        Ok(())
    }

    #[must_use]
    pub fn total_burden_parts_per_million(&self) -> u32 {
        self.primary_burden_parts_per_million
            .saturating_add(self.metastatic_burden_parts_per_million)
            .min(CANCER_BURDEN_PARTS_PER_MILLION_MAX)
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.total_burden_parts_per_million() >= CANCER_TERMINAL_BURDEN_PARTS_PER_MILLION
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerBurdenTransition {
    pub resident_id: EntityId,
    pub from: CancerBurdenState,
    pub to: CancerBurdenState,
}

impl CancerBurdenTransition {
    pub fn validate(&self) -> Result<(), CancerBurdenError> {
        self.from.validate()?;
        self.to.validate()?;
        if self.from.target != self.to.target
            || self.from.observed_at >= self.to.observed_at
            || self.from.primary_burden_parts_per_million > self.to.primary_burden_parts_per_million
            || self.from.metastatic_burden_parts_per_million
                > self.to.metastatic_burden_parts_per_million
        {
            return Err(CancerBurdenError::InvalidTransition);
        }
        Ok(())
    }
}

fn ceil_ratio(left: u64, right: u64, denominator: u64) -> Result<u64, CancerBurdenError> {
    let product = left
        .checked_mul(right)
        .ok_or(CancerBurdenError::Arithmetic)?;
    product
        .checked_add(denominator.saturating_sub(1))
        .map(|value| value / denominator)
        .ok_or(CancerBurdenError::Arithmetic)
}

#[derive(Debug, thiserror::Error)]
pub enum CancerBurdenError {
    #[error("unsupported cancer-burden schema {0}")]
    UnsupportedSchema(u16),
    #[error("cancer-burden state is outside its fixed bounds")]
    InvalidState,
    #[error("cancer-burden transition is not monotonic or target-stable")]
    InvalidTransition,
    #[error("cancer-burden arithmetic overflowed")]
    Arithmetic,
    #[error("cancer-burden hashing failed: {0}")]
    Hash(#[from] CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldId;
    use uuid::Uuid;

    #[test]
    fn seeded_burden_and_daily_progression_are_replay_stable() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(38));
        let resident_id = EntityId::deterministic(world_id, b"affected-resident");
        let initial = CancerBurdenState::seeded_initial(
            WorldSeed::new(38),
            resident_id,
            CancerResearchTarget::AdultGlioblastoma,
        )
        .expect("initial burden");
        let first = initial
            .advance_one_day(WorldSeed::new(38), resident_id, 1, SimTick::new(288))
            .expect("daily progression");
        let repeated = initial
            .advance_one_day(WorldSeed::new(38), resident_id, 1, SimTick::new(288))
            .expect("repeated progression");
        assert_eq!(first, repeated);
        assert!(first.primary_burden_parts_per_million > initial.primary_burden_parts_per_million);
    }
}
