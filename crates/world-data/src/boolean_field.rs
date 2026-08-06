//! Canonical packed Boolean fields for physical coverage evidence.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, MAX_S2_LEVEL, S2CellId};

pub const PACKED_BOOLEAN_FIELD_TILE_SCHEMA_VERSION: u16 = 1;
pub const PACKED_BOOLEAN_FIELD_TILE_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.packed-boolean-field-tile+json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BooleanFieldCell {
    pub s2_cell_id: S2CellId,
    pub support_samples: u64,
    pub true_samples: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackedBooleanFieldTile {
    pub tile_schema_version: u16,
    pub layer_id: String,
    pub source_snapshot_digest: Digest,
    pub source_artifact_digest: Digest,
    pub sample_policy: String,
    pub container_s2_cell_id: S2CellId,
    pub target_s2_level: u8,
    pub cells: Vec<BooleanFieldCell>,
}

impl PackedBooleanFieldTile {
    pub fn validate(&self) -> Result<(), BooleanFieldTileError> {
        if self.tile_schema_version != PACKED_BOOLEAN_FIELD_TILE_SCHEMA_VERSION {
            return Err(BooleanFieldTileError::UnsupportedSchema(
                self.tile_schema_version,
            ));
        }
        if !slug(&self.layer_id) || !slug(&self.sample_policy) {
            return Err(BooleanFieldTileError::InvalidIdentifier);
        }
        if self.source_snapshot_digest == Digest::ZERO
            || self.source_artifact_digest == Digest::ZERO
        {
            return Err(BooleanFieldTileError::ZeroDigest);
        }
        if self.target_s2_level > MAX_S2_LEVEL
            || self.target_s2_level <= self.container_s2_cell_id.level()
        {
            return Err(BooleanFieldTileError::InvalidTargetLevel);
        }
        let expected = descendants(self.container_s2_cell_id, self.target_s2_level)?;
        if self.cells.len() != expected.len() {
            return Err(BooleanFieldTileError::WrongCellCount);
        }
        for (cell, expected_cell) in self.cells.iter().zip(expected) {
            if cell.s2_cell_id != expected_cell {
                return Err(BooleanFieldTileError::NonCanonicalCoverage);
            }
            if cell.support_samples == 0 || cell.true_samples > cell.support_samples {
                return Err(BooleanFieldTileError::InvalidSupport);
            }
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, BooleanFieldTileError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|e| BooleanFieldTileError::Encoding(e.to_string()))
    }
    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, BooleanFieldTileError> {
        let tile: Self = serde_json::from_slice(bytes)
            .map_err(|e| BooleanFieldTileError::Decode(e.to_string()))?;
        tile.validate()?;
        if tile.canonical_bytes()? != bytes {
            return Err(BooleanFieldTileError::NonCanonicalEncoding);
        }
        Ok(tile)
    }
}

fn descendants(root: S2CellId, target: u8) -> Result<Vec<S2CellId>, BooleanFieldTileError> {
    let mut cells = vec![root];
    while cells.first().is_some_and(|cell| cell.level() < target) {
        let mut next = Vec::with_capacity(
            cells
                .len()
                .checked_mul(4)
                .ok_or(BooleanFieldTileError::CoverageOverflow)?,
        );
        for cell in cells {
            next.extend(
                cell.children()
                    .map_err(|e| BooleanFieldTileError::Spatial(e.to_string()))?,
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

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BooleanFieldTileError {
    #[error("unsupported Boolean-field schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid Boolean-field identifier")]
    InvalidIdentifier,
    #[error("Boolean-field digest must not be zero")]
    ZeroDigest,
    #[error("invalid target level")]
    InvalidTargetLevel,
    #[error("wrong canonical cell count")]
    WrongCellCount,
    #[error("noncanonical coverage")]
    NonCanonicalCoverage,
    #[error("invalid source support")]
    InvalidSupport,
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
    fn packed_boolean_field_round_trips_and_rejects_impossible_support() {
        let container: S2CellId = "1000010000000000".parse().expect("valid cell");
        let mut tile = PackedBooleanFieldTile {
            tile_schema_version: 1,
            layer_id: "land-coverage".to_owned(),
            source_snapshot_digest: Digest::sha256(b"s"),
            source_artifact_digest: Digest::sha256(b"a"),
            sample_policy: "source-points".to_owned(),
            container_s2_cell_id: container,
            target_s2_level: 11,
            cells: container
                .children()
                .expect("children")
                .into_iter()
                .map(|s2_cell_id| BooleanFieldCell {
                    s2_cell_id,
                    support_samples: 4,
                    true_samples: 3,
                })
                .collect(),
        };
        let bytes = tile.canonical_bytes().expect("canonical bytes");
        assert_eq!(
            PackedBooleanFieldTile::from_canonical_slice(&bytes),
            Ok(tile.clone())
        );
        tile.cells[0].true_samples = 5;
        assert!(matches!(
            tile.validate(),
            Err(BooleanFieldTileError::InvalidSupport)
        ));
    }
}
