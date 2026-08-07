//! Species-bound reproductive physiology without agent-facing reproductive concepts.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BirthCategory, Digest, PhysiologicalEvidenceBasis, SpeciesIdentity};

pub const REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION: u16 = 1;
pub const REPRODUCTIVE_PROBABILITY_SCALE: u32 = 1_000_000;

/// Neutral internal outcome for a pending development that cannot reach birth.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReproductiveDevelopmentEnd {
    DevelopingParentUnavailable,
}

/// One compatible category pairing. Categories are engine metadata, never concepts
/// supplied to an organism or fields exposed by the safe observer projection.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ReproductiveCategoryPair {
    pub first: BirthCategory,
    pub second: BirthCategory,
    pub developing_parent: BirthCategory,
}

/// A deterministic weighted category choice for a future birth.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OffspringCategoryWeight {
    pub category: BirthCategory,
    pub weight: u32,
}

/// Immutable parameters for the first neutral reproductive-physiology driver.
/// Durations are already converted into simulation ticks by a separately reviewable
/// profile artifact; the engine never consults wall time or observer state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReproductivePhysiologyCommitment {
    pub commitment_schema_version: u16,
    pub profile_id: String,
    pub profile_digest: Digest,
    pub species: SpeciesIdentity,
    pub evidence_basis: PhysiologicalEvidenceBasis,
    pub tick_duration_seconds: u32,
    pub maturity_age_ticks: u64,
    pub development_ticks: u64,
    pub recovery_ticks: u64,
    pub opportunity_interval_ticks: u64,
    pub initiation_probability_millionths: u32,
    pub compatible_pairs: Vec<ReproductiveCategoryPair>,
    pub offspring_categories: Vec<OffspringCategoryWeight>,
}

impl ReproductivePhysiologyCommitment {
    pub fn validate(&self) -> Result<(), ReproductionError> {
        if self.commitment_schema_version != REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION {
            return Err(ReproductionError::UnsupportedCommitmentSchema);
        }
        self.species
            .validate()
            .map_err(|_| ReproductionError::InvalidCommitment)?;
        if !technical(&self.profile_id)
            || self.profile_digest == Digest::ZERO
            || self.tick_duration_seconds == 0
            || self.maturity_age_ticks == 0
            || self.development_ticks == 0
            || self.recovery_ticks == 0
            || self.opportunity_interval_ticks == 0
            || self.initiation_probability_millionths == 0
            || self.initiation_probability_millionths > REPRODUCTIVE_PROBABILITY_SCALE
            || self.compatible_pairs.is_empty()
            || self.offspring_categories.is_empty()
        {
            return Err(ReproductionError::InvalidCommitment);
        }
        if self.compatible_pairs.windows(2).any(|pair| {
            pair[0] >= pair[1]
                || (pair[0].first == pair[1].first && pair[0].second == pair[1].second)
        }) || self
            .offspring_categories
            .windows(2)
            .any(|pair| pair[0] >= pair[1] || pair[0].category == pair[1].category)
        {
            return Err(ReproductionError::NonCanonicalOrder);
        }
        for pair in &self.compatible_pairs {
            if pair.first > pair.second
                || (pair.developing_parent != pair.first && pair.developing_parent != pair.second)
            {
                return Err(ReproductionError::InvalidPair);
            }
        }
        let mut total_weight = 0_u64;
        for category in &self.offspring_categories {
            if category.weight == 0 || !self.supports_category(&category.category) {
                return Err(ReproductionError::InvalidOffspringWeight);
            }
            total_weight = total_weight
                .checked_add(u64::from(category.weight))
                .ok_or(ReproductionError::InvalidOffspringWeight)?;
        }
        if total_weight == 0 {
            return Err(ReproductionError::InvalidOffspringWeight);
        }
        Ok(())
    }

    #[must_use]
    pub fn supports_category(&self, category: &BirthCategory) -> bool {
        self.compatible_pairs.iter().any(|pair| {
            pair.first == *category
                || pair.second == *category
                || pair.developing_parent == *category
        })
    }
}

fn technical(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ReproductionError {
    #[error("unsupported reproductive-physiology commitment schema")]
    UnsupportedCommitmentSchema,
    #[error("invalid reproductive-physiology commitment")]
    InvalidCommitment,
    #[error("reproductive categories must be strictly canonical")]
    NonCanonicalOrder,
    #[error("invalid reproductive category pairing")]
    InvalidPair,
    #[error("invalid offspring category weight")]
    InvalidOffspringWeight,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn category(value: &str) -> BirthCategory {
        BirthCategory::new(value).expect("category")
    }

    fn commitment() -> ReproductivePhysiologyCommitment {
        ReproductivePhysiologyCommitment {
            commitment_schema_version: REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION,
            profile_id: "test-human-reproduction-v1".to_owned(),
            profile_digest: Digest::sha256(b"test reproduction profile"),
            species: SpeciesIdentity::new(
                "gbif",
                "2436436",
                "Homo sapiens",
                "https://www.gbif.org/species/2436436",
            )
            .expect("species"),
            evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
            tick_duration_seconds: 300,
            maturity_age_ticks: 10,
            development_ticks: 3,
            recovery_ticks: 4,
            opportunity_interval_ticks: 1,
            initiation_probability_millionths: REPRODUCTIVE_PROBABILITY_SCALE,
            compatible_pairs: vec![ReproductiveCategoryPair {
                first: category("female"),
                second: category("male"),
                developing_parent: category("female"),
            }],
            offspring_categories: vec![
                OffspringCategoryWeight {
                    category: category("female"),
                    weight: 1,
                },
                OffspringCategoryWeight {
                    category: category("male"),
                    weight: 1,
                },
            ],
        }
    }

    #[test]
    fn commitment_is_species_bound_ordered_and_nonzero() {
        commitment().validate().expect("valid commitment");
        let mut invalid = commitment();
        invalid.compatible_pairs[0].developing_parent = category("unspecified");
        assert_eq!(invalid.validate(), Err(ReproductionError::InvalidPair));
        let mut invalid = commitment();
        invalid.offspring_categories[0].weight = 0;
        assert_eq!(
            invalid.validate(),
            Err(ReproductionError::InvalidOffspringWeight)
        );
        let mut invalid = commitment();
        invalid.offspring_categories.insert(
            1,
            OffspringCategoryWeight {
                category: category("female"),
                weight: 2,
            },
        );
        assert_eq!(
            invalid.validate(),
            Err(ReproductionError::NonCanonicalOrder)
        );
    }
}
