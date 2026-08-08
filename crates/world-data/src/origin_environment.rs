//! Canonical evidence at a seed-derived provisional origin.
//!
//! This joins exact cells from pinned global releases without interpreting them as
//! habitat suitability, organism occurrence, or abundance.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, S2CellId};

use crate::{
    LandCoverEvidenceCell, SOILGRIDS_TOPSOIL_PROPERTIES, ScalarFieldCell, ScalarTerrainCell,
    SeasonalScalarFieldCell, SoilDepth, SoilGridsPropertySource, SoilGridsTopsoilCell,
    soilgrids_source_set_digest,
};

pub const LEGACY_PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION: u16 = 1;
pub const PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION: u16 = 2;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_surface: Option<ProvisionalOriginSurfaceEvidence>,
}

/// Exact, uninterpreted physical-source evidence joined at the selected L10 cell.
///
/// These values do not assert habitat suitability or expose an agent-facing label.
/// In particular, the JRC value remains an upstream source code and SoilGrids values
/// remain in their documented source integer domains until a later admitted mapping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProvisionalOriginSurfaceEvidence {
    pub terrain_root_digest: Digest,
    pub terrain_tile_digest: Digest,
    pub terrain: ScalarTerrainCell,
    pub surface_water_root_digest: Digest,
    pub surface_water_tile_digest: Digest,
    pub surface_water_unit: String,
    pub surface_water_decimal_places: u8,
    pub surface_water: ScalarFieldCell,
    pub topsoil_root_digest: Digest,
    pub topsoil_tile_digest: Digest,
    pub topsoil_depth: SoilDepth,
    pub topsoil_source_set_digest: Digest,
    pub topsoil_property_sources: Vec<SoilGridsPropertySource>,
    pub topsoil_sampling_reprojection_method: String,
    pub topsoil: SoilGridsTopsoilCell,
}

impl ProvisionalOriginEnvironment {
    pub fn validate(&self) -> Result<(), ProvisionalOriginEnvironmentError> {
        if !matches!(
            self.environment_schema_version,
            LEGACY_PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION
                | PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION
        ) {
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
        match (self.environment_schema_version, self.local_surface.as_ref()) {
            (LEGACY_PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION, None) => {}
            (PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION, Some(surface)) => {
                surface.validate(self.selected_l10_patch)?;
            }
            _ => return Err(ProvisionalOriginEnvironmentError::InvalidSurfaceEvidence),
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

impl ProvisionalOriginSurfaceEvidence {
    fn validate(&self, selected_patch: S2CellId) -> Result<(), ProvisionalOriginEnvironmentError> {
        if [
            self.terrain_root_digest,
            self.terrain_tile_digest,
            self.surface_water_root_digest,
            self.surface_water_tile_digest,
            self.topsoil_root_digest,
            self.topsoil_tile_digest,
            self.topsoil_source_set_digest,
        ]
        .contains(&Digest::ZERO)
            || self.terrain.s2_cell_id != selected_patch
            || self.terrain.support_samples == 0
            || self.terrain.minimum_millimetres > self.terrain.mean_millimetres
            || self.terrain.mean_millimetres > self.terrain.maximum_millimetres
            || self.surface_water.s2_cell_id != selected_patch
            || self.surface_water.support_samples == 0
            || self.surface_water.minimum_value > self.surface_water.mean_value
            || self.surface_water.mean_value > self.surface_water.maximum_value
            || self.surface_water.minimum_value < 0
            || self.surface_water.maximum_value > 255
            || self.surface_water_unit != "source_code"
            || self.surface_water_decimal_places != 0
            || self.topsoil.s2_cell_id != selected_patch
            || self.topsoil.support_samples == 0
            || self.topsoil_property_sources.len() != SOILGRIDS_TOPSOIL_PROPERTIES.len()
            || self
                .topsoil_property_sources
                .iter()
                .zip(SOILGRIDS_TOPSOIL_PROPERTIES)
                .any(|(source, expected)| {
                    source.property != expected
                        || source.quantile_artifact_digests.contains(&Digest::ZERO)
                })
            || soilgrids_source_set_digest(&self.topsoil_property_sources)
                != self.topsoil_source_set_digest
            || !identifier(&self.topsoil_sampling_reprojection_method)
        {
            return Err(ProvisionalOriginEnvironmentError::InvalidSurfaceEvidence);
        }
        Ok(())
    }
}

fn unit(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'^' | b'-' | b'_' | b'.')
        })
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
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
    #[error("invalid or incomplete provisional origin surface evidence")]
    InvalidSurfaceEvidence,
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
    use crate::{
        LandCoverClassCount, LandCoverSignedValueCount, SOILGRIDS_TOPSOIL_PROPERTIES,
        SoilGridsQuantileValues,
    };

    fn surface(selected_patch: S2CellId) -> ProvisionalOriginSurfaceEvidence {
        let topsoil_property_sources = SOILGRIDS_TOPSOIL_PROPERTIES
            .iter()
            .copied()
            .map(|property| SoilGridsPropertySource {
                property,
                quantile_artifact_digests: [
                    Digest::sha256(format!("{property:?}-low").as_bytes()),
                    Digest::sha256(format!("{property:?}-middle").as_bytes()),
                    Digest::sha256(format!("{property:?}-high").as_bytes()),
                ],
            })
            .collect::<Vec<_>>();
        ProvisionalOriginSurfaceEvidence {
            terrain_root_digest: Digest::sha256(b"terrain root"),
            terrain_tile_digest: Digest::sha256(b"terrain tile"),
            terrain: ScalarTerrainCell {
                s2_cell_id: selected_patch,
                support_samples: 16,
                minimum_millimetres: 1_000,
                mean_millimetres: 2_000,
                maximum_millimetres: 3_000,
            },
            surface_water_root_digest: Digest::sha256(b"water root"),
            surface_water_tile_digest: Digest::sha256(b"water tile"),
            surface_water_unit: "source_code".to_owned(),
            surface_water_decimal_places: 0,
            surface_water: ScalarFieldCell {
                s2_cell_id: selected_patch,
                support_samples: 1,
                minimum_value: 0,
                mean_value: 0,
                maximum_value: 0,
            },
            topsoil_root_digest: Digest::sha256(b"soil root"),
            topsoil_tile_digest: Digest::sha256(b"soil tile"),
            topsoil_depth: SoilDepth::ZeroToFiveCentimeters,
            topsoil_source_set_digest: soilgrids_source_set_digest(&topsoil_property_sources),
            topsoil_property_sources,
            topsoil_sampling_reprojection_method: "nearest-source-overview-cell".to_owned(),
            topsoil: SoilGridsTopsoilCell {
                s2_cell_id: selected_patch,
                support_samples: 1,
                property_values: [SoilGridsQuantileValues {
                    q0_05: 1,
                    q0_5: 2,
                    q0_95: 3,
                }; 9],
            },
        }
    }

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
            local_surface: None,
        }
    }

    #[test]
    fn origin_environment_round_trips_and_rejects_changed_cell_binding() {
        let mut environment = environment();
        environment.environment_schema_version =
            LEGACY_PROVISIONAL_ORIGIN_ENVIRONMENT_SCHEMA_VERSION;
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

    #[test]
    fn current_origin_environment_requires_complete_source_bound_surface_evidence() {
        let mut environment = environment();
        environment.local_surface = Some(surface(environment.selected_l10_patch));
        let bytes = environment.canonical_bytes().expect("canonical bytes");
        assert_eq!(
            ProvisionalOriginEnvironment::from_canonical_slice(&bytes),
            Ok(environment.clone())
        );

        environment
            .local_surface
            .as_mut()
            .expect("surface")
            .surface_water
            .mean_value = 256;
        assert_eq!(
            environment.validate(),
            Err(ProvisionalOriginEnvironmentError::InvalidSurfaceEvidence)
        );
    }
}
