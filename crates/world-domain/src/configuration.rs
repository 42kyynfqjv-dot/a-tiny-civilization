use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::{
    Digest, MAX_S2_LEVEL, ProvisionalLocalEnvironmentBaseline, ProvisionalLocalWeatherBaseline,
};

pub const LEGACY_WORLD_CONFIGURATION_SCHEMA_VERSION: u16 = 1;
/// Scientifically admitted full-Earth configuration schema.
pub const WORLD_CONFIGURATION_SCHEMA_VERSION: u16 = 2;
/// Full-Earth execution proof whose inputs are explicitly not scientifically admitted.
pub const PROVISIONAL_WORLD_CONFIGURATION_SCHEMA_VERSION: u16 = 3;
/// Provisional execution with an immutable source-bound local environment baseline.
pub const PROVISIONAL_ENVIRONMENT_WORLD_CONFIGURATION_SCHEMA_VERSION: u16 = 4;
/// Provisional execution with source-bound environmental and weather inputs.
pub const PROVISIONAL_WEATHER_WORLD_CONFIGURATION_SCHEMA_VERSION: u16 = 5;
const SECONDS_PER_DAY: u32 = 86_400;
const MAX_V1_GRID_CELLS: u64 = 1_000_000;
const WGS_84_ECEF_EPSG: u32 = 4_978;
const WGS_84_3D_EPSG: u32 = 4_979;
const EGM_2008_HEIGHT_EPSG: u32 = 3_855;

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
        if !is_https_url(&self.download_url) {
            return Err(WorldConfigurationError::NonHttpsBundleUrl);
        }
        if self.license_expression.trim().is_empty() {
            return Err(WorldConfigurationError::MissingLicenseExpression);
        }
        Ok(())
    }
}

/// Content address for a complete provisional composition used by an execution proof.
///
/// This is deliberately not a [`WorldDataBundleReference`]. A caller cannot obtain an
/// admitted-bundle reference by changing a status bit or reusing these fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalWorldCompositionReference {
    pub composition_schema_version: u16,
    pub composition_id: String,
    pub composition_version: String,
    pub content_hash: Digest,
}

impl ProvisionalWorldCompositionReference {
    pub fn new(
        composition_schema_version: u16,
        composition_id: impl Into<String>,
        composition_version: impl Into<String>,
        content_hash: Digest,
    ) -> Result<Self, WorldConfigurationError> {
        let reference = Self {
            composition_schema_version,
            composition_id: composition_id.into(),
            composition_version: composition_version.into(),
            content_hash,
        };
        reference.validate()?;
        Ok(reference)
    }

    pub fn validate(&self) -> Result<(), WorldConfigurationError> {
        if self.composition_schema_version == 0 {
            return Err(WorldConfigurationError::ZeroCompositionSchemaVersion);
        }
        if !is_slug(&self.composition_id) {
            return Err(WorldConfigurationError::InvalidCompositionId);
        }
        if !is_semantic_version(&self.composition_version) {
            return Err(WorldConfigurationError::InvalidCompositionVersion);
        }
        if self.content_hash == Digest::ZERO {
            return Err(WorldConfigurationError::ZeroCompositionHash);
        }
        Ok(())
    }
}

/// Mutually exclusive scientific input classes committed at tick zero.
///
/// The distinct field names are part of the wire contract: provisional input cannot
/// decode through the admitted `world_data` path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WorldInputReference {
    ScientificallyAdmitted {
        world_data: WorldDataBundleReference,
    },
    ProvisionalExecution {
        provisional_world_composition: ProvisionalWorldCompositionReference,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmittedWorldInputWire {
    world_data: WorldDataBundleReference,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvisionalWorldInputWire {
    provisional_world_composition: ProvisionalWorldCompositionReference,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WorldInputReferenceWire {
    ScientificallyAdmitted(AdmittedWorldInputWire),
    ProvisionalExecution(ProvisionalWorldInputWire),
}

impl<'de> Deserialize<'de> for WorldInputReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match WorldInputReferenceWire::deserialize(deserializer)? {
            WorldInputReferenceWire::ScientificallyAdmitted(wire) => Self::ScientificallyAdmitted {
                world_data: wire.world_data,
            },
            WorldInputReferenceWire::ProvisionalExecution(wire) => Self::ProvisionalExecution {
                provisional_world_composition: wire.provisional_world_composition,
            },
        })
    }
}

impl WorldInputReference {
    fn validate(&self) -> Result<(), WorldConfigurationError> {
        match self {
            Self::ScientificallyAdmitted { world_data } => world_data.validate(),
            Self::ProvisionalExecution {
                provisional_world_composition,
            } => provisional_world_composition.validate(),
        }
    }

    #[must_use]
    pub const fn world_data(&self) -> Option<&WorldDataBundleReference> {
        match self {
            Self::ScientificallyAdmitted { world_data } => Some(world_data),
            Self::ProvisionalExecution { .. } => None,
        }
    }

    #[must_use]
    pub const fn provisional_world_composition(
        &self,
    ) -> Option<&ProvisionalWorldCompositionReference> {
        match self {
            Self::ScientificallyAdmitted { .. } => None,
            Self::ProvisionalExecution {
                provisional_world_composition,
            } => Some(provisional_world_composition),
        }
    }
}

/// Legacy bounded integer raster retained so published schema-v1 configurations decode.
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

/// Full-Earth addressing and physics frames pinned before canonical genesis.
///
/// S2 is an address hierarchy only. Distances and local physics use WGS 84 ECEF
/// and deterministic local east/north/up frames rather than S2's unit sphere.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FullEarthGrid {
    pub physics_crs_epsg: u32,
    pub catalog_crs_epsg: u32,
    pub vertical_crs_epsg: u32,
    pub s2_definition_url: String,
    pub s2_library_revision: String,
    pub s2_definition_hash: Digest,
    pub s2_projection: S2Projection,
    pub levels: EarthResolutionLevels,
    pub refinement_policy_version: u16,
}

impl FullEarthGrid {
    pub fn validate(&self) -> Result<(), WorldConfigurationError> {
        if self.physics_crs_epsg != WGS_84_ECEF_EPSG
            || self.catalog_crs_epsg != WGS_84_3D_EPSG
            || self.vertical_crs_epsg != EGM_2008_HEIGHT_EPSG
        {
            return Err(WorldConfigurationError::UnsupportedEarthReferenceFrames);
        }
        if !is_https_url(&self.s2_definition_url) {
            return Err(WorldConfigurationError::NonHttpsS2DefinitionUrl);
        }
        if self.s2_library_revision.len() < 12
            || !self
                .s2_library_revision
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(WorldConfigurationError::InvalidS2LibraryRevision);
        }
        if self.s2_definition_hash == Digest::ZERO {
            return Err(WorldConfigurationError::ZeroS2DefinitionHash);
        }
        self.levels.validate()?;
        if self.refinement_policy_version == 0 {
            return Err(WorldConfigurationError::ZeroRefinementPolicyVersion);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum S2Projection {
    Quadratic,
}

/// Resolution roles, not claims that every source was measured at the finest level.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EarthResolutionLevels {
    pub planetary_aggregate: u8,
    pub regional_ecology: u8,
    pub active_landscape: u8,
    pub embodied_patch: u8,
}

impl EarthResolutionLevels {
    fn validate(&self) -> Result<(), WorldConfigurationError> {
        let levels = [
            self.planetary_aggregate,
            self.regional_ecology,
            self.active_landscape,
            self.embodied_patch,
        ];
        if levels.iter().any(|level| *level > MAX_S2_LEVEL)
            || !levels.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(WorldConfigurationError::InvalidEarthResolutionLevels);
        }
        Ok(())
    }
}

/// Spatial representation encoded without changing schema-v1 JSON field names.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WorldGeometry {
    BoundedRaster { spatial_grid: SpatialGrid },
    FullEarth { full_earth_grid: FullEarthGrid },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonRepresentation {
    DurableIndividuals,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerKind {
    DeterministicEventQueue,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityExhaustionPolicy {
    PauseAtCommittedBoundary,
}

/// Schema-v2 work partitioning. Partition ownership and worker count are operational;
/// they may change wall-clock throughput but not ordered causal results.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PartitionedExecution {
    pub scheduler_schema_version: u16,
    pub scheduler: SchedulerKind,
    pub partition_s2_level: u8,
    pub person_representation: PersonRepresentation,
    pub capacity_exhaustion: CapacityExhaustionPolicy,
    pub max_events_per_partition_transition: u32,
}

impl PartitionedExecution {
    fn validate(&self, grid: &FullEarthGrid) -> Result<(), WorldConfigurationError> {
        if self.scheduler_schema_version == 0 {
            return Err(WorldConfigurationError::ZeroSchedulerSchemaVersion);
        }
        if self.partition_s2_level != grid.levels.planetary_aggregate {
            return Err(WorldConfigurationError::PartitionLevelMismatch);
        }
        if self.max_events_per_partition_transition == 0 {
            return Err(WorldConfigurationError::ZeroEventBudget);
        }
        Ok(())
    }
}

/// Execution semantics encoded without changing schema-v1 JSON field names.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ExecutionScale {
    SingleTransition {
        max_events_per_transition: u32,
    },
    Partitioned {
        partitioned_execution: PartitionedExecution,
    },
}

/// Immutable causal scale and data inputs committed at tick zero.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorldConfiguration {
    pub configuration_schema_version: u16,
    pub tick_duration_seconds: u32,
    #[serde(flatten)]
    pub geometry: WorldGeometry,
    #[serde(flatten)]
    pub input: WorldInputReference,
    #[serde(flatten)]
    pub execution: ExecutionScale,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_environment_baseline: Option<Box<ProvisionalLocalEnvironmentBaseline>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_weather_baseline: Option<Box<ProvisionalLocalWeatherBaseline>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyWorldConfigurationWire {
    configuration_schema_version: u16,
    tick_duration_seconds: u32,
    spatial_grid: SpatialGrid,
    world_data: WorldDataBundleReference,
    max_events_per_transition: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FullEarthWorldConfigurationWire {
    configuration_schema_version: u16,
    tick_duration_seconds: u32,
    full_earth_grid: FullEarthGrid,
    world_data: WorldDataBundleReference,
    partitioned_execution: PartitionedExecution,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvisionalFullEarthWorldConfigurationWire {
    configuration_schema_version: u16,
    tick_duration_seconds: u32,
    full_earth_grid: FullEarthGrid,
    provisional_world_composition: ProvisionalWorldCompositionReference,
    partitioned_execution: PartitionedExecution,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvisionalEnvironmentalWorldConfigurationWire {
    configuration_schema_version: u16,
    tick_duration_seconds: u32,
    full_earth_grid: FullEarthGrid,
    provisional_world_composition: ProvisionalWorldCompositionReference,
    partitioned_execution: PartitionedExecution,
    local_environment_baseline: Box<ProvisionalLocalEnvironmentBaseline>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvisionalWeatherWorldConfigurationWire {
    configuration_schema_version: u16,
    tick_duration_seconds: u32,
    full_earth_grid: FullEarthGrid,
    provisional_world_composition: ProvisionalWorldCompositionReference,
    partitioned_execution: PartitionedExecution,
    local_environment_baseline: Box<ProvisionalLocalEnvironmentBaseline>,
    local_weather_baseline: Box<ProvisionalLocalWeatherBaseline>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WorldConfigurationWire {
    Legacy(LegacyWorldConfigurationWire),
    FullEarth(FullEarthWorldConfigurationWire),
    ProvisionalFullEarth(ProvisionalFullEarthWorldConfigurationWire),
    ProvisionalEnvironmental(Box<ProvisionalEnvironmentalWorldConfigurationWire>),
    ProvisionalWeather(Box<ProvisionalWeatherWorldConfigurationWire>),
}

impl<'de> Deserialize<'de> for WorldConfiguration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let configuration = match WorldConfigurationWire::deserialize(deserializer)? {
            WorldConfigurationWire::Legacy(wire) => Self {
                configuration_schema_version: wire.configuration_schema_version,
                tick_duration_seconds: wire.tick_duration_seconds,
                geometry: WorldGeometry::BoundedRaster {
                    spatial_grid: wire.spatial_grid,
                },
                input: WorldInputReference::ScientificallyAdmitted {
                    world_data: wire.world_data,
                },
                execution: ExecutionScale::SingleTransition {
                    max_events_per_transition: wire.max_events_per_transition,
                },
                local_environment_baseline: None,
                local_weather_baseline: None,
            },
            WorldConfigurationWire::FullEarth(wire) => Self {
                configuration_schema_version: wire.configuration_schema_version,
                tick_duration_seconds: wire.tick_duration_seconds,
                geometry: WorldGeometry::FullEarth {
                    full_earth_grid: wire.full_earth_grid,
                },
                input: WorldInputReference::ScientificallyAdmitted {
                    world_data: wire.world_data,
                },
                execution: ExecutionScale::Partitioned {
                    partitioned_execution: wire.partitioned_execution,
                },
                local_environment_baseline: None,
                local_weather_baseline: None,
            },
            WorldConfigurationWire::ProvisionalFullEarth(wire) => Self {
                configuration_schema_version: wire.configuration_schema_version,
                tick_duration_seconds: wire.tick_duration_seconds,
                geometry: WorldGeometry::FullEarth {
                    full_earth_grid: wire.full_earth_grid,
                },
                input: WorldInputReference::ProvisionalExecution {
                    provisional_world_composition: wire.provisional_world_composition,
                },
                execution: ExecutionScale::Partitioned {
                    partitioned_execution: wire.partitioned_execution,
                },
                local_environment_baseline: None,
                local_weather_baseline: None,
            },
            WorldConfigurationWire::ProvisionalEnvironmental(wire) => Self {
                configuration_schema_version: wire.configuration_schema_version,
                tick_duration_seconds: wire.tick_duration_seconds,
                geometry: WorldGeometry::FullEarth {
                    full_earth_grid: wire.full_earth_grid,
                },
                input: WorldInputReference::ProvisionalExecution {
                    provisional_world_composition: wire.provisional_world_composition,
                },
                execution: ExecutionScale::Partitioned {
                    partitioned_execution: wire.partitioned_execution,
                },
                local_environment_baseline: Some(wire.local_environment_baseline),
                local_weather_baseline: None,
            },
            WorldConfigurationWire::ProvisionalWeather(wire) => Self {
                configuration_schema_version: wire.configuration_schema_version,
                tick_duration_seconds: wire.tick_duration_seconds,
                geometry: WorldGeometry::FullEarth {
                    full_earth_grid: wire.full_earth_grid,
                },
                input: WorldInputReference::ProvisionalExecution {
                    provisional_world_composition: wire.provisional_world_composition,
                },
                execution: ExecutionScale::Partitioned {
                    partitioned_execution: wire.partitioned_execution,
                },
                local_environment_baseline: Some(wire.local_environment_baseline),
                local_weather_baseline: Some(wire.local_weather_baseline),
            },
        };
        configuration.validate().map_err(serde::de::Error::custom)?;
        Ok(configuration)
    }
}

impl WorldConfiguration {
    /// Construct a legacy bounded schema-v1 configuration.
    pub fn new(
        tick_duration_seconds: u32,
        spatial_grid: SpatialGrid,
        world_data: WorldDataBundleReference,
        max_events_per_transition: u32,
    ) -> Result<Self, WorldConfigurationError> {
        let configuration = Self {
            configuration_schema_version: LEGACY_WORLD_CONFIGURATION_SCHEMA_VERSION,
            tick_duration_seconds,
            geometry: WorldGeometry::BoundedRaster { spatial_grid },
            input: WorldInputReference::ScientificallyAdmitted { world_data },
            execution: ExecutionScale::SingleTransition {
                max_events_per_transition,
            },
            local_environment_baseline: None,
            local_weather_baseline: None,
        };
        configuration.validate()?;
        Ok(configuration)
    }

    /// The only admissible durable organism-position level for a full-Earth world.
    /// Legacy bounded configurations deliberately have no S2 embodied-patch address.
    #[must_use]
    pub fn embodied_patch_s2_level(&self) -> Option<u8> {
        match &self.geometry {
            WorldGeometry::BoundedRaster { .. } => None,
            WorldGeometry::FullEarth { full_earth_grid } => {
                Some(full_earth_grid.levels.embodied_patch)
            }
        }
    }

    /// Construct the schema-v2 full-Earth configuration required for a public world.
    pub fn new_full_earth(
        tick_duration_seconds: u32,
        full_earth_grid: FullEarthGrid,
        world_data: WorldDataBundleReference,
        partitioned_execution: PartitionedExecution,
    ) -> Result<Self, WorldConfigurationError> {
        let configuration = Self {
            configuration_schema_version: WORLD_CONFIGURATION_SCHEMA_VERSION,
            tick_duration_seconds,
            geometry: WorldGeometry::FullEarth { full_earth_grid },
            input: WorldInputReference::ScientificallyAdmitted { world_data },
            execution: ExecutionScale::Partitioned {
                partitioned_execution,
            },
            local_environment_baseline: None,
            local_weather_baseline: None,
        };
        configuration.validate()?;
        Ok(configuration)
    }

    /// Construct a schema-v3 full-Earth execution proof over explicitly provisional
    /// composition inputs. This never converts those inputs into an admitted bundle.
    pub fn new_provisional_full_earth(
        tick_duration_seconds: u32,
        full_earth_grid: FullEarthGrid,
        provisional_world_composition: ProvisionalWorldCompositionReference,
        partitioned_execution: PartitionedExecution,
    ) -> Result<Self, WorldConfigurationError> {
        let configuration = Self {
            configuration_schema_version: PROVISIONAL_WORLD_CONFIGURATION_SCHEMA_VERSION,
            tick_duration_seconds,
            geometry: WorldGeometry::FullEarth { full_earth_grid },
            input: WorldInputReference::ProvisionalExecution {
                provisional_world_composition,
            },
            execution: ExecutionScale::Partitioned {
                partitioned_execution,
            },
            local_environment_baseline: None,
            local_weather_baseline: None,
        };
        configuration.validate()?;
        Ok(configuration)
    }

    pub fn new_provisional_full_earth_with_environment_baseline(
        tick_duration_seconds: u32,
        full_earth_grid: FullEarthGrid,
        provisional_world_composition: ProvisionalWorldCompositionReference,
        partitioned_execution: PartitionedExecution,
        local_environment_baseline: ProvisionalLocalEnvironmentBaseline,
    ) -> Result<Self, WorldConfigurationError> {
        let configuration = Self {
            configuration_schema_version:
                PROVISIONAL_ENVIRONMENT_WORLD_CONFIGURATION_SCHEMA_VERSION,
            tick_duration_seconds,
            geometry: WorldGeometry::FullEarth { full_earth_grid },
            input: WorldInputReference::ProvisionalExecution {
                provisional_world_composition,
            },
            execution: ExecutionScale::Partitioned {
                partitioned_execution,
            },
            local_environment_baseline: Some(Box::new(local_environment_baseline)),
            local_weather_baseline: None,
        };
        configuration.validate()?;
        Ok(configuration)
    }

    pub fn new_provisional_full_earth_with_weather_baseline(
        tick_duration_seconds: u32,
        full_earth_grid: FullEarthGrid,
        provisional_world_composition: ProvisionalWorldCompositionReference,
        partitioned_execution: PartitionedExecution,
        local_environment_baseline: ProvisionalLocalEnvironmentBaseline,
        local_weather_baseline: ProvisionalLocalWeatherBaseline,
    ) -> Result<Self, WorldConfigurationError> {
        let configuration = Self {
            configuration_schema_version: PROVISIONAL_WEATHER_WORLD_CONFIGURATION_SCHEMA_VERSION,
            tick_duration_seconds,
            geometry: WorldGeometry::FullEarth { full_earth_grid },
            input: WorldInputReference::ProvisionalExecution {
                provisional_world_composition,
            },
            execution: ExecutionScale::Partitioned {
                partitioned_execution,
            },
            local_environment_baseline: Some(Box::new(local_environment_baseline)),
            local_weather_baseline: Some(Box::new(local_weather_baseline)),
        };
        configuration.validate()?;
        Ok(configuration)
    }

    pub fn validate(&self) -> Result<(), WorldConfigurationError> {
        if self.tick_duration_seconds == 0
            || !SECONDS_PER_DAY.is_multiple_of(self.tick_duration_seconds)
        {
            return Err(WorldConfigurationError::InvalidTickDuration);
        }
        self.input.validate()?;

        match (&self.geometry, &self.input, &self.execution) {
            (
                WorldGeometry::BoundedRaster { spatial_grid },
                WorldInputReference::ScientificallyAdmitted { world_data },
                ExecutionScale::SingleTransition {
                    max_events_per_transition,
                },
            ) if self.configuration_schema_version == LEGACY_WORLD_CONFIGURATION_SCHEMA_VERSION => {
                if world_data.bundle_schema_version != LEGACY_WORLD_CONFIGURATION_SCHEMA_VERSION {
                    return Err(WorldConfigurationError::BundleSchemaMismatch);
                }
                spatial_grid.validate()?;
                if *max_events_per_transition == 0 {
                    return Err(WorldConfigurationError::ZeroEventBudget);
                }
            }
            (
                WorldGeometry::FullEarth { full_earth_grid },
                WorldInputReference::ScientificallyAdmitted { world_data },
                ExecutionScale::Partitioned {
                    partitioned_execution,
                },
            ) if self.configuration_schema_version == WORLD_CONFIGURATION_SCHEMA_VERSION => {
                if world_data.bundle_schema_version != WORLD_CONFIGURATION_SCHEMA_VERSION {
                    return Err(WorldConfigurationError::BundleSchemaMismatch);
                }
                full_earth_grid.validate()?;
                partitioned_execution.validate(full_earth_grid)?;
            }
            (
                WorldGeometry::FullEarth { full_earth_grid },
                WorldInputReference::ProvisionalExecution { .. },
                ExecutionScale::Partitioned {
                    partitioned_execution,
                },
            ) if self.configuration_schema_version
                == PROVISIONAL_WORLD_CONFIGURATION_SCHEMA_VERSION =>
            {
                if self.local_environment_baseline.is_some()
                    || self.local_weather_baseline.is_some()
                {
                    return Err(WorldConfigurationError::ConfigurationShapeMismatch {
                        schema: self.configuration_schema_version,
                    });
                }
                full_earth_grid.validate()?;
                partitioned_execution.validate(full_earth_grid)?;
            }
            (
                WorldGeometry::FullEarth { full_earth_grid },
                WorldInputReference::ProvisionalExecution { .. },
                ExecutionScale::Partitioned {
                    partitioned_execution,
                },
            ) if self.configuration_schema_version
                == PROVISIONAL_ENVIRONMENT_WORLD_CONFIGURATION_SCHEMA_VERSION =>
            {
                if self.local_weather_baseline.is_some() {
                    return Err(WorldConfigurationError::ConfigurationShapeMismatch {
                        schema: self.configuration_schema_version,
                    });
                }
                let baseline = self.local_environment_baseline.as_ref().ok_or(
                    WorldConfigurationError::ConfigurationShapeMismatch {
                        schema: self.configuration_schema_version,
                    },
                )?;
                full_earth_grid.validate()?;
                partitioned_execution.validate(full_earth_grid)?;
                baseline.validate().map_err(|error| {
                    WorldConfigurationError::InvalidLocalEnvironment(error.to_string())
                })?;
                if baseline.active_patch.level() != full_earth_grid.levels.embodied_patch {
                    return Err(WorldConfigurationError::LocalEnvironmentPatchLevelMismatch);
                }
            }
            (
                WorldGeometry::FullEarth { full_earth_grid },
                WorldInputReference::ProvisionalExecution { .. },
                ExecutionScale::Partitioned {
                    partitioned_execution,
                },
            ) if self.configuration_schema_version
                == PROVISIONAL_WEATHER_WORLD_CONFIGURATION_SCHEMA_VERSION =>
            {
                let environment = self.local_environment_baseline.as_ref().ok_or(
                    WorldConfigurationError::ConfigurationShapeMismatch {
                        schema: self.configuration_schema_version,
                    },
                )?;
                let weather = self.local_weather_baseline.as_ref().ok_or(
                    WorldConfigurationError::ConfigurationShapeMismatch {
                        schema: self.configuration_schema_version,
                    },
                )?;
                full_earth_grid.validate()?;
                partitioned_execution.validate(full_earth_grid)?;
                environment.validate().map_err(|error| {
                    WorldConfigurationError::InvalidLocalEnvironment(error.to_string())
                })?;
                weather.validate().map_err(|error| {
                    WorldConfigurationError::InvalidLocalWeather(error.to_string())
                })?;
                if environment.active_patch.level() != full_earth_grid.levels.embodied_patch
                    || weather.active_patch.level() != full_earth_grid.levels.embodied_patch
                    || weather.active_patch != environment.active_patch
                    || weather.evidence_patch != environment.evidence_patch
                {
                    return Err(WorldConfigurationError::LocalEnvironmentPatchLevelMismatch);
                }
            }
            _ => {
                return Err(WorldConfigurationError::ConfigurationShapeMismatch {
                    schema: self.configuration_schema_version,
                });
            }
        }
        Ok(())
    }

    #[must_use]
    pub const fn spatial_grid(&self) -> Option<&SpatialGrid> {
        match &self.geometry {
            WorldGeometry::BoundedRaster { spatial_grid } => Some(spatial_grid),
            WorldGeometry::FullEarth { .. } => None,
        }
    }

    #[must_use]
    pub const fn full_earth_grid(&self) -> Option<&FullEarthGrid> {
        match &self.geometry {
            WorldGeometry::BoundedRaster { .. } => None,
            WorldGeometry::FullEarth { full_earth_grid } => Some(full_earth_grid),
        }
    }

    /// Admitted normalized bundle input, absent for a provisional execution proof.
    #[must_use]
    pub const fn world_data(&self) -> Option<&WorldDataBundleReference> {
        self.input.world_data()
    }

    /// Provisional composition input, absent for admitted and legacy configurations.
    #[must_use]
    pub const fn provisional_world_composition(
        &self,
    ) -> Option<&ProvisionalWorldCompositionReference> {
        self.input.provisional_world_composition()
    }

    #[must_use]
    pub const fn is_provisional_execution(&self) -> bool {
        matches!(self.input, WorldInputReference::ProvisionalExecution { .. })
    }

    #[must_use]
    pub fn local_environment_baseline(&self) -> Option<&ProvisionalLocalEnvironmentBaseline> {
        self.local_environment_baseline.as_deref()
    }

    #[must_use]
    pub fn local_weather_baseline(&self) -> Option<&ProvisionalLocalWeatherBaseline> {
        self.local_weather_baseline.as_deref()
    }

    /// Durable partition semantics for a full-Earth configuration. Operational
    /// worker count is intentionally absent: it cannot affect this causal contract.
    #[must_use]
    pub const fn partitioned_execution(&self) -> Option<&PartitionedExecution> {
        match &self.execution {
            ExecutionScale::SingleTransition { .. } => None,
            ExecutionScale::Partitioned {
                partitioned_execution,
            } => Some(partitioned_execution),
        }
    }

    #[must_use]
    pub const fn transition_event_limit(&self) -> u32 {
        match &self.execution {
            ExecutionScale::SingleTransition {
                max_events_per_transition,
            } => *max_events_per_transition,
            ExecutionScale::Partitioned {
                partitioned_execution,
            } => partitioned_execution.max_events_per_partition_transition,
        }
    }
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn is_https_url(value: &str) -> bool {
    value.starts_with("https://") && value.len() > "https://".len()
}

fn is_semantic_version(value: &str) -> bool {
    let mut parts = value.split('.');
    let valid = (0..3).all(|_| {
        parts.next().is_some_and(|part| {
            !part.is_empty()
                && (part == "0" || !part.starts_with('0'))
                && part.bytes().all(|byte| byte.is_ascii_digit())
        })
    });
    valid && parts.next().is_none()
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
    #[error("provisional composition schema version must be greater than zero")]
    ZeroCompositionSchemaVersion,
    #[error("provisional composition identifier must be a lowercase ASCII slug")]
    InvalidCompositionId,
    #[error("provisional composition version must be a canonical major.minor.patch version")]
    InvalidCompositionVersion,
    #[error("provisional composition content hash must not be zero")]
    ZeroCompositionHash,
    #[error("spatial grid EPSG code must be greater than zero")]
    ZeroEpsgCode,
    #[error("spatial grid cell size, width, and height must be greater than zero")]
    ZeroGridDimension,
    #[error("spatial grid has {cells} cells; schema v1 permits at most {maximum}")]
    GridTooLarge { cells: u64, maximum: u64 },
    #[error("full-Earth physics/catalog/height frames must be EPSG:4978/4979/3855")]
    UnsupportedEarthReferenceFrames,
    #[error("S2 definition URL must use HTTPS")]
    NonHttpsS2DefinitionUrl,
    #[error("S2 library revision must be at least 12 lowercase hexadecimal characters")]
    InvalidS2LibraryRevision,
    #[error("S2 definition hash must not be zero")]
    ZeroS2DefinitionHash,
    #[error("Earth resolution levels must be strictly increasing S2 levels from 0 through 30")]
    InvalidEarthResolutionLevels,
    #[error("refinement policy version must be greater than zero")]
    ZeroRefinementPolicyVersion,
    #[error("scheduler schema version must be greater than zero")]
    ZeroSchedulerSchemaVersion,
    #[error("partition S2 level must equal the planetary aggregate level")]
    PartitionLevelMismatch,
    #[error("world configuration schema {schema} does not match its geometry/execution shape")]
    ConfigurationShapeMismatch { schema: u16 },
    #[error("world-data bundle schema must match its world-configuration schema")]
    BundleSchemaMismatch,
    #[error("tick duration must be a positive whole-second divisor of one solar day")]
    InvalidTickDuration,
    #[error("maximum events per transition must be greater than zero")]
    ZeroEventBudget,
    #[error("invalid provisional local-environment baseline: {0}")]
    InvalidLocalEnvironment(String),
    #[error("invalid provisional local-weather baseline: {0}")]
    InvalidLocalWeather(String),
    #[error("local-environment active patch must use the configured embodied-patch level")]
    LocalEnvironmentPatchLevelMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::S2CellId;

    fn bundle(schema: u16) -> WorldDataBundleReference {
        WorldDataBundleReference::new(
            schema,
            "earth-test",
            "0.1.0",
            Digest::from_bytes([7; 32]),
            "https://data.atinycivilization.com/earth-test/0.1.0.json",
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

    fn full_earth_grid() -> FullEarthGrid {
        FullEarthGrid {
            physics_crs_epsg: WGS_84_ECEF_EPSG,
            catalog_crs_epsg: WGS_84_3D_EPSG,
            vertical_crs_epsg: EGM_2008_HEIGHT_EPSG,
            s2_definition_url: "https://s2geometry.io/devguide/s2cell_hierarchy".to_owned(),
            s2_library_revision: "0123456789abcdef".to_owned(),
            s2_definition_hash: Digest::sha256(b"pinned S2 definition fixture"),
            s2_projection: S2Projection::Quadratic,
            levels: EarthResolutionLevels {
                planetary_aggregate: 10,
                regional_ecology: 14,
                active_landscape: 18,
                embodied_patch: 23,
            },
            refinement_policy_version: 1,
        }
    }

    fn execution() -> PartitionedExecution {
        PartitionedExecution {
            scheduler_schema_version: 1,
            scheduler: SchedulerKind::DeterministicEventQueue,
            partition_s2_level: 10,
            person_representation: PersonRepresentation::DurableIndividuals,
            capacity_exhaustion: CapacityExhaustionPolicy::PauseAtCommittedBoundary,
            max_events_per_partition_transition: 10_000,
        }
    }

    fn provisional_composition() -> ProvisionalWorldCompositionReference {
        ProvisionalWorldCompositionReference::new(
            1,
            "full-earth-breadth-first",
            "0.1.0",
            Digest::from_bytes([9; 32]),
        )
        .expect("valid provisional composition reference")
    }

    #[test]
    fn legacy_configuration_keeps_its_published_json_shape() {
        let configuration =
            WorldConfiguration::new(300, grid(), bundle(1), 10_000).expect("valid legacy config");
        assert_eq!(
            configuration.spatial_grid().map(SpatialGrid::cell_count),
            Some(10_000)
        );

        let encoded = serde_json::to_string(&configuration).expect("serializable legacy config");
        assert!(encoded.contains("\"configuration_schema_version\":1"));
        assert!(encoded.contains("\"spatial_grid\""));
        assert!(encoded.contains("\"max_events_per_transition\":10000"));
        assert!(!encoded.contains("full_earth_grid"));
        assert_eq!(
            encoded,
            concat!(
                "{\"configuration_schema_version\":1,\"tick_duration_seconds\":300,",
                "\"spatial_grid\":{\"epsg\":32736,\"origin_easting_mm\":500000000,",
                "\"origin_northing_mm\":9700000000,\"cell_size_mm\":10000,",
                "\"width_cells\":100,\"height_cells\":100},\"world_data\":{",
                "\"bundle_schema_version\":1,\"bundle_id\":\"earth-test\",",
                "\"bundle_version\":\"0.1.0\",\"content_hash\":",
                "\"0707070707070707070707070707070707070707070707070707070707070707\",",
                "\"download_url\":\"https://data.atinycivilization.com/earth-test/0.1.0.json\",",
                "\"license_expression\":\"CC-BY-4.0\"},",
                "\"max_events_per_transition\":10000}"
            )
        );
        assert_eq!(
            serde_json::from_str::<WorldConfiguration>(&encoded).expect("decodable legacy config"),
            configuration
        );

        let mut with_unknown =
            serde_json::from_str::<serde_json::Value>(&encoded).expect("JSON object");
        with_unknown["observer_requested_detail"] = serde_json::Value::Bool(true);
        assert!(
            serde_json::from_value::<WorldConfiguration>(with_unknown).is_err(),
            "unknown fields must not be silently dropped from a hashed configuration"
        );
    }

    #[test]
    fn full_earth_configuration_pins_identity_scale_and_safe_capacity_behavior() {
        let configuration =
            WorldConfiguration::new_full_earth(300, full_earth_grid(), bundle(2), execution())
                .expect("valid full-Earth config");
        assert_eq!(configuration.transition_event_limit(), 10_000);
        assert_eq!(
            configuration
                .full_earth_grid()
                .map(|grid| grid.levels.embodied_patch),
            Some(23)
        );

        let encoded = serde_json::to_string(&configuration).expect("serializable Earth config");
        assert!(encoded.contains("\"configuration_schema_version\":2"));
        assert!(encoded.contains("\"full_earth_grid\""));
        assert!(encoded.contains("\"durable_individuals\""));
        assert!(encoded.contains("\"pause_at_committed_boundary\""));
        assert!(!encoded.contains("spatial_grid"));
        assert_eq!(
            encoded,
            format!(
                concat!(
                    "{{\"configuration_schema_version\":2,\"tick_duration_seconds\":300,",
                    "\"full_earth_grid\":{{\"physics_crs_epsg\":4978,",
                    "\"catalog_crs_epsg\":4979,\"vertical_crs_epsg\":3855,",
                    "\"s2_definition_url\":",
                    "\"https://s2geometry.io/devguide/s2cell_hierarchy\",",
                    "\"s2_library_revision\":\"0123456789abcdef\",",
                    "\"s2_definition_hash\":\"{}\",",
                    "\"s2_projection\":\"quadratic\",\"levels\":{{",
                    "\"planetary_aggregate\":10,\"regional_ecology\":14,",
                    "\"active_landscape\":18,\"embodied_patch\":23}},",
                    "\"refinement_policy_version\":1}},\"world_data\":{{",
                    "\"bundle_schema_version\":2,\"bundle_id\":\"earth-test\",",
                    "\"bundle_version\":\"0.1.0\",\"content_hash\":",
                    "\"0707070707070707070707070707070707070707070707070707070707070707\",",
                    "\"download_url\":",
                    "\"https://data.atinycivilization.com/earth-test/0.1.0.json\",",
                    "\"license_expression\":\"CC-BY-4.0\"}},",
                    "\"partitioned_execution\":{{\"scheduler_schema_version\":1,",
                    "\"scheduler\":\"deterministic_event_queue\",",
                    "\"partition_s2_level\":10,",
                    "\"person_representation\":\"durable_individuals\",",
                    "\"capacity_exhaustion\":\"pause_at_committed_boundary\",",
                    "\"max_events_per_partition_transition\":10000}}}}"
                ),
                full_earth_grid().s2_definition_hash
            )
        );
        assert_eq!(
            serde_json::from_str::<WorldConfiguration>(&encoded).expect("decodable Earth config"),
            configuration
        );
    }

    #[test]
    fn provisional_full_earth_has_a_distinct_canonical_schema_three_shape() {
        let configuration = WorldConfiguration::new_provisional_full_earth(
            300,
            full_earth_grid(),
            provisional_composition(),
            execution(),
        )
        .expect("valid provisional full-Earth configuration");
        assert!(configuration.is_provisional_execution());
        assert!(configuration.world_data().is_none());
        assert_eq!(
            configuration
                .provisional_world_composition()
                .map(|reference| reference.composition_id.as_str()),
            Some("full-earth-breadth-first")
        );

        let encoded = serde_json::to_string(&configuration)
            .expect("serializable provisional full-Earth configuration");
        assert!(encoded.contains("\"configuration_schema_version\":3"));
        assert!(encoded.contains("\"provisional_world_composition\":"));
        assert!(encoded.contains("\"content_hash\":\"0909090909090909"));
        assert!(!encoded.contains("\"world_data\":"));
        assert_eq!(
            serde_json::from_str::<WorldConfiguration>(&encoded)
                .expect("decodable provisional configuration"),
            configuration
        );

        let as_value = serde_json::to_value(&configuration).expect("JSON value");
        let provisional_input = serde_json::from_value::<WorldInputReference>(serde_json::json!({
            "provisional_world_composition": as_value["provisional_world_composition"]
        }));
        assert!(matches!(
            provisional_input,
            Ok(WorldInputReference::ProvisionalExecution { .. })
        ));
    }

    #[test]
    fn environmental_provisional_configuration_pins_a_local_baseline() {
        let evidence_patch: S2CellId = "1000010000000000".parse().expect("L10 patch");
        let mut active_patch = evidence_patch;
        for _ in 10..23 {
            active_patch = active_patch.children().expect("child patch")[0];
        }
        let baseline = ProvisionalLocalEnvironmentBaseline {
            status: "provisional-evidence-only".to_owned(),
            source_evidence_digest: Digest::sha256(b"origin environment"),
            evidence_patch,
            active_patch,
            air_temperature_unit: "degC".to_owned(),
            air_temperature_decimal_places: 1,
            air_temperature_normal_minimum: [1; 12],
            air_temperature_normal_mean: [2; 12],
            air_temperature_normal_maximum: [3; 12],
        };
        let configuration =
            WorldConfiguration::new_provisional_full_earth_with_environment_baseline(
                300,
                full_earth_grid(),
                provisional_composition(),
                execution(),
                baseline,
            )
            .expect("environmental provisional configuration");
        assert_eq!(
            configuration.configuration_schema_version,
            PROVISIONAL_ENVIRONMENT_WORLD_CONFIGURATION_SCHEMA_VERSION
        );
        assert_eq!(
            configuration
                .local_environment_baseline()
                .expect("environment baseline")
                .mean_at_normal_phase(0),
            Ok(2)
        );
        let encoded = serde_json::to_string(&configuration).expect("serialize");
        assert!(encoded.contains("local_environment_baseline"));
        assert_eq!(
            serde_json::from_str::<WorldConfiguration>(&encoded).expect("decode"),
            configuration
        );
    }

    #[test]
    fn weather_configuration_has_a_distinct_round_tripping_schema_five_shape() {
        let evidence_patch: S2CellId = "1000010000000000".parse().expect("L10 patch");
        let mut active_patch = evidence_patch;
        for _ in 10..23 {
            active_patch = active_patch.children().expect("child patch")[0];
        }
        let environment = ProvisionalLocalEnvironmentBaseline {
            status: "provisional-evidence-only".to_owned(),
            source_evidence_digest: Digest::sha256(b"origin environment"),
            evidence_patch,
            active_patch,
            air_temperature_unit: "degC".to_owned(),
            air_temperature_decimal_places: 1,
            air_temperature_normal_minimum: [1; 12],
            air_temperature_normal_mean: [2; 12],
            air_temperature_normal_maximum: [3; 12],
        };
        let weather = ProvisionalLocalWeatherBaseline {
            status: "provisional-weather-input-not-scientifically-admitted".to_owned(),
            source_normals_digest: Digest::sha256(b"origin climate normals"),
            evidence_patch,
            active_patch,
            air_temperature_unit: "degC".to_owned(),
            air_temperature_decimal_places: 3,
            air_temperature_normal_minimum: [10_000; 12],
            air_temperature_normal_mean: [15_000; 12],
            air_temperature_normal_maximum: [20_000; 12],
            precipitation_unit: "m".to_owned(),
            precipitation_decimal_places: 6,
            precipitation_normal_mean: [1_000; 12],
            eastward_wind_unit: "m/s".to_owned(),
            eastward_wind_decimal_places: 3,
            eastward_wind_normal_mean: [500; 12],
            northward_wind_unit: "m/s".to_owned(),
            northward_wind_decimal_places: 3,
            northward_wind_normal_mean: [-500; 12],
        };
        let configuration = WorldConfiguration::new_provisional_full_earth_with_weather_baseline(
            300,
            full_earth_grid(),
            provisional_composition(),
            execution(),
            environment,
            weather,
        )
        .expect("weather configuration");
        assert_eq!(
            configuration.configuration_schema_version,
            PROVISIONAL_WEATHER_WORLD_CONFIGURATION_SCHEMA_VERSION
        );
        let encoded = serde_json::to_string(&configuration).expect("serialize weather config");
        assert!(encoded.contains("local_weather_baseline"));
        assert_eq!(
            serde_json::from_str::<WorldConfiguration>(&encoded).expect("decode weather config"),
            configuration
        );
    }

    #[test]
    fn admitted_and_provisional_inputs_cannot_impersonate_each_other() {
        let admitted = WorldConfiguration::new_full_earth(
            300,
            full_earth_grid(),
            bundle(WORLD_CONFIGURATION_SCHEMA_VERSION),
            execution(),
        )
        .expect("valid admitted full-Earth configuration");
        let mut admitted_as_provisional =
            serde_json::to_value(admitted).expect("admitted JSON value");
        admitted_as_provisional["configuration_schema_version"] =
            serde_json::json!(PROVISIONAL_WORLD_CONFIGURATION_SCHEMA_VERSION);
        assert!(serde_json::from_value::<WorldConfiguration>(admitted_as_provisional).is_err());

        let provisional = WorldConfiguration::new_provisional_full_earth(
            300,
            full_earth_grid(),
            provisional_composition(),
            execution(),
        )
        .expect("valid provisional full-Earth configuration");
        let mut provisional_as_admitted =
            serde_json::to_value(provisional).expect("provisional JSON value");
        provisional_as_admitted["configuration_schema_version"] =
            serde_json::json!(WORLD_CONFIGURATION_SCHEMA_VERSION);
        assert!(serde_json::from_value::<WorldConfiguration>(provisional_as_admitted).is_err());
    }

    #[test]
    fn provisional_reference_rejects_ambiguous_or_unpinned_identity() {
        assert_eq!(
            ProvisionalWorldCompositionReference::new(
                0,
                "full-earth",
                "0.1.0",
                Digest::from_bytes([1; 32]),
            ),
            Err(WorldConfigurationError::ZeroCompositionSchemaVersion)
        );
        assert_eq!(
            ProvisionalWorldCompositionReference::new(
                1,
                "full-earth",
                "01.1.0",
                Digest::from_bytes([1; 32]),
            ),
            Err(WorldConfigurationError::InvalidCompositionVersion)
        );
        assert_eq!(
            ProvisionalWorldCompositionReference::new(1, "full-earth", "0.1.0", Digest::ZERO,),
            Err(WorldConfigurationError::ZeroCompositionHash)
        );
    }

    #[test]
    fn configuration_shape_and_resolution_errors_are_rejected() {
        let mut bad_levels = full_earth_grid();
        bad_levels.levels.regional_ecology = bad_levels.levels.planetary_aggregate;
        assert_eq!(
            WorldConfiguration::new_full_earth(300, bad_levels, bundle(2), execution()),
            Err(WorldConfigurationError::InvalidEarthResolutionLevels)
        );

        let mut bad_partition = execution();
        bad_partition.partition_s2_level = 11;
        assert_eq!(
            WorldConfiguration::new_full_earth(300, full_earth_grid(), bundle(2), bad_partition,),
            Err(WorldConfigurationError::PartitionLevelMismatch)
        );

        let mut mismatched =
            WorldConfiguration::new(300, grid(), bundle(1), 10_000).expect("valid legacy config");
        mismatched.configuration_schema_version = WORLD_CONFIGURATION_SCHEMA_VERSION;
        assert_eq!(
            mismatched.validate(),
            Err(WorldConfigurationError::ConfigurationShapeMismatch {
                schema: WORLD_CONFIGURATION_SCHEMA_VERSION,
            })
        );

        let mut wrong_bundle = WorldConfiguration::new_full_earth(
            300,
            full_earth_grid(),
            bundle(WORLD_CONFIGURATION_SCHEMA_VERSION),
            execution(),
        )
        .expect("valid full-Earth config");
        if let WorldInputReference::ScientificallyAdmitted { world_data } = &mut wrong_bundle.input
        {
            world_data.bundle_schema_version = LEGACY_WORLD_CONFIGURATION_SCHEMA_VERSION;
        }
        assert_eq!(
            wrong_bundle.validate(),
            Err(WorldConfigurationError::BundleSchemaMismatch)
        );
    }
}
