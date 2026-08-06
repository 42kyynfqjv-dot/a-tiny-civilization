//! Canonical packed seasonal scalar fields for source-backed climate evidence.
//!
//! A seasonal tile retains every declared phase rather than reducing a normal year to
//! an annual average. It is intentionally an evidence container, not a weather model.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, MAX_S2_LEVEL, S2CellId};

pub const MONTHS_PER_NORMAL_YEAR: usize = 12;
pub const PACKED_SEASONAL_FIELD_TILE_SCHEMA_VERSION: u16 = 2;
pub const PACKED_SEASONAL_FIELD_TILE_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.packed-seasonal-field-tile+json";
const MAX_DECIMAL_PLACES: u8 = 9;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SeasonalScalarFieldCell {
    pub s2_cell_id: S2CellId,
    /// Equal source support per seasonal phase. A later schema can represent unequal
    /// coverage explicitly; v1 refuses to hide it in an annual mean.
    pub support_samples_per_phase: u64,
    pub minimum_values: Vec<i64>,
    pub mean_values: Vec<i64>,
    pub maximum_values: Vec<i64>,
}

/// One retained raw artifact and the normal-year phases to which it contributes.
///
/// Bit zero represents January and bit eleven represents December. This lets an
/// annual archive truthfully support every phase, while a monthly normal can retain
/// one distinct artifact per phase. It deliberately records source support rather
/// than pretending a normal year has only twelve upstream files.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SeasonalSourceArtifact {
    pub digest: Digest,
    pub phase_mask: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackedSeasonalScalarFieldTile {
    pub tile_schema_version: u16,
    pub layer_id: String,
    pub unit: String,
    pub decimal_places: u8,
    pub phases_per_cycle: u8,
    pub source_snapshot_digest: Digest,
    /// Canonically digest-ordered raw artifacts and the phases each supports.
    pub source_artifacts: Vec<SeasonalSourceArtifact>,
    pub quadrature_points_per_axis: u8,
    pub container_s2_cell_id: S2CellId,
    pub target_s2_level: u8,
    pub cells: Vec<SeasonalScalarFieldCell>,
}

impl PackedSeasonalScalarFieldTile {
    pub fn validate(&self) -> Result<(), SeasonalFieldTileError> {
        if self.tile_schema_version != PACKED_SEASONAL_FIELD_TILE_SCHEMA_VERSION {
            return Err(SeasonalFieldTileError::UnsupportedSchema(
                self.tile_schema_version,
            ));
        }
        if !slug(&self.layer_id) || !unit(&self.unit) {
            return Err(SeasonalFieldTileError::InvalidIdentifier);
        }
        if self.decimal_places > MAX_DECIMAL_PLACES {
            return Err(SeasonalFieldTileError::InvalidDecimalPlaces(
                self.decimal_places,
            ));
        }
        if usize::from(self.phases_per_cycle) != MONTHS_PER_NORMAL_YEAR {
            return Err(SeasonalFieldTileError::InvalidCycle);
        }
        if self.source_snapshot_digest == Digest::ZERO || self.source_artifacts.is_empty() {
            return Err(SeasonalFieldTileError::ZeroDigest);
        }
        let mut previous = None;
        let mut phase_coverage = 0_u16;
        for artifact in &self.source_artifacts {
            if artifact.digest == Digest::ZERO {
                return Err(SeasonalFieldTileError::ZeroDigest);
            }
            if artifact.phase_mask == 0
                || artifact.phase_mask & !normal_year_phase_mask() != 0
            {
                return Err(SeasonalFieldTileError::InvalidSourcePhaseCoverage);
            }
            if previous.is_some_and(|digest| digest >= artifact.digest) {
                return Err(SeasonalFieldTileError::NonCanonicalSourceArtifacts);
            }
            previous = Some(artifact.digest);
            phase_coverage |= artifact.phase_mask;
        }
        if phase_coverage != normal_year_phase_mask() {
            return Err(SeasonalFieldTileError::IncompleteSourcePhaseCoverage);
        }
        if self.quadrature_points_per_axis == 0
            || 60 % u16::from(self.quadrature_points_per_axis) != 0
        {
            return Err(SeasonalFieldTileError::InvalidQuadrature);
        }
        if self.target_s2_level > MAX_S2_LEVEL
            || self.target_s2_level <= self.container_s2_cell_id.level()
        {
            return Err(SeasonalFieldTileError::InvalidTargetLevel);
        }
        let expected = descendants(self.container_s2_cell_id, self.target_s2_level)?;
        if self.cells.len() != expected.len() {
            return Err(SeasonalFieldTileError::WrongCellCount);
        }
        for (cell, expected_cell) in self.cells.iter().zip(expected) {
            if cell.s2_cell_id != expected_cell {
                return Err(SeasonalFieldTileError::NonCanonicalCoverage);
            }
            if cell.support_samples_per_phase == 0 {
                return Err(SeasonalFieldTileError::ZeroSupport);
            }
            if cell.minimum_values.len() != MONTHS_PER_NORMAL_YEAR
                || cell.mean_values.len() != MONTHS_PER_NORMAL_YEAR
                || cell.maximum_values.len() != MONTHS_PER_NORMAL_YEAR
            {
                return Err(SeasonalFieldTileError::InvalidPhaseValues);
            }
            if cell
                .minimum_values
                .iter()
                .zip(&cell.mean_values)
                .zip(&cell.maximum_values)
                .any(|((minimum, mean), maximum)| minimum > mean || mean > maximum)
            {
                return Err(SeasonalFieldTileError::InvalidRange);
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SeasonalFieldTileError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| SeasonalFieldTileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, SeasonalFieldTileError> {
        let tile: Self = serde_json::from_slice(bytes)
            .map_err(|error| SeasonalFieldTileError::Decode(error.to_string()))?;
        tile.validate()?;
        if tile.canonical_bytes()? != bytes {
            return Err(SeasonalFieldTileError::NonCanonicalEncoding);
        }
        Ok(tile)
    }
}

const fn normal_year_phase_mask() -> u16 {
    (1_u16 << MONTHS_PER_NORMAL_YEAR) - 1
}

fn descendants(root: S2CellId, target: u8) -> Result<Vec<S2CellId>, SeasonalFieldTileError> {
    let mut cells = vec![root];
    while cells.first().is_some_and(|cell| cell.level() < target) {
        let mut next = Vec::with_capacity(
            cells
                .len()
                .checked_mul(4)
                .ok_or(SeasonalFieldTileError::CoverageOverflow)?,
        );
        for cell in cells {
            next.extend(
                cell.children()
                    .map_err(|error| SeasonalFieldTileError::Spatial(error.to_string()))?,
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

fn unit(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'^' | b'-' | b'_' | b'.')
        })
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SeasonalFieldTileError {
    #[error("unsupported seasonal-field schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid seasonal-field identifier or unit")]
    InvalidIdentifier,
    #[error("seasonal-field decimal places {0} exceeds the supported maximum")]
    InvalidDecimalPlaces(u8),
    #[error("a normal-year seasonal tile requires exactly twelve phases")]
    InvalidCycle,
    #[error("seasonal-field digest must not be zero")]
    ZeroDigest,
    #[error("a source artifact has an invalid normal-year phase mask")]
    InvalidSourcePhaseCoverage,
    #[error("source artifacts must be strictly ordered by digest")]
    NonCanonicalSourceArtifacts,
    #[error("source artifacts do not cover every normal-year phase")]
    IncompleteSourcePhaseCoverage,
    #[error("invalid quadrature")]
    InvalidQuadrature,
    #[error("invalid target level")]
    InvalidTargetLevel,
    #[error("wrong canonical cell count")]
    WrongCellCount,
    #[error("noncanonical coverage")]
    NonCanonicalCoverage,
    #[error("zero support")]
    ZeroSupport,
    #[error("seasonal phase values must have exactly twelve entries")]
    InvalidPhaseValues,
    #[error("invalid value range")]
    InvalidRange,
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

    fn tile() -> PackedSeasonalScalarFieldTile {
        let container: S2CellId = "1000010000000000".parse().expect("valid S2 cell");
        let mut source_artifacts: Vec<_> = (0..12)
            .map(|month| SeasonalSourceArtifact {
                digest: Digest::sha256(&[month]),
                phase_mask: 1 << month,
            })
            .collect();
        source_artifacts.sort_by_key(|artifact| artifact.digest);
        PackedSeasonalScalarFieldTile {
            tile_schema_version: PACKED_SEASONAL_FIELD_TILE_SCHEMA_VERSION,
            layer_id: "air-temperature-normal".to_owned(),
            unit: "degC".to_owned(),
            decimal_places: 3,
            phases_per_cycle: 12,
            source_snapshot_digest: Digest::sha256(b"annual-source"),
            source_artifacts,
            quadrature_points_per_axis: 4,
            container_s2_cell_id: container,
            target_s2_level: 11,
            cells: container
                .children()
                .expect("children")
                .into_iter()
                .map(|s2_cell_id| SeasonalScalarFieldCell {
                    s2_cell_id,
                    support_samples_per_phase: 1,
                    minimum_values: vec![-1; 12],
                    mean_values: vec![0; 12],
                    maximum_values: vec![1; 12],
                })
                .collect(),
        }
    }

    #[test]
    fn normal_year_tile_round_trips_canonically() {
        let tile = tile();
        let bytes = tile.canonical_bytes().expect("canonical tile");
        assert_eq!(
            PackedSeasonalScalarFieldTile::from_canonical_slice(&bytes),
            Ok(tile)
        );
    }

    #[test]
    fn rejects_flattened_or_partial_climate_cycles() {
        let mut partial = tile();
        partial.source_artifacts.pop();
        assert_eq!(
            partial.validate(),
            Err(SeasonalFieldTileError::IncompleteSourcePhaseCoverage)
        );
        let mut flattened = tile();
        flattened.cells[0].mean_values.truncate(1);
        assert_eq!(
            flattened.validate(),
            Err(SeasonalFieldTileError::InvalidPhaseValues)
        );
        let mut unsorted_sources = tile();
        unsorted_sources.source_artifacts.swap(0, 1);
        assert_eq!(
            unsorted_sources.validate(),
            Err(SeasonalFieldTileError::NonCanonicalSourceArtifacts)
        );
    }

    #[test]
    fn annual_sources_can_support_all_normal_months() {
        let mut annual = tile();
        annual.source_artifacts = (0_u8..30)
            .map(|year| SeasonalSourceArtifact {
                digest: Digest::sha256(&year.to_be_bytes()),
                phase_mask: normal_year_phase_mask(),
            })
            .collect();
        annual
            .source_artifacts
            .sort_by_key(|artifact| artifact.digest);
        assert!(annual.validate().is_ok());
    }
}
