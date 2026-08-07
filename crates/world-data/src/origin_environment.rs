//! Canonical evidence at a seed-derived provisional origin.
//!
//! This joins exact cells from pinned global releases without interpreting them as
//! habitat suitability, organism occurrence, or abundance.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, S2CellId};

use crate::{LandCoverEvidenceCell, SeasonalScalarFieldCell};

pub const PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION: u16 = 1;
pub const PROVISIONAL_ORIGIN_ENVIRONMENT_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.provisional-origin-environment+json";
const STATUS: &str = "evidence-only-not-habitat-suitability-or-population";
const NORMAL_YEAR_PHASES: usize = 12;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProvisionalOriginEnvironment {
    pub environment_schema_version: u16,
    pub status: String,
    pub origin_selection_digest: Digest,
    pub composition_digest: Digest,
    pub selected_l10_patch: S2CellId,
    pub selected_embodied_patch: S2CellId,
    pub observed_land_cover_root_digest: Digest,
    pub observed_land_cover_tile_digest: Digest,
    pub observed_land_cover: LandCoverEvidenceCell,
    pub air_temperature_normal_root_digest: Digest,
    pub air_temperature_normal_tile_digest: Digest,
    pub air_temperature_normal_unit: String,
    pub air_temperature_normal_decimal_places: u8,
    pub air_temperature_normal: SeasonalScalarFieldCell,
}

impl ProvisionalOriginEnvironment {
    pub fn validate(&self) -> Result<(), ProvisionalOriginEnvironmentError> {
        if self.environment_schema_version != PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION {
            return Err(ProvisionalOriginEnvironmentError::UnsupportedSchema(
                self.environment_schema_version,
            ));
        }
        if self.status != STATUS
            || [
                self.origin_selection_digest,
                self.composition_digest,
                self.observed_land_cover_root_digest,
                self.observed_land_cover_tile_digest,
                self.air_temperature_normal_root_digest,
                self.air_temperature_normal_tile_digest,
            ]
            .contains(&Digest::ZERO)
            || self.selected_l10_patch.level() != 10
            || self.selected_embodied_patch.level() < self.selected_l10_patch.level()
            || self
                .selected_embodied_patch
                .ancestor(self.selected_l10_patch.level())
                .map_err(|_| ProvisionalOriginEnvironmentError::InvalidSpatialBinding)?
                != self.selected_l10_patch
        {
            return Err(ProvisionalOriginEnvironmentError::InvalidIdentity);
        }
        if self.observed_land_cover.s2_cell_id != self.selected_l10_patch
            || self.observed_land_cover.support_samples == 0
            || self.observed_land_cover.class_counts.is_empty()
            || self
                .observed_land_cover
                .class_counts
                .windows(2)
                .any(|pair| pair[0].class_value >= pair[1].class_value || pair[0].samples == 0)
            || self
                .observed_land_cover
                .class_counts
                .last()
                .is_some_and(|entry| entry.samples == 0)
            || self
                .observed_land_cover
                .class_counts
                .iter()
                .try_fold(0_u64, |total, entry| total.checked_add(entry.samples))
                != Some(self.observed_land_cover.support_samples)
        {
            return Err(ProvisionalOriginEnvironmentError::InvalidLandCoverEvidence);
        }
        let climate = &self.air_temperature_normal;
        if climate.s2_cell_id != self.selected_l10_patch
            || climate.support_samples_per_phase == 0
            || climate.minimum_values.len() != NORMAL_YEAR_PHASES
            || climate.mean_values.len() != NORMAL_YEAR_PHASES
            || climate.maximum_values.len() != NORMAL_YEAR_PHASES
            || climate
                .minimum_values
                .iter()
                .zip(&climate.mean_values)
                .zip(&climate.maximum_values)
                .any(|((minimum, mean), maximum)| minimum > mean || mean > maximum)
            || !unit(&self.air_temperature_normal_unit)
            || self.air_temperature_normal_decimal_places > 9
        {
            return Err(ProvisionalOriginEnvironmentError::InvalidClimateEvidence);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProvisionalOriginEnvironmentError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| ProvisionalOriginEnvironmentError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, ProvisionalOriginEnvironmentError> {
        let environment: Self = serde_json::from_slice(bytes)
            .map_err(|error| ProvisionalOriginEnvironmentError::Decode(error.to_string()))?;
        if environment.canonical_bytes()? != bytes {
            return Err(ProvisionalOriginEnvironmentError::NonCanonicalEncoding);
        }
        Ok(environment)
    }
}

fn unit(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'^' | b'-' | b'_' | b'.')
        })
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProvisionalOriginEnvironmentError {
    #[error("unsupported provisional origin-environment schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid provisional origin-environment identity or provenance")]
    InvalidIdentity,
    #[error("invalid provisional origin-environment spatial binding")]
    InvalidSpatialBinding,
    #[error("invalid provisional origin land-cover evidence")]
    InvalidLandCoverEvidence,
    #[error("invalid provisional origin climate evidence")]
    InvalidClimateEvidence,
    #[error("decode error: {0}")]
    Decode(String),
    #[error("encoding error: {0}")]
    Encoding(String),
    #[error("noncanonical encoding")]
    NonCanonicalEncoding,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LandCoverClassCount, LandCoverSignedValueCount};

    fn environment() -> ProvisionalOriginEnvironment {
        let selected_l10_patch: S2CellId = "1000010000000000".parse().expect("L10 patch");
        ProvisionalOriginEnvironment {
            environment_schema_version: PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION,
            status: STATUS.to_owned(),
            origin_selection_digest: Digest::sha256(b"origin"),
            composition_digest: Digest::sha256(b"composition"),
            selected_l10_patch,
            selected_embodied_patch: selected_l10_patch.descendants_at(11).expect("children")[0],
            observed_land_cover_root_digest: Digest::sha256(b"land root"),
            observed_land_cover_tile_digest: Digest::sha256(b"land tile"),
            observed_land_cover: LandCoverEvidenceCell {
                s2_cell_id: selected_l10_patch,
                support_samples: 4,
                class_counts: vec![LandCoverClassCount {
                    class_value: 130,
                    samples: 4,
                }],
                processed_flag_counts: vec![LandCoverSignedValueCount {
                    value: 1,
                    samples: 4,
                }],
                current_pixel_state_counts: vec![LandCoverSignedValueCount {
                    value: 1,
                    samples: 4,
                }],
                observation_count_minimum: 1,
                observation_count_sum: 4,
                observation_count_maximum: 1,
                change_count_minimum: 0,
                change_count_sum: 0,
                change_count_maximum: 0,
            },
            air_temperature_normal_root_digest: Digest::sha256(b"climate root"),
            air_temperature_normal_tile_digest: Digest::sha256(b"climate tile"),
            air_temperature_normal_unit: "degC".to_owned(),
            air_temperature_normal_decimal_places: 3,
            air_temperature_normal: SeasonalScalarFieldCell {
                s2_cell_id: selected_l10_patch,
                support_samples_per_phase: 1,
                minimum_values: vec![1; NORMAL_YEAR_PHASES],
                mean_values: vec![2; NORMAL_YEAR_PHASES],
                maximum_values: vec![3; NORMAL_YEAR_PHASES],
            },
        }
    }

    #[test]
    fn origin_environment_round_trips_and_rejects_changed_cell_binding() {
        let environment = environment();
        let bytes = environment.canonical_bytes().expect("canonical bytes");
        assert_eq!(
            ProvisionalOriginEnvironment::from_canonical_slice(&bytes),
            Ok(environment.clone())
        );
        let mut invalid = environment;
        invalid.air_temperature_normal.s2_cell_id = "1000030000000000".parse().expect("L10");
        assert_eq!(
            invalid.validate(),
            Err(ProvisionalOriginEnvironmentError::InvalidClimateEvidence)
        );
    }
}
