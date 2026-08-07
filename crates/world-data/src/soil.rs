//! Packed SoilGrids topsoil evidence without premature unit interpretation.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, MAX_S2_LEVEL, S2CellId};

pub const PACKED_SOILGRIDS_TOPSOIL_TILE_SCHEMA_VERSION: u16 = 1;
pub const PACKED_SOILGRIDS_TOPSOIL_TILE_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.packed-soilgrids-topsoil-tile+json";
pub const SOILGRIDS_NO_DATA_VALUE: i16 = i16::MIN;
pub const SOILGRIDS_TOPSOIL_PROPERTIES: [SoilGridsProperty; 9] = [
    SoilGridsProperty::Bdod,
    SoilGridsProperty::Cec,
    SoilGridsProperty::Cfvo,
    SoilGridsProperty::Clay,
    SoilGridsProperty::Nitrogen,
    SoilGridsProperty::Phh2o,
    SoilGridsProperty::Sand,
    SoilGridsProperty::Silt,
    SoilGridsProperty::Soc,
];
pub const SOILGRIDS_QUANTILES: [SoilGridsQuantile; 3] = [
    SoilGridsQuantile::Q005,
    SoilGridsQuantile::Q050,
    SoilGridsQuantile::Q095,
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SoilGridsProperty {
    Bdod,
    Cec,
    Cfvo,
    Clay,
    Nitrogen,
    Phh2o,
    Sand,
    Silt,
    Soc,
}

impl SoilGridsProperty {
    const fn slug(self) -> &'static str {
        match self {
            Self::Bdod => "bdod",
            Self::Cec => "cec",
            Self::Cfvo => "cfvo",
            Self::Clay => "clay",
            Self::Nitrogen => "nitrogen",
            Self::Phh2o => "phh2o",
            Self::Sand => "sand",
            Self::Silt => "silt",
            Self::Soc => "soc",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SoilGridsQuantile {
    #[serde(rename = "Q0.05")]
    Q005,
    #[serde(rename = "Q0.5")]
    Q050,
    #[serde(rename = "Q0.95")]
    Q095,
}

impl SoilGridsQuantile {
    const fn slug(self) -> &'static str {
        match self {
            Self::Q005 => "Q0.05",
            Self::Q050 => "Q0.5",
            Self::Q095 => "Q0.95",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SoilDepth {
    #[serde(rename = "0-5cm")]
    ZeroToFiveCentimeters,
}

/// The three source values are ordered Q0.05, Q0.5, then Q0.95.
///
/// Values remain exactly in the upstream signed-i16 domain. In particular, -32768 is
/// retained as explicit no-data and is not treated as a numeric soil measurement.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SoilGridsQuantileValues {
    pub q0_05: i16,
    pub q0_5: i16,
    pub q0_95: i16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SoilGridsPropertySource {
    pub property: SoilGridsProperty,
    /// Artifact digests ordered Q0.05, Q0.5, then Q0.95.
    pub quantile_artifact_digests: [Digest; 3],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SoilGridsTopsoilCell {
    pub s2_cell_id: S2CellId,
    pub support_samples: u64,
    /// Values use the exact order declared by `SOILGRIDS_TOPSOIL_PROPERTIES`.
    pub property_values: [SoilGridsQuantileValues; 9],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackedSoilGridsTopsoilTile {
    pub tile_schema_version: u16,
    pub layer_id: String,
    pub depth: SoilDepth,
    pub source_snapshot_digest: Digest,
    /// Binds the ordered property and quantile artifact set as a whole.
    pub source_set_digest: Digest,
    /// Must use the exact order declared by `SOILGRIDS_TOPSOIL_PROPERTIES`.
    pub property_sources: Vec<SoilGridsPropertySource>,
    pub sampling_reprojection_method: String,
    pub container_s2_cell_id: S2CellId,
    pub target_s2_level: u8,
    pub cells: Vec<SoilGridsTopsoilCell>,
}

impl PackedSoilGridsTopsoilTile {
    pub fn validate(&self) -> Result<(), SoilGridsTopsoilTileError> {
        if self.tile_schema_version != PACKED_SOILGRIDS_TOPSOIL_TILE_SCHEMA_VERSION {
            return Err(SoilGridsTopsoilTileError::UnsupportedSchema(
                self.tile_schema_version,
            ));
        }
        if !slug(&self.layer_id) || !slug(&self.sampling_reprojection_method) {
            return Err(SoilGridsTopsoilTileError::InvalidIdentifier);
        }
        if self.source_snapshot_digest == Digest::ZERO || self.source_set_digest == Digest::ZERO {
            return Err(SoilGridsTopsoilTileError::ZeroDigest);
        }
        validate_property_sources(&self.property_sources)?;
        if self.source_set_digest != soilgrids_source_set_digest(&self.property_sources) {
            return Err(SoilGridsTopsoilTileError::SourceSetDigestMismatch);
        }
        if self.target_s2_level > MAX_S2_LEVEL
            || self.target_s2_level <= self.container_s2_cell_id.level()
        {
            return Err(SoilGridsTopsoilTileError::InvalidTargetLevel);
        }
        let expected = descendants(self.container_s2_cell_id, self.target_s2_level)?;
        if self.cells.len() != expected.len() {
            return Err(SoilGridsTopsoilTileError::WrongCellCount);
        }
        for (cell, expected_cell) in self.cells.iter().zip(expected) {
            if cell.s2_cell_id != expected_cell {
                return Err(SoilGridsTopsoilTileError::NonCanonicalCoverage);
            }
            if cell.support_samples == 0 {
                return Err(SoilGridsTopsoilTileError::ZeroSupport);
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SoilGridsTopsoilTileError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| SoilGridsTopsoilTileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, SoilGridsTopsoilTileError> {
        let tile: Self = serde_json::from_slice(bytes)
            .map_err(|error| SoilGridsTopsoilTileError::Decode(error.to_string()))?;
        tile.validate()?;
        if tile.canonical_bytes()? != bytes {
            return Err(SoilGridsTopsoilTileError::NonCanonicalEncoding);
        }
        Ok(tile)
    }
}

/// Computes the domain-separated digest for the exact ordered 9-by-3 artifact set.
#[must_use]
pub fn soilgrids_source_set_digest(sources: &[SoilGridsPropertySource]) -> Digest {
    let mut bytes = b"a-tiny-civilization/soilgrids-topsoil-source-set/v1\0".to_vec();
    for source in sources {
        bytes.extend_from_slice(source.property.slug().as_bytes());
        bytes.push(0);
        for (quantile, digest) in SOILGRIDS_QUANTILES
            .iter()
            .zip(source.quantile_artifact_digests.iter())
        {
            bytes.extend_from_slice(quantile.slug().as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(digest.as_bytes());
        }
    }
    Digest::sha256(&bytes)
}

fn validate_property_sources(
    sources: &[SoilGridsPropertySource],
) -> Result<(), SoilGridsTopsoilTileError> {
    if sources.len() != SOILGRIDS_TOPSOIL_PROPERTIES.len() {
        return Err(SoilGridsTopsoilTileError::InvalidPropertySources);
    }
    for (source, expected_property) in sources.iter().zip(SOILGRIDS_TOPSOIL_PROPERTIES) {
        if source.property != expected_property
            || source.quantile_artifact_digests.contains(&Digest::ZERO)
        {
            return Err(SoilGridsTopsoilTileError::InvalidPropertySources);
        }
    }
    Ok(())
}

fn descendants(root: S2CellId, target: u8) -> Result<Vec<S2CellId>, SoilGridsTopsoilTileError> {
    let mut cells = vec![root];
    while cells.first().is_some_and(|cell| cell.level() < target) {
        let mut next = Vec::with_capacity(
            cells
                .len()
                .checked_mul(4)
                .ok_or(SoilGridsTopsoilTileError::CoverageOverflow)?,
        );
        for cell in cells {
            next.extend(
                cell.children()
                    .map_err(|error| SoilGridsTopsoilTileError::Spatial(error.to_string()))?,
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
pub enum SoilGridsTopsoilTileError {
    #[error("unsupported SoilGrids topsoil schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid SoilGrids topsoil identifier")]
    InvalidIdentifier,
    #[error("SoilGrids topsoil digest must not be zero")]
    ZeroDigest,
    #[error("invalid or noncanonical SoilGrids property sources")]
    InvalidPropertySources,
    #[error("SoilGrids source-set digest does not match its ordered artifacts")]
    SourceSetDigestMismatch,
    #[error("invalid target level")]
    InvalidTargetLevel,
    #[error("wrong canonical cell count")]
    WrongCellCount,
    #[error("noncanonical coverage")]
    NonCanonicalCoverage,
    #[error("soil evidence has zero source support")]
    ZeroSupport,
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

    fn property_sources() -> Vec<SoilGridsPropertySource> {
        SOILGRIDS_TOPSOIL_PROPERTIES
            .iter()
            .copied()
            .enumerate()
            .map(|(property_index, property)| SoilGridsPropertySource {
                property,
                quantile_artifact_digests: std::array::from_fn(|quantile_index| {
                    Digest::sha256(&[property_index as u8, quantile_index as u8])
                }),
            })
            .collect()
    }

    fn tile() -> PackedSoilGridsTopsoilTile {
        let container: S2CellId = "1000010000000000".parse().expect("valid cell");
        let property_sources = property_sources();
        PackedSoilGridsTopsoilTile {
            tile_schema_version: PACKED_SOILGRIDS_TOPSOIL_TILE_SCHEMA_VERSION,
            layer_id: "soilgrids-topsoil".to_owned(),
            depth: SoilDepth::ZeroToFiveCentimeters,
            source_snapshot_digest: Digest::sha256(b"soilgrids snapshot"),
            source_set_digest: soilgrids_source_set_digest(&property_sources),
            property_sources,
            sampling_reprojection_method: "s2-center-to-homolosine-nearest-v1".to_owned(),
            container_s2_cell_id: container,
            target_s2_level: 11,
            cells: container
                .children()
                .expect("children")
                .into_iter()
                .enumerate()
                .map(|(cell_index, s2_cell_id)| SoilGridsTopsoilCell {
                    s2_cell_id,
                    support_samples: 1,
                    property_values: std::array::from_fn(|property_index| {
                        SoilGridsQuantileValues {
                            q0_05: if cell_index == 0 && property_index == 0 {
                                SOILGRIDS_NO_DATA_VALUE
                            } else {
                                property_index as i16
                            },
                            q0_5: property_index as i16 + 10,
                            q0_95: property_index as i16 + 20,
                        }
                    }),
                })
                .collect(),
        }
    }

    #[test]
    fn soilgrids_topsoil_tile_round_trips_canonically_and_retains_no_data() {
        let tile = tile();
        let bytes = tile.canonical_bytes().expect("canonical tile");
        let decoded =
            PackedSoilGridsTopsoilTile::from_canonical_slice(&bytes).expect("canonical decode");
        assert_eq!(decoded, tile);
        assert_eq!(
            decoded.cells[0].property_values[0].q0_05,
            SOILGRIDS_NO_DATA_VALUE
        );

        let pretty = serde_json::to_vec_pretty(&tile).expect("pretty JSON");
        assert_eq!(
            PackedSoilGridsTopsoilTile::from_canonical_slice(&pretty),
            Err(SoilGridsTopsoilTileError::NonCanonicalEncoding)
        );
    }

    #[test]
    fn soilgrids_topsoil_rejects_wrong_property_or_quantile_provenance() {
        let mut invalid = tile();
        invalid.property_sources.swap(0, 1);
        assert_eq!(
            invalid.validate(),
            Err(SoilGridsTopsoilTileError::InvalidPropertySources)
        );

        let mut invalid = tile();
        invalid.property_sources[0]
            .quantile_artifact_digests
            .swap(0, 1);
        assert_eq!(
            invalid.validate(),
            Err(SoilGridsTopsoilTileError::SourceSetDigestMismatch)
        );

        let mut invalid = tile();
        invalid.property_sources[0].quantile_artifact_digests[0] = Digest::ZERO;
        assert_eq!(
            invalid.validate(),
            Err(SoilGridsTopsoilTileError::InvalidPropertySources)
        );
    }

    #[test]
    fn soilgrids_topsoil_rejects_zero_support_and_noncanonical_cells() {
        let mut invalid = tile();
        invalid.cells[0].support_samples = 0;
        assert_eq!(
            invalid.validate(),
            Err(SoilGridsTopsoilTileError::ZeroSupport)
        );

        let mut invalid = tile();
        invalid.cells.swap(0, 1);
        assert_eq!(
            invalid.validate(),
            Err(SoilGridsTopsoilTileError::NonCanonicalCoverage)
        );
    }
}
