//! Packed Copernicus land-cover evidence with explicit classification quality.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, MAX_S2_LEVEL, S2CellId};

pub const PACKED_LAND_COVER_EVIDENCE_TILE_SCHEMA_VERSION: u16 = 1;
pub const PACKED_LAND_COVER_EVIDENCE_TILE_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.packed-land-cover-evidence-tile+json";

pub const COPERNICUS_LCCS_CLASSES: [(u8, &str); 38] = [
    (0, "no_data"),
    (10, "cropland_rainfed"),
    (11, "cropland_rainfed_herbaceous_cover"),
    (12, "cropland_rainfed_tree_or_shrub_cover"),
    (20, "cropland_irrigated"),
    (30, "mosaic_cropland"),
    (40, "mosaic_natural_vegetation"),
    (50, "tree_broadleaved_evergreen_closed_to_open"),
    (60, "tree_broadleaved_deciduous_closed_to_open"),
    (61, "tree_broadleaved_deciduous_closed"),
    (62, "tree_broadleaved_deciduous_open"),
    (70, "tree_needleleaved_evergreen_closed_to_open"),
    (71, "tree_needleleaved_evergreen_closed"),
    (72, "tree_needleleaved_evergreen_open"),
    (80, "tree_needleleaved_deciduous_closed_to_open"),
    (81, "tree_needleleaved_deciduous_closed"),
    (82, "tree_needleleaved_deciduous_open"),
    (90, "tree_mixed"),
    (100, "mosaic_tree_and_shrub"),
    (110, "mosaic_herbaceous"),
    (120, "shrubland"),
    (121, "shrubland_evergreen"),
    (122, "shrubland_deciduous"),
    (130, "grassland"),
    (140, "lichens_and_mosses"),
    (150, "sparse_vegetation"),
    (151, "sparse_tree"),
    (152, "sparse_shrub"),
    (153, "sparse_herbaceous"),
    (160, "tree_cover_flooded_fresh_or_brakish_water"),
    (170, "tree_cover_flooded_saline_water"),
    (180, "shrub_or_herbaceous_cover_flooded"),
    (190, "urban"),
    (200, "bare_areas"),
    (201, "bare_areas_consolidated"),
    (202, "bare_areas_unconsolidated"),
    (210, "water"),
    (220, "snow_and_ice"),
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LandCoverClassCount {
    pub class_value: u8,
    pub samples: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LandCoverSignedValueCount {
    pub value: i8,
    pub samples: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LandCoverEvidenceCell {
    pub s2_cell_id: S2CellId,
    pub support_samples: u64,
    pub class_counts: Vec<LandCoverClassCount>,
    pub processed_flag_counts: Vec<LandCoverSignedValueCount>,
    pub current_pixel_state_counts: Vec<LandCoverSignedValueCount>,
    pub observation_count_minimum: u16,
    pub observation_count_sum: u64,
    pub observation_count_maximum: u16,
    pub change_count_minimum: u8,
    pub change_count_sum: u64,
    pub change_count_maximum: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackedLandCoverEvidenceTile {
    pub tile_schema_version: u16,
    pub layer_id: String,
    pub source_snapshot_digest: Digest,
    pub source_artifact_digest: Digest,
    pub sample_policy: String,
    pub quadrature_points_per_axis: u8,
    pub container_s2_cell_id: S2CellId,
    pub target_s2_level: u8,
    pub cells: Vec<LandCoverEvidenceCell>,
}

impl PackedLandCoverEvidenceTile {
    pub fn validate(&self) -> Result<(), LandCoverEvidenceTileError> {
        if self.tile_schema_version != PACKED_LAND_COVER_EVIDENCE_TILE_SCHEMA_VERSION {
            return Err(LandCoverEvidenceTileError::UnsupportedSchema(
                self.tile_schema_version,
            ));
        }
        if !slug(&self.layer_id) || !slug(&self.sample_policy) {
            return Err(LandCoverEvidenceTileError::InvalidIdentifier);
        }
        if self.source_snapshot_digest == Digest::ZERO
            || self.source_artifact_digest == Digest::ZERO
        {
            return Err(LandCoverEvidenceTileError::ZeroDigest);
        }
        if self.quadrature_points_per_axis == 0 || self.quadrature_points_per_axis > 64 {
            return Err(LandCoverEvidenceTileError::InvalidQuadrature);
        }
        if self.target_s2_level > MAX_S2_LEVEL
            || self.target_s2_level <= self.container_s2_cell_id.level()
        {
            return Err(LandCoverEvidenceTileError::InvalidTargetLevel);
        }
        let expected = descendants(self.container_s2_cell_id, self.target_s2_level)?;
        if self.cells.len() != expected.len() {
            return Err(LandCoverEvidenceTileError::WrongCellCount);
        }
        let expected_support = u64::from(self.quadrature_points_per_axis)
            .checked_mul(u64::from(self.quadrature_points_per_axis))
            .ok_or(LandCoverEvidenceTileError::CoverageOverflow)?;
        for (cell, expected_cell) in self.cells.iter().zip(expected) {
            if cell.s2_cell_id != expected_cell {
                return Err(LandCoverEvidenceTileError::NonCanonicalCoverage);
            }
            if cell.support_samples != expected_support {
                return Err(LandCoverEvidenceTileError::InvalidSupport);
            }
            validate_class_counts(&cell.class_counts, expected_support)?;
            validate_signed_counts(&cell.processed_flag_counts, expected_support, -1, 1)?;
            validate_signed_counts(&cell.current_pixel_state_counts, expected_support, -1, 5)?;
            validate_summary(
                u64::from(cell.observation_count_minimum),
                cell.observation_count_sum,
                u64::from(cell.observation_count_maximum),
                expected_support,
                32_767,
            )?;
            validate_summary(
                u64::from(cell.change_count_minimum),
                cell.change_count_sum,
                u64::from(cell.change_count_maximum),
                expected_support,
                100,
            )?;
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, LandCoverEvidenceTileError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| LandCoverEvidenceTileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, LandCoverEvidenceTileError> {
        let tile: Self = serde_json::from_slice(bytes)
            .map_err(|error| LandCoverEvidenceTileError::Decode(error.to_string()))?;
        tile.validate()?;
        if tile.canonical_bytes()? != bytes {
            return Err(LandCoverEvidenceTileError::NonCanonicalEncoding);
        }
        Ok(tile)
    }
}

fn validate_class_counts(
    counts: &[LandCoverClassCount],
    expected_support: u64,
) -> Result<(), LandCoverEvidenceTileError> {
    let mut prior = None;
    let mut total = 0_u64;
    for count in counts {
        if count.samples == 0
            || prior.is_some_and(|value| value >= count.class_value)
            || !COPERNICUS_LCCS_CLASSES
                .iter()
                .any(|(value, _)| *value == count.class_value)
        {
            return Err(LandCoverEvidenceTileError::InvalidClassCounts);
        }
        prior = Some(count.class_value);
        total = total
            .checked_add(count.samples)
            .ok_or(LandCoverEvidenceTileError::CoverageOverflow)?;
    }
    if total != expected_support {
        return Err(LandCoverEvidenceTileError::InvalidClassCounts);
    }
    Ok(())
}

fn validate_signed_counts(
    counts: &[LandCoverSignedValueCount],
    expected_support: u64,
    minimum: i8,
    maximum: i8,
) -> Result<(), LandCoverEvidenceTileError> {
    let mut prior = None;
    let mut total = 0_u64;
    for count in counts {
        if count.samples == 0
            || count.value < minimum
            || count.value > maximum
            || prior.is_some_and(|value| value >= count.value)
        {
            return Err(LandCoverEvidenceTileError::InvalidQualityCounts);
        }
        prior = Some(count.value);
        total = total
            .checked_add(count.samples)
            .ok_or(LandCoverEvidenceTileError::CoverageOverflow)?;
    }
    if total != expected_support {
        return Err(LandCoverEvidenceTileError::InvalidQualityCounts);
    }
    Ok(())
}

fn validate_summary(
    minimum: u64,
    sum: u64,
    maximum: u64,
    support: u64,
    domain_maximum: u64,
) -> Result<(), LandCoverEvidenceTileError> {
    if minimum > maximum || maximum > domain_maximum {
        return Err(LandCoverEvidenceTileError::InvalidQualitySummary);
    }
    let minimum_sum = minimum
        .checked_mul(support)
        .ok_or(LandCoverEvidenceTileError::CoverageOverflow)?;
    let maximum_sum = maximum
        .checked_mul(support)
        .ok_or(LandCoverEvidenceTileError::CoverageOverflow)?;
    if sum < minimum_sum || sum > maximum_sum {
        return Err(LandCoverEvidenceTileError::InvalidQualitySummary);
    }
    Ok(())
}

fn descendants(root: S2CellId, target: u8) -> Result<Vec<S2CellId>, LandCoverEvidenceTileError> {
    let mut cells = vec![root];
    while cells.first().is_some_and(|cell| cell.level() < target) {
        let mut next = Vec::with_capacity(
            cells
                .len()
                .checked_mul(4)
                .ok_or(LandCoverEvidenceTileError::CoverageOverflow)?,
        );
        for cell in cells {
            next.extend(
                cell.children()
                    .map_err(|error| LandCoverEvidenceTileError::Spatial(error.to_string()))?,
            );
        }
        cells = next;
    }
    Ok(cells)
}

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LandCoverEvidenceTileError {
    #[error("unsupported land-cover evidence schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid land-cover evidence identifier")]
    InvalidIdentifier,
    #[error("land-cover evidence digest must not be zero")]
    ZeroDigest,
    #[error("invalid land-cover target quadrature")]
    InvalidQuadrature,
    #[error("invalid target level")]
    InvalidTargetLevel,
    #[error("wrong canonical cell count")]
    WrongCellCount,
    #[error("noncanonical coverage")]
    NonCanonicalCoverage,
    #[error("invalid target support")]
    InvalidSupport,
    #[error("invalid LCCS class counts")]
    InvalidClassCounts,
    #[error("invalid classification quality counts")]
    InvalidQualityCounts,
    #[error("invalid classification quality summary")]
    InvalidQualitySummary,
    #[error("coverage overflow")]
    CoverageOverflow,
    #[error("spatial error: {0}")]
    Spatial(String),
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

    fn tile() -> PackedLandCoverEvidenceTile {
        let container: S2CellId = "1000010000000000".parse().expect("valid cell");
        PackedLandCoverEvidenceTile {
            tile_schema_version: 1,
            layer_id: "observed-land-cover".to_owned(),
            source_snapshot_digest: Digest::sha256(b"snapshot"),
            source_artifact_digest: Digest::sha256(b"artifact"),
            sample_policy: "s2-face-uv-q2-e7-source-area-v1".to_owned(),
            quadrature_points_per_axis: 2,
            container_s2_cell_id: container,
            target_s2_level: 11,
            cells: container
                .children()
                .expect("children")
                .into_iter()
                .map(|s2_cell_id| LandCoverEvidenceCell {
                    s2_cell_id,
                    support_samples: 4,
                    class_counts: vec![
                        LandCoverClassCount {
                            class_value: 130,
                            samples: 3,
                        },
                        LandCoverClassCount {
                            class_value: 210,
                            samples: 1,
                        },
                    ],
                    processed_flag_counts: vec![LandCoverSignedValueCount {
                        value: 1,
                        samples: 4,
                    }],
                    current_pixel_state_counts: vec![
                        LandCoverSignedValueCount {
                            value: 1,
                            samples: 3,
                        },
                        LandCoverSignedValueCount {
                            value: 2,
                            samples: 1,
                        },
                    ],
                    observation_count_minimum: 10,
                    observation_count_sum: 50,
                    observation_count_maximum: 15,
                    change_count_minimum: 0,
                    change_count_sum: 1,
                    change_count_maximum: 1,
                })
                .collect(),
        }
    }

    #[test]
    fn packed_land_cover_evidence_round_trips_canonically() {
        let tile = tile();
        let bytes = tile.canonical_bytes().expect("canonical tile");
        assert_eq!(
            PackedLandCoverEvidenceTile::from_canonical_slice(&bytes),
            Ok(tile)
        );
    }

    #[test]
    fn land_cover_evidence_rejects_lost_or_privileged_class_support() {
        let mut invalid = tile();
        invalid.cells[0].class_counts[0].samples = 2;
        assert_eq!(
            invalid.validate(),
            Err(LandCoverEvidenceTileError::InvalidClassCounts)
        );
        let mut invalid = tile();
        invalid.cells[0].class_counts[0].class_value = 131;
        assert_eq!(
            invalid.validate(),
            Err(LandCoverEvidenceTileError::InvalidClassCounts)
        );
    }

    #[test]
    fn land_cover_evidence_rejects_invalid_quality_support() {
        let mut invalid = tile();
        invalid.cells[0].processed_flag_counts[0].value = 2;
        assert_eq!(
            invalid.validate(),
            Err(LandCoverEvidenceTileError::InvalidQualityCounts)
        );
        let mut invalid = tile();
        invalid.cells[0].observation_count_sum = 9;
        assert_eq!(
            invalid.validate(),
            Err(LandCoverEvidenceTileError::InvalidQualitySummary)
        );
    }
}
