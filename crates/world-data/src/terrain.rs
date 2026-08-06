//! Canonical packed scalar terrain tiles for full-Earth layers.
//!
//! One filesystem artifact covers a coarse S2 container and carries every value at
//! the declared target level. This avoids millions of one-cell files while keeping
//! every value, its support count, and its source spread auditable.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, MAX_S2_LEVEL, S2CellId};

pub const PACKED_SCALAR_TERRAIN_TILE_SCHEMA_VERSION: u16 = 1;
pub const PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.packed-scalar-terrain-tile+json";

/// A source-supported scalar summary for one target S2 cell, measured in millimetres.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScalarTerrainCell {
    pub s2_cell_id: S2CellId,
    pub support_samples: u64,
    pub minimum_millimetres: i64,
    pub mean_millimetres: i64,
    pub maximum_millimetres: i64,
}

/// All same-level target values contained by one coarser S2 container.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackedScalarTerrainTile {
    pub tile_schema_version: u16,
    pub layer_id: String,
    pub source_snapshot_digest: Digest,
    pub source_artifact_digest: Digest,
    pub quadrature_points_per_axis: u8,
    pub container_s2_cell_id: S2CellId,
    pub target_s2_level: u8,
    pub cells: Vec<ScalarTerrainCell>,
}

impl PackedScalarTerrainTile {
    pub fn validate(&self) -> Result<(), TerrainTileError> {
        if self.tile_schema_version != PACKED_SCALAR_TERRAIN_TILE_SCHEMA_VERSION {
            return Err(TerrainTileError::UnsupportedSchema(
                self.tile_schema_version,
            ));
        }
        if !is_slug(&self.layer_id) {
            return Err(TerrainTileError::InvalidLayerId(self.layer_id.clone()));
        }
        if self.source_snapshot_digest == Digest::ZERO {
            return Err(TerrainTileError::ZeroDigest("source_snapshot_digest"));
        }
        if self.source_artifact_digest == Digest::ZERO {
            return Err(TerrainTileError::ZeroDigest("source_artifact_digest"));
        }
        if self.quadrature_points_per_axis == 0
            || 60 % u16::from(self.quadrature_points_per_axis) != 0
        {
            return Err(TerrainTileError::InvalidQuadrature(
                self.quadrature_points_per_axis,
            ));
        }
        if self.target_s2_level > MAX_S2_LEVEL
            || self.target_s2_level <= self.container_s2_cell_id.level()
        {
            return Err(TerrainTileError::InvalidTargetLevel {
                container: self.container_s2_cell_id.level(),
                target: self.target_s2_level,
            });
        }

        let expected = expected_descendants(self.container_s2_cell_id, self.target_s2_level)?;
        if self.cells.len() != expected.len() {
            return Err(TerrainTileError::WrongCellCount {
                expected: expected.len(),
                actual: self.cells.len(),
            });
        }
        for (index, (cell, expected_cell)) in self.cells.iter().zip(expected).enumerate() {
            if cell.s2_cell_id != expected_cell {
                return Err(TerrainTileError::NonCanonicalCoverage { index });
            }
            if cell.support_samples == 0 {
                return Err(TerrainTileError::ZeroSupportSamples {
                    cell: cell.s2_cell_id,
                });
            }
            if cell.minimum_millimetres > cell.mean_millimetres
                || cell.mean_millimetres > cell.maximum_millimetres
            {
                return Err(TerrainTileError::InvalidRange {
                    cell: cell.s2_cell_id,
                });
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, TerrainTileError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| TerrainTileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, TerrainTileError> {
        let tile: Self = serde_json::from_slice(bytes)
            .map_err(|error| TerrainTileError::Decode(error.to_string()))?;
        tile.validate()?;
        if tile.canonical_bytes()? != bytes {
            return Err(TerrainTileError::NonCanonicalEncoding);
        }
        Ok(tile)
    }
}

fn expected_descendants(
    root: S2CellId,
    target_level: u8,
) -> Result<Vec<S2CellId>, TerrainTileError> {
    let mut current = vec![root];
    while current
        .first()
        .is_some_and(|cell| cell.level() < target_level)
    {
        let mut next = Vec::with_capacity(
            current
                .len()
                .checked_mul(4)
                .ok_or(TerrainTileError::CoverageOverflow)?,
        );
        for cell in current {
            next.extend(
                cell.children()
                    .map_err(|error| TerrainTileError::Spatial(error.to_string()))?,
            );
        }
        current = next;
    }
    Ok(current)
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TerrainTileError {
    #[error("packed scalar terrain tile schema version {0} is unsupported")]
    UnsupportedSchema(u16),
    #[error("packed scalar terrain tile layer id {0:?} is invalid")]
    InvalidLayerId(String),
    #[error("packed scalar terrain tile {0} must not be zero")]
    ZeroDigest(&'static str),
    #[error("quadrature points per axis {0} must be a nonzero divisor of 60")]
    InvalidQuadrature(u8),
    #[error(
        "target S2 level {target} must be finer than container level {container} and at most 30"
    )]
    InvalidTargetLevel { container: u8, target: u8 },
    #[error("packed scalar terrain tile needs {expected} cells but has {actual}")]
    WrongCellCount { expected: usize, actual: usize },
    #[error("packed scalar terrain tile coverage is not canonical at cell index {index}")]
    NonCanonicalCoverage { index: usize },
    #[error("packed scalar terrain tile cell {cell} has zero support samples")]
    ZeroSupportSamples { cell: S2CellId },
    #[error("packed scalar terrain tile cell {cell} has an invalid min/mean/max range")]
    InvalidRange { cell: S2CellId },
    #[error("packed scalar terrain tile coverage would overflow memory")]
    CoverageOverflow,
    #[error("packed scalar terrain tile spatial failure: {0}")]
    Spatial(String),
    #[error("packed scalar terrain tile JSON could not be decoded: {0}")]
    Decode(String),
    #[error("packed scalar terrain tile JSON could not be encoded: {0}")]
    Encoding(String),
    #[error("packed scalar terrain tile JSON is valid but not canonical")]
    NonCanonicalEncoding,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile() -> PackedScalarTerrainTile {
        let container: S2CellId = "1000010000000000".parse().expect("valid level-10 cell");
        let cells = container
            .children()
            .expect("children")
            .into_iter()
            .map(|s2_cell_id| ScalarTerrainCell {
                s2_cell_id,
                support_samples: 16,
                minimum_millimetres: -2_000,
                mean_millimetres: 500,
                maximum_millimetres: 3_000,
            })
            .collect();
        PackedScalarTerrainTile {
            tile_schema_version: PACKED_SCALAR_TERRAIN_TILE_SCHEMA_VERSION,
            layer_id: "bedrock-relief".to_owned(),
            source_snapshot_digest: Digest::sha256(b"snapshot"),
            source_artifact_digest: Digest::sha256(b"artifact"),
            quadrature_points_per_axis: 4,
            container_s2_cell_id: container,
            target_s2_level: 11,
            cells,
        }
    }

    #[test]
    fn canonical_tile_covers_every_descendant_and_round_trips() {
        let tile = tile();
        let bytes = tile.canonical_bytes().expect("canonical bytes");
        assert_eq!(
            PackedScalarTerrainTile::from_canonical_slice(&bytes),
            Ok(tile)
        );
    }

    #[test]
    fn missing_reordered_or_unsourced_cells_fail_closed() {
        let mut missing = tile();
        missing.cells.pop();
        assert!(missing.validate().is_err());

        let mut reordered = tile();
        reordered.cells.swap(0, 1);
        assert!(reordered.validate().is_err());

        let mut unsourced = tile();
        unsourced.cells[0].support_samples = 0;
        assert!(unsourced.validate().is_err());
    }
}
