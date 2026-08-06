use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Digest;

pub const WORLD_CONFIGURATION_SCHEMA_VERSION: u16 = 1;
const SECONDS_PER_DAY: u32 = 86_400;
const MAX_V1_GRID_CELLS: u64 = 1_000_000;

/// Content-addressed normalized inputs used by a configured world.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldDataBundleReference {
    pub bundle_schema_version: u16,
    pub bundle_id: String,
    pub bundle_version: String,
    pub content_hash: Digest,
    pub download_url: String,
    pub license_expression: String,
}

impl WorldDataBundleReference {
    pub fn new(
        bundle_schema_version: u16,
        bundle_id: impl Into<String>,
        bundle_version: impl Into<String>,
        content_hash: Digest,
        download_url: impl Into<String>,
        license_expression: impl Into<String>,
    ) -> Result<Self, WorldConfigurationError> {
        let reference = Self {
            bundle_schema_version,
            bundle_id: bundle_id.into(),
            bundle_version: bundle_version.into(),
            content_hash,
            download_url: download_url.into(),
            license_expression: license_expression.into(),
        };
        reference.validate()?;
        Ok(reference)
    }

    pub fn validate(&self) -> Result<(), WorldConfigurationError> {
        if self.bundle_schema_version == 0 {
            return Err(WorldConfigurationError::ZeroBundleSchemaVersion);
        }
        if !is_slug(&self.bundle_id) {
            return Err(WorldConfigurationError::InvalidBundleId);
        }
        if self.bundle_version.trim().is_empty() {
            return Err(WorldConfigurationError::MissingBundleVersion);
        }
        if self.content_hash == Digest::ZERO {
            return Err(WorldConfigurationError::ZeroBundleHash);
        }
        if !self.download_url.starts_with("https://") {
            return Err(WorldConfigurationError::NonHttpsBundleUrl);
        }
        if self.license_expression.trim().is_empty() {
            return Err(WorldConfigurationError::MissingLicenseExpression);
        }
        Ok(())
    }
}

/// Integer raster geometry aligned to a real projected coordinate reference system.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpatialGrid {
    pub epsg: u32,
    pub origin_easting_mm: i64,
    pub origin_northing_mm: i64,
    pub cell_size_mm: u32,
    pub width_cells: u32,
    pub height_cells: u32,
}

impl SpatialGrid {
    pub fn validate(&self) -> Result<(), WorldConfigurationError> {
        if self.epsg == 0 {
            return Err(WorldConfigurationError::ZeroEpsgCode);
        }
        if self.cell_size_mm == 0 || self.width_cells == 0 || self.height_cells == 0 {
            return Err(WorldConfigurationError::ZeroGridDimension);
        }
        if self.cell_count() > MAX_V1_GRID_CELLS {
            return Err(WorldConfigurationError::GridTooLarge {
                cells: self.cell_count(),
                maximum: MAX_V1_GRID_CELLS,
            });
        }
        Ok(())
    }

    #[must_use]
    pub fn cell_count(&self) -> u64 {
        u64::from(self.width_cells) * u64::from(self.height_cells)
    }
}

/// Immutable causal scale and data inputs committed at tick zero.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldConfiguration {
    pub configuration_schema_version: u16,
    pub tick_duration_seconds: u32,
    pub spatial_grid: SpatialGrid,
    pub world_data: WorldDataBundleReference,
    pub max_events_per_transition: u32,
}

impl WorldConfiguration {
    pub fn new(
        tick_duration_seconds: u32,
        spatial_grid: SpatialGrid,
        world_data: WorldDataBundleReference,
        max_events_per_transition: u32,
    ) -> Result<Self, WorldConfigurationError> {
        let configuration = Self {
            configuration_schema_version: WORLD_CONFIGURATION_SCHEMA_VERSION,
            tick_duration_seconds,
            spatial_grid,
            world_data,
            max_events_per_transition,
        };
        configuration.validate()?;
        Ok(configuration)
    }

    pub fn validate(&self) -> Result<(), WorldConfigurationError> {
        if self.configuration_schema_version != WORLD_CONFIGURATION_SCHEMA_VERSION {
            return Err(WorldConfigurationError::UnsupportedConfigurationSchema(
                self.configuration_schema_version,
            ));
        }
        if self.tick_duration_seconds == 0
            || !SECONDS_PER_DAY.is_multiple_of(self.tick_duration_seconds)
        {
            return Err(WorldConfigurationError::InvalidTickDuration);
        }
        if self.max_events_per_transition == 0 {
            return Err(WorldConfigurationError::ZeroEventBudget);
        }
        self.spatial_grid.validate()?;
        self.world_data.validate()?;
        Ok(())
    }
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WorldConfigurationError {
    #[error("world-data bundle schema version must be greater than zero")]
    ZeroBundleSchemaVersion,
    #[error("world-data bundle identifier must be a lowercase ASCII slug")]
    InvalidBundleId,
    #[error("world-data bundle version is required")]
    MissingBundleVersion,
    #[error("world-data bundle content hash must not be zero")]
    ZeroBundleHash,
    #[error("world-data bundle download URL must use HTTPS")]
    NonHttpsBundleUrl,
    #[error("world-data bundle license expression is required")]
    MissingLicenseExpression,
    #[error("spatial grid EPSG code must be greater than zero")]
    ZeroEpsgCode,
    #[error("spatial grid cell size, width, and height must be greater than zero")]
    ZeroGridDimension,
    #[error("spatial grid has {cells} cells; schema v1 permits at most {maximum}")]
    GridTooLarge { cells: u64, maximum: u64 },
    #[error("world configuration schema version {0} is unsupported")]
    UnsupportedConfigurationSchema(u16),
    #[error("tick duration must be a positive whole-second divisor of one solar day")]
    InvalidTickDuration,
    #[error("maximum events per transition must be greater than zero")]
    ZeroEventBudget,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle() -> WorldDataBundleReference {
        WorldDataBundleReference::new(
            1,
            "bounded-biome-test",
            "0.1.0",
            Digest::from_bytes([7; 32]),
            "https://data.atinycivilization.com/bounded-biome-test/0.1.0.json",
            "CC-BY-4.0",
        )
        .expect("valid test bundle")
    }

    fn grid() -> SpatialGrid {
        SpatialGrid {
            epsg: 32_736,
            origin_easting_mm: 500_000_000,
            origin_northing_mm: 9_700_000_000,
            cell_size_mm: 10_000,
            width_cells: 100,
            height_cells: 100,
        }
    }

    #[test]
    fn validates_content_addressed_integer_world_scale() {
        let configuration =
            WorldConfiguration::new(300, grid(), bundle(), 10_000).expect("valid configured world");
        assert_eq!(configuration.spatial_grid.cell_count(), 10_000);

        let mut invalid_tick = configuration.clone();
        invalid_tick.tick_duration_seconds = 301;
        assert_eq!(
            invalid_tick.validate(),
            Err(WorldConfigurationError::InvalidTickDuration)
        );

        let mut invalid_hash = configuration;
        invalid_hash.world_data.content_hash = Digest::ZERO;
        assert_eq!(
            invalid_hash.validate(),
            Err(WorldConfigurationError::ZeroBundleHash)
        );
    }
}
