//! Bounded, species-bound heritable dispositions.
//!
//! This is an explicit abstraction of inherited individual variation, not a molecular
//! genome model. Values bias only the existing use-neutral motor grammar. They cannot
//! contain memories, learned policies, cultural concepts, object labels, or goals.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Digest, PhysiologicalEvidenceBasis, PrimitiveActionKind, SimTick, SpeciesIdentity};

pub const HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION: u16 = 1;
pub const HERITABLE_DISPOSITION_SCHEMA_VERSION: u16 = 1;
pub const HERITABLE_PROBABILITY_SCALE: u32 = 1_000_000;
pub const MAX_HERITABLE_ACTION_WEIGHT_RATIO: u32 = 64;
pub const MAX_HERITABLE_MUTATION_PROBABILITY_MILLIONTHS: u32 = 250_000;

pub const HERITABLE_ACTION_KINDS: [PrimitiveActionKind; 11] = [
    PrimitiveActionKind::Move,
    PrimitiveActionKind::Orient,
    PrimitiveActionKind::Reach,
    PrimitiveActionKind::Grasp,
    PrimitiveActionKind::Release,
    PrimitiveActionKind::ApplyForce,
    PrimitiveActionKind::Bite,
    PrimitiveActionKind::Chew,
    PrimitiveActionKind::Swallow,
    PrimitiveActionKind::Rest,
    PrimitiveActionKind::EmitSignal,
];

/// Immutable species-level rules for founder variation and offspring mixing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeritableDispositionProfile {
    pub profile_schema_version: u16,
    pub profile_id: String,
    pub profile_digest: Digest,
    pub species: SpeciesIdentity,
    pub evidence_basis: PhysiologicalEvidenceBasis,
    pub minimum_action_weight: u16,
    pub neutral_action_weight: u16,
    pub maximum_action_weight: u16,
    pub founder_variation_steps: u16,
    pub mutation_probability_millionths: u32,
    pub mutation_max_step: u16,
}

impl HeritableDispositionProfile {
    pub fn validate(&self) -> Result<(), HeredityError> {
        if self.profile_schema_version != HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION {
            return Err(HeredityError::UnsupportedProfileSchema);
        }
        self.species
            .validate()
            .map_err(|_| HeredityError::InvalidProfile)?;
        if !is_technical(&self.profile_id)
            || self.profile_digest == Digest::ZERO
            || self.minimum_action_weight == 0
            || self.minimum_action_weight > self.neutral_action_weight
            || self.neutral_action_weight > self.maximum_action_weight
            || self.founder_variation_steps == 0
            || self.founder_variation_steps
                > self
                    .neutral_action_weight
                    .saturating_sub(self.minimum_action_weight)
            || self.founder_variation_steps
                > self
                    .maximum_action_weight
                    .saturating_sub(self.neutral_action_weight)
            || self.mutation_probability_millionths == 0
            || self.mutation_probability_millionths > MAX_HERITABLE_MUTATION_PROBABILITY_MILLIONTHS
            || self.mutation_max_step == 0
            || self.mutation_max_step
                > self
                    .maximum_action_weight
                    .saturating_sub(self.minimum_action_weight)
            || u32::from(self.maximum_action_weight)
                > u32::from(self.minimum_action_weight)
                    .saturating_mul(MAX_HERITABLE_ACTION_WEIGHT_RATIO)
            || u32::from(self.mutation_max_step).saturating_mul(4)
                > u32::from(
                    self.maximum_action_weight
                        .saturating_sub(self.minimum_action_weight),
                )
        {
            return Err(HeredityError::InvalidProfile);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeritableActionWeight {
    pub action_kind: PrimitiveActionKind,
    pub weight: u16,
}

/// One organism's immutable inherited motor dispositions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeritableDisposition {
    pub disposition_schema_version: u16,
    pub profile_digest: Digest,
    pub generation: u64,
    pub derived_at: SimTick,
    pub action_weights: Vec<HeritableActionWeight>,
}

impl HeritableDisposition {
    pub fn validate_against(
        &self,
        profile: &HeritableDispositionProfile,
    ) -> Result<(), HeredityError> {
        profile.validate()?;
        if self.disposition_schema_version != HERITABLE_DISPOSITION_SCHEMA_VERSION {
            return Err(HeredityError::UnsupportedDispositionSchema);
        }
        if self.profile_digest != profile.profile_digest
            || self.action_weights.len() != HERITABLE_ACTION_KINDS.len()
        {
            return Err(HeredityError::InvalidDisposition);
        }
        for (weight, expected_kind) in self.action_weights.iter().zip(HERITABLE_ACTION_KINDS) {
            if weight.action_kind != expected_kind
                || !(profile.minimum_action_weight..=profile.maximum_action_weight)
                    .contains(&weight.weight)
            {
                return Err(HeredityError::InvalidDisposition);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn action_weight(&self, action_kind: PrimitiveActionKind) -> Option<u16> {
        self.action_weights
            .binary_search_by_key(&action_kind, |entry| entry.action_kind)
            .ok()
            .map(|index| self.action_weights[index].weight)
    }
}

fn is_technical(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HeredityError {
    #[error("unsupported heritable-disposition profile schema")]
    UnsupportedProfileSchema,
    #[error("invalid heritable-disposition profile")]
    InvalidProfile,
    #[error("unsupported heritable-disposition schema")]
    UnsupportedDispositionSchema,
    #[error("invalid heritable disposition")]
    InvalidDisposition,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn species() -> SpeciesIdentity {
        SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("species")
    }

    fn profile() -> HeritableDispositionProfile {
        HeritableDispositionProfile {
            profile_schema_version: HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
            profile_id: "experimental-heritable-disposition-v1".to_owned(),
            profile_digest: Digest::sha256(b"experimental heredity assumptions"),
            species: species(),
            evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
            minimum_action_weight: 1,
            neutral_action_weight: 16,
            maximum_action_weight: 31,
            founder_variation_steps: 4,
            mutation_probability_millionths: 100_000,
            mutation_max_step: 2,
        }
    }

    fn disposition(profile: &HeritableDispositionProfile) -> HeritableDisposition {
        HeritableDisposition {
            disposition_schema_version: HERITABLE_DISPOSITION_SCHEMA_VERSION,
            profile_digest: profile.profile_digest,
            generation: 0,
            derived_at: SimTick::ZERO,
            action_weights: HERITABLE_ACTION_KINDS
                .into_iter()
                .map(|action_kind| HeritableActionWeight {
                    action_kind,
                    weight: profile.neutral_action_weight,
                })
                .collect(),
        }
    }

    #[test]
    fn profile_and_disposition_are_species_bound_bounded_and_canonical() {
        let profile = profile();
        let disposition = disposition(&profile);
        assert_eq!(disposition.validate_against(&profile), Ok(()));
        assert_eq!(
            disposition.action_weight(PrimitiveActionKind::EmitSignal),
            Some(profile.neutral_action_weight)
        );

        let mut reordered = disposition.clone();
        reordered.action_weights.swap(0, 1);
        assert_eq!(
            reordered.validate_against(&profile),
            Err(HeredityError::InvalidDisposition)
        );

        let mut out_of_range = disposition;
        out_of_range.action_weights[0].weight = 0;
        assert_eq!(
            out_of_range.validate_against(&profile),
            Err(HeredityError::InvalidDisposition)
        );
    }

    #[test]
    fn probability_and_symmetric_founder_ranges_fail_closed() {
        let mut invalid = profile();
        invalid.founder_variation_steps = 16;
        assert_eq!(invalid.validate(), Err(HeredityError::InvalidProfile));

        let mut inconsistent_mutation = profile();
        inconsistent_mutation.mutation_probability_millionths = 0;
        assert_eq!(
            inconsistent_mutation.validate(),
            Err(HeredityError::InvalidProfile)
        );

        let mut no_founder_variation = profile();
        no_founder_variation.founder_variation_steps = 0;
        assert_eq!(
            no_founder_variation.validate(),
            Err(HeredityError::InvalidProfile)
        );

        let mut scripted_weight_ratio = profile();
        scripted_weight_ratio.maximum_action_weight = 65;
        assert_eq!(
            scripted_weight_ratio.validate(),
            Err(HeredityError::InvalidProfile)
        );

        let mut overwhelming_mutation = profile();
        overwhelming_mutation.mutation_probability_millionths = 250_001;
        assert_eq!(
            overwhelming_mutation.validate(),
            Err(HeredityError::InvalidProfile)
        );
    }
}
