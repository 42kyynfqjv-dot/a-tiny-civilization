//! Unit-declared packed scalar fields for full-Earth evidence layers.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, MAX_S2_LEVEL, S2CellId};

pub const PACKED_SCALAR_FIELD_TILE_SCHEMA_VERSION: u16 = 1;
pub const PACKED_SCALAR_FIELD_TILE_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.packed-scalar-field-tile+json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScalarFieldCell {
    pub s2_cell_id: S2CellId,
    pub support_samples: u64,
    pub minimum_value: i64,
    pub mean_value: i64,
    pub maximum_value: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackedScalarFieldTile {
    pub tile_schema_version: u16,
    pub layer_id: String,
    pub unit: String,
    pub decimal_places: u8,
    pub source_snapshot_digest: Digest,
    pub source_artifact_digest: Digest,
    pub quadrature_points_per_axis: u8,
    pub container_s2_cell_id: S2CellId,
    pub target_s2_level: u8,
    pub cells: Vec<ScalarFieldCell>,
}

impl PackedScalarFieldTile {
    pub fn validate(&self) -> Result<(), ScalarFieldTileError> {
        if self.tile_schema_version != PACKED_SCALAR_FIELD_TILE_SCHEMA_VERSION {
            return Err(ScalarFieldTileError::UnsupportedSchema(
                self.tile_schema_version,
            ));
        }
        if !slug(&self.layer_id) || !unit(&self.unit) {
            return Err(ScalarFieldTileError::InvalidIdentifier);
        }
        if self.source_snapshot_digest == Digest::ZERO
            || self.source_artifact_digest == Digest::ZERO
        {
            return Err(ScalarFieldTileError::ZeroDigest);
        }
        if self.quadrature_points_per_axis == 0
            || 60 % u16::from(self.quadrature_points_per_axis) != 0
        {
            return Err(ScalarFieldTileError::InvalidQuadrature);
        }
        if self.target_s2_level > MAX_S2_LEVEL
            || self.target_s2_level <= self.container_s2_cell_id.level()
        {
            return Err(ScalarFieldTileError::InvalidTargetLevel);
        }
        let expected = descendants(self.container_s2_cell_id, self.target_s2_level)?;
        if self.cells.len() != expected.len() {
            return Err(ScalarFieldTileError::WrongCellCount);
        }
        for (cell, expected_cell) in self.cells.iter().zip(expected) {
            if cell.s2_cell_id != expected_cell {
                return Err(ScalarFieldTileError::NonCanonicalCoverage);
            }
            if cell.support_samples == 0 {
                return Err(ScalarFieldTileError::ZeroSupport);
            }
            if cell.minimum_value > cell.mean_value || cell.mean_value > cell.maximum_value {
                return Err(ScalarFieldTileError::InvalidRange);
            }
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ScalarFieldTileError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| ScalarFieldTileError::Encoding(error.to_string()))
    }
    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, ScalarFieldTileError> {
        let tile: Self = serde_json::from_slice(bytes)
            .map_err(|error| ScalarFieldTileError::Decode(error.to_string()))?;
        tile.validate()?;
        if tile.canonical_bytes()? != bytes {
            return Err(ScalarFieldTileError::NonCanonicalEncoding);
        }
        Ok(tile)
    }
}

fn descendants(root: S2CellId, target: u8) -> Result<Vec<S2CellId>, ScalarFieldTileError> {
    let mut cells = vec![root];
    while cells.first().is_some_and(|cell| cell.level() < target) {
        let mut next = Vec::with_capacity(
            cells
                .len()
                .checked_mul(4)
                .ok_or(ScalarFieldTileError::CoverageOverflow)?,
        );
        for cell in cells {
            next.extend(
                cell.children()
                    .map_err(|error| ScalarFieldTileError::Spatial(error.to_string()))?,
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
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn unit(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'^' | b'-' | b'_' | b'.'))
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ScalarFieldTileError {
    #[error("unsupported scalar-field schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid scalar-field identifier or unit")]
    InvalidIdentifier,
    #[error("scalar-field digest must not be zero")]
    ZeroDigest,
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
    #[test]
    fn unit_declared_tile_round_trips_canonically() {
        let container: S2CellId = "1000010000000000".parse().unwrap();
        let tile = PackedScalarFieldTile {
            tile_schema_version: 1,
            layer_id: "january-air-temperature".to_owned(),
            unit: "degC".to_owned(),
            decimal_places: 3,
            source_snapshot_digest: Digest::sha256(b"s"),
            source_artifact_digest: Digest::sha256(b"a"),
            quadrature_points_per_axis: 4,
            container_s2_cell_id: container,
            target_s2_level: 11,
            cells: container
                .children()
                .unwrap()
                .into_iter()
                .map(|s2_cell_id| ScalarFieldCell {
                    s2_cell_id,
                    support_samples: 1,
                    minimum_value: -1,
                    mean_value: 0,
                    maximum_value: 1,
                })
                .collect(),
        };
        let bytes = tile.canonical_bytes().unwrap();
        assert_eq!(
            PackedScalarFieldTile::from_canonical_slice(&bytes),
            Ok(tile)
        );
    }
}
