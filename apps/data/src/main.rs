use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use netcdf_reader::{NcAttrValue, NcFile, NcSliceInfo, NcSliceInfoElem, NcType};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use world_data::{
    BooleanFieldCell, COPERNICUS_LCCS_CLASSES, PACKED_BOOLEAN_FIELD_TILE_MEDIA_TYPE,
    PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE, PackedBooleanFieldTile, PackedScalarTerrainTile,
    ScalarTerrainCell, SourceSnapshotArtifact, SourceSnapshotManifest, TileArtifactReference,
    TileTreeEntry, TileTreeEntryKind, TileTreeIndex, WorldDataBundle,
};
use world_data_filesystem::{
    verify_release_artifacts, verify_source_snapshot_artifact, verify_source_snapshot_artifacts,
};
use world_domain::{
    Digest, GeographicCoordinateE7, GeographicCoordinateHalfArcsecond, MAX_S2_LEVEL, S2CellId,
    S2FaceUv, WorldConfiguration, decode_s2_face_ij, route_geographic_to_s2,
    route_half_arcsecond_to_s2, s2_face_ij_center_uv, s2_face_ij_vertex_uv, s2_face_uv_to_ray,
    s2_ray_to_geographic_e7,
};

#[derive(Debug, Parser)]
#[command(name = "civilization-data")]
#[command(about = "Validate canonical scientific world-data bundles")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate release completeness, canonical bytes, and optionally a world config.
    Validate {
        bundle: PathBuf,
        #[arg(long)]
        configuration: Option<PathBuf>,
    },
    /// Acquire or verify exact pre-normalization scientific source bytes.
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    /// Inspect exact source bytes without treating them as normalized world data.
    Inspect {
        #[command(subcommand)]
        command: InspectCommand,
    },
    /// Derive deterministic intermediate artifacts from verified scientific sources.
    Derive {
        #[command(subcommand)]
        command: DeriveCommand,
    },
}

#[derive(Debug, Subcommand)]
enum SourceCommand {
    /// Verify every retained artifact without contacting the network.
    Validate {
        manifest: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Fetch missing artifacts over HTTPS, refusing to replace any existing file.
    Fetch {
        manifest: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum InspectCommand {
    /// Parse the pinned Natural Earth polygon stream into an auditable summary.
    NaturalEarthLand {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Evaluate one exact coordinate against the pinned Natural Earth land polygons.
    NaturalEarthLandPoint {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long)]
        latitude_e7: i32,
        #[arg(long)]
        longitude_e7: i32,
    },
    /// Inspect the pinned ETOPO NetCDF schema through the portable Rust reader.
    Etopo {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Inspect the pinned CHELSA January-temperature NetCDF schema through the portable Rust reader.
    ChelsaJanuaryTemperature {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Inspect and cross-check all twelve pinned CHELSA monthly temperature normals.
    ChelsaAnnualTemperature {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Read one exact raw sample from the pinned CHELSA January-temperature grid.
    ChelsaJanuaryCell {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Zero-based source row in the retained latitude axis order.
        #[arg(long)]
        row: u64,
        /// Zero-based source column in the retained longitude axis order.
        #[arg(long)]
        column: u64,
    },
    /// Resolve an exact WGS 84 coordinate to its nearest retained CHELSA source cell.
    ChelsaNearestCell {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Latitude in exact 10^-7 degrees.
        #[arg(long)]
        latitude_e7: i32,
        /// Longitude in exact 10^-7 degrees.
        #[arg(long)]
        longitude_e7: i32,
    },
    /// Read all twelve retained CHELSA normal-month samples at one exact coordinate.
    ChelsaAnnualCell {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Latitude in exact 10^-7 degrees.
        #[arg(long)]
        latitude_e7: i32,
        /// Longitude in exact 10^-7 degrees.
        #[arg(long)]
        longitude_e7: i32,
    },
    /// Inspect the two NetCDF members inside one verified annual ERA5 ZIP response.
    Era5AnnualArchive {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long, default_value_t = 1981)]
        year: u16,
    },
    /// Inspect the verified 2022 Copernicus land-cover ZIP and NetCDF schema.
    CopernicusLandCover {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
    },
    /// Census every Copernicus land-cover class and quality value globally.
    CopernicusLandCoverCensus {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Optionally publish deterministic pretty JSON to a new path.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Inspect fixed target-support samples for one S2 land-cover cell.
    CopernicusLandCoverTargetSupport {
        #[arg(long)]
        s2_cell_id: S2CellId,
        #[arg(long, default_value_t = 32)]
        points_per_axis: u8,
    },
    /// Route one exact WGS 84 geographic coordinate through the shared S2 contract.
    GeographicRoute {
        /// Latitude in exact 10^-7 degrees, within [-900000000, 900000000].
        #[arg(long)]
        latitude_e7: i32,
        /// Longitude in exact 10^-7 degrees, within [-1800000000, 1800000000).
        #[arg(long)]
        longitude_e7: i32,
        #[arg(long, default_value_t = 10)]
        s2_level: u8,
    },
    /// Route an exact ETOPO 2022 60-arc-second area-cell centre through shared S2 routing.
    EtopoCellRoute {
        /// Zero-based source row, south to north.
        #[arg(long)]
        row: u32,
        /// Zero-based source column, west to east.
        #[arg(long)]
        column: u32,
        #[arg(long, default_value_t = 10)]
        s2_level: u8,
    },
    /// Inspect a deterministic interior quadrature of one ETOPO source area cell.
    EtopoCellQuadrature {
        /// Zero-based source row, south to north.
        #[arg(long)]
        row: u32,
        /// Zero-based source column, west to east.
        #[arg(long)]
        column: u32,
        #[arg(long, default_value_t = 10)]
        s2_level: u8,
        /// Equal-spaced interior points per source-cell axis. Must divide 60.
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Measure the exact interior-quadrature routing workload without writing data.
    EtopoQuadratureThroughput {
        /// Zero-based first source row, south to north.
        #[arg(long, default_value_t = 0)]
        start_row: u32,
        /// Consecutive complete source rows to route. This never creates output.
        #[arg(long, default_value_t = 1)]
        source_rows: u32,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Verify every artifact in a standalone packed ETOPO terrain layer release.
    EtopoTerrainLayer {
        /// Directory emitted by `derive etopo-terrain-layer`.
        #[arg(long)]
        input_directory: PathBuf,
        #[arg(long, default_value = "bedrock-relief")]
        layer_id: String,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Verify every artifact in a standalone packed Natural Earth land-reference release.
    NaturalEarthLandReferenceLayer {
        /// Directory emitted by `derive natural-earth-land-reference-layer`.
        #[arg(long)]
        input_directory: PathBuf,
        #[arg(long, default_value = "land-reference")]
        layer_id: String,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
    },
}

#[derive(Debug, Subcommand)]
enum DeriveCommand {
    /// Preserve a regular, evenly sampled ETOPO elevation grid as portable canonical bytes.
    ///
    /// This is a provenance-bound intermediate artifact, not yet a canonical world-data
    /// layer or a claim that full-Earth genesis is available.
    EtopoGrid {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Sampling spacing in ETOPO's native one-arc-minute cells. Must divide 60.
        #[arg(long, default_value_t = 5)]
        sample_arc_minutes: u16,
        /// New output path. It must not exist; existing results are never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Pair sampled ETOPO source values with their exact centre-routed S2 addresses.
    ///
    /// This is a provenance-bound intermediate index, not a canonical elevation layer.
    EtopoCentreIndex {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Sampling spacing in ETOPO's native one-arc-minute cells. Must divide 60.
        #[arg(long, default_value_t = 5)]
        sample_arc_minutes: u16,
        #[arg(long, default_value_t = 10)]
        s2_level: u8,
        /// New output path. It must not exist; existing results are never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Summarize a verified ETOPO centre-attribution index at a coarser S2 level.
    ///
    /// This is a source-centre quadrature summary, not a target-cell coverage or
    /// area-overlap terrain layer.
    CentreSummary {
        /// Existing ETOPO centre index to read and validate.
        #[arg(long)]
        input: PathBuf,
        /// S2 level to summarize into. It must not be finer than the input level.
        #[arg(long)]
        s2_level: u8,
        /// New output path. It must not exist; existing results are never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Derive the exact weighted S2 contribution of one verified ETOPO source cell.
    ///
    /// This is a normalizer-kernel probe, not a complete global terrain layer.
    EtopoQuadratureContribution {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Zero-based source row, south to north.
        #[arg(long)]
        row: u32,
        /// Zero-based source column, west to east.
        #[arg(long)]
        column: u32,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        /// Equal-spaced interior points per source-cell axis. Must divide 60.
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Normalize the complete pinned ETOPO source into packed L6→L10 terrain tiles.
    ///
    /// This is deliberately a long offline batch. It emits only an ETOPO terrain-layer
    /// root, not a complete world-data bundle or canonical world.
    EtopoTerrainLayer {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        /// Layer identifier, normally bedrock-relief.
        #[arg(long, default_value = "bedrock-relief")]
        layer_id: String,
        /// New empty release directory. Existing paths are never reused.
        #[arg(long)]
        output_directory: PathBuf,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
        /// Equal-spaced interior points per source-cell axis. Must divide 60.
        #[arg(long, default_value_t = 4)]
        points_per_axis: u8,
    },
    /// Classify every L10 centre against pinned Natural Earth land polygons.
    ///
    /// The source is a generalized land reference, so this creates a clearly named
    /// reference layer, never a claim of a measurement-resolution coastline.
    NaturalEarthLandReferenceLayer {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
        #[arg(long, default_value = "land-reference")]
        layer_id: String,
        #[arg(long)]
        output_directory: PathBuf,
        #[arg(long, default_value_t = 6)]
        container_s2_level: u8,
        #[arg(long, default_value_t = 10)]
        target_s2_level: u8,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate {
            bundle,
            configuration,
        } => validate(bundle, configuration.as_ref()),
        Command::Source { command } => match command {
            SourceCommand::Validate {
                manifest,
                artifact_root,
            } => validate_source(&manifest, &artifact_root),
            SourceCommand::Fetch {
                manifest,
                artifact_root,
            } => fetch_source(&manifest, &artifact_root).await,
        },
        Command::Inspect { command } => match command {
            InspectCommand::NaturalEarthLand {
                source_snapshot,
                artifact_root,
            } => inspect_natural_earth_land(&source_snapshot, &artifact_root),
            InspectCommand::NaturalEarthLandPoint {
                source_snapshot,
                artifact_root,
                latitude_e7,
                longitude_e7,
            } => inspect_natural_earth_land_point(
                &source_snapshot,
                &artifact_root,
                latitude_e7,
                longitude_e7,
            ),
            InspectCommand::Etopo {
                source_snapshot,
                artifact_root,
            } => inspect_etopo(&source_snapshot, &artifact_root),
            InspectCommand::ChelsaJanuaryTemperature {
                source_snapshot,
                artifact_root,
            } => inspect_chelsa_january_temperature(&source_snapshot, &artifact_root),
            InspectCommand::ChelsaAnnualTemperature {
                source_snapshot,
                artifact_root,
            } => inspect_chelsa_annual_temperature(&source_snapshot, &artifact_root),
            InspectCommand::ChelsaJanuaryCell {
                source_snapshot,
                artifact_root,
                row,
                column,
            } => inspect_chelsa_january_cell(&source_snapshot, &artifact_root, row, column),
            InspectCommand::ChelsaNearestCell {
                source_snapshot,
                artifact_root,
                latitude_e7,
                longitude_e7,
            } => inspect_chelsa_nearest_cell(
                &source_snapshot,
                &artifact_root,
                latitude_e7,
                longitude_e7,
            ),
            InspectCommand::ChelsaAnnualCell {
                source_snapshot,
                artifact_root,
                latitude_e7,
                longitude_e7,
            } => inspect_chelsa_annual_cell(
                &source_snapshot,
                &artifact_root,
                latitude_e7,
                longitude_e7,
            ),
            InspectCommand::Era5AnnualArchive {
                source_snapshot,
                artifact_root,
                year,
            } => inspect_era5_annual_archive(&source_snapshot, &artifact_root, year),
            InspectCommand::CopernicusLandCover {
                source_snapshot,
                artifact_root,
            } => inspect_copernicus_land_cover(&source_snapshot, &artifact_root),
            InspectCommand::CopernicusLandCoverCensus {
                source_snapshot,
                artifact_root,
                output,
            } => inspect_copernicus_land_cover_census(
                &source_snapshot,
                &artifact_root,
                output.as_deref(),
            ),
            InspectCommand::CopernicusLandCoverTargetSupport {
                s2_cell_id,
                points_per_axis,
            } => inspect_copernicus_land_cover_target_support(s2_cell_id, points_per_axis),
            InspectCommand::GeographicRoute {
                latitude_e7,
                longitude_e7,
                s2_level,
            } => inspect_geographic_route(latitude_e7, longitude_e7, s2_level),
            InspectCommand::EtopoCellRoute {
                row,
                column,
                s2_level,
            } => inspect_etopo_cell_route(row, column, s2_level),
            InspectCommand::EtopoCellQuadrature {
                row,
                column,
                s2_level,
                points_per_axis,
            } => inspect_etopo_cell_quadrature(row, column, s2_level, points_per_axis),
            InspectCommand::EtopoQuadratureThroughput {
                start_row,
                source_rows,
                target_s2_level,
                points_per_axis,
            } => inspect_etopo_quadrature_throughput(
                start_row,
                source_rows,
                target_s2_level,
                points_per_axis,
            ),
            InspectCommand::EtopoTerrainLayer {
                input_directory,
                layer_id,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            } => inspect_etopo_terrain_layer(
                &input_directory,
                &layer_id,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            ),
            InspectCommand::NaturalEarthLandReferenceLayer {
                input_directory,
                layer_id,
                container_s2_level,
                target_s2_level,
            } => inspect_natural_earth_land_reference_layer(
                &input_directory,
                &layer_id,
                container_s2_level,
                target_s2_level,
            ),
        },
        Command::Derive { command } => match command {
            DeriveCommand::EtopoGrid {
                source_snapshot,
                artifact_root,
                sample_arc_minutes,
                output,
            } => derive_etopo_grid(
                &source_snapshot,
                &artifact_root,
                sample_arc_minutes,
                &output,
            ),
            DeriveCommand::EtopoCentreIndex {
                source_snapshot,
                artifact_root,
                sample_arc_minutes,
                s2_level,
                output,
            } => derive_etopo_centre_index(
                &source_snapshot,
                &artifact_root,
                sample_arc_minutes,
                s2_level,
                &output,
            ),
            DeriveCommand::CentreSummary {
                input,
                s2_level,
                output,
            } => derive_etopo_centre_summary(&input, s2_level, &output),
            DeriveCommand::EtopoQuadratureContribution {
                source_snapshot,
                artifact_root,
                row,
                column,
                target_s2_level,
                points_per_axis,
            } => derive_etopo_quadrature_contribution(
                &source_snapshot,
                &artifact_root,
                row,
                column,
                target_s2_level,
                points_per_axis,
            ),
            DeriveCommand::EtopoTerrainLayer {
                source_snapshot,
                artifact_root,
                layer_id,
                output_directory,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            } => derive_etopo_terrain_layer(
                &source_snapshot,
                &artifact_root,
                &layer_id,
                &output_directory,
                container_s2_level,
                target_s2_level,
                points_per_axis,
            ),
            DeriveCommand::NaturalEarthLandReferenceLayer {
                source_snapshot,
                artifact_root,
                layer_id,
                output_directory,
                container_s2_level,
                target_s2_level,
            } => derive_natural_earth_land_reference_layer(
                &source_snapshot,
                &artifact_root,
                &layer_id,
                &output_directory,
                container_s2_level,
                target_s2_level,
            ),
        },
    }
}

#[derive(Serialize)]
struct GeographicRouteInspection {
    inspection_schema_version: u16,
    coordinate_frame: &'static str,
    latitude_e7: i32,
    longitude_e7: i32,
    s2_level: u8,
    s2_cell_id: String,
}

fn inspect_geographic_route(latitude_e7: i32, longitude_e7: i32, s2_level: u8) -> Result<()> {
    let coordinate = GeographicCoordinateE7::new(latitude_e7, longitude_e7)
        .context("validate WGS 84 geographic coordinate")?;
    let cell = route_geographic_to_s2(coordinate, s2_level)
        .context("route geographic coordinate to S2")?;
    println!(
        "{}",
        serde_json::to_string(&GeographicRouteInspection {
            inspection_schema_version: 1,
            coordinate_frame: "WGS 84 geodetic coordinates routed through a WGS 84 ECEF ray",
            latitude_e7,
            longitude_e7,
            s2_level,
            s2_cell_id: cell.to_string(),
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct EtopoCellRouteInspection {
    inspection_schema_version: u16,
    source_grid: &'static str,
    sample_support: &'static str,
    boundary_convention: &'static str,
    row: u32,
    column: u32,
    latitude_half_arcseconds: i32,
    longitude_half_arcseconds: i32,
    south_boundary_half_arcseconds: i32,
    north_boundary_half_arcseconds: i32,
    west_boundary_half_arcseconds: i32,
    east_boundary_half_arcseconds: i32,
    s2_level: u8,
    s2_cell_id: String,
}

fn inspect_etopo_cell_route(row: u32, column: u32, s2_level: u8) -> Result<()> {
    let support = etopo_cell_support(row, column)?;
    let cell = route_half_arcsecond_to_s2(support.centre, s2_level)
        .context("route ETOPO cell centre to S2")?;
    println!(
        "{}",
        serde_json::to_string(&EtopoCellRouteInspection {
            inspection_schema_version: 1,
            source_grid: "NOAA ETOPO 2022 v1 60-arc-second WGS 84 / EGM2008",
            sample_support: "60-arc-second area cell center",
            boundary_convention: "south/west inclusive, north/east exclusive; +180 wraps to -180",
            row,
            column,
            latitude_half_arcseconds: support.centre.latitude_half_arcseconds(),
            longitude_half_arcseconds: support.centre.longitude_half_arcseconds(),
            south_boundary_half_arcseconds: support.south_boundary_half_arcseconds,
            north_boundary_half_arcseconds: support.north_boundary_half_arcseconds,
            west_boundary_half_arcseconds: support.west_boundary_half_arcseconds,
            east_boundary_half_arcseconds: support.east_boundary_half_arcseconds,
            s2_level,
            s2_cell_id: cell.to_string(),
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct EtopoCellQuadratureInspection {
    inspection_schema_version: u16,
    source_grid: &'static str,
    support_policy: &'static str,
    row: u32,
    column: u32,
    s2_level: u8,
    points_per_axis: u8,
    source_sample_count: u32,
    target_cells: Vec<EtopoQuadratureTargetCell>,
}

#[derive(Serialize)]
struct EtopoQuadratureTargetCell {
    s2_cell_id: String,
    equal_weight_samples: u32,
}

#[derive(Serialize)]
struct EtopoQuadratureThroughputInspection {
    inspection_schema_version: u16,
    start_row: u32,
    source_rows: u32,
    target_s2_level: u8,
    points_per_axis: u8,
    source_cells_routed: u64,
    interior_points_routed: u64,
    elapsed_milliseconds: u128,
}

/// Returns a deterministic, equal-point quadrature of an ETOPO source area cell.
///
/// This is deliberately an approximation to source-cell overlap, rather than an
/// assertion of exact spherical area intersection. Points are strictly interior, so
/// no source sample depends on an arbitrary north/east boundary ownership rule.
fn etopo_cell_quadrature(
    row: u32,
    column: u32,
    s2_level: u8,
    points_per_axis: u8,
) -> Result<std::collections::BTreeMap<S2CellId, u32>> {
    if points_per_axis == 0 || 60 % points_per_axis != 0 {
        bail!("points_per_axis must be a non-zero divisor of 60");
    }
    if s2_level > MAX_S2_LEVEL {
        bail!("s2_level must be within 0 through {MAX_S2_LEVEL}");
    }
    let support = etopo_cell_support(row, column)?;
    let denominator = i32::from(points_per_axis)
        .checked_mul(2)
        .context("ETOPO quadrature denominator overflow")?;
    let spacing = ETOPO_CELL_STEP_HALF_ARCSECONDS / denominator;
    if spacing == 0 || ETOPO_CELL_STEP_HALF_ARCSECONDS % denominator != 0 {
        bail!("points_per_axis does not produce an exact ETOPO half-arcsecond quadrature");
    }
    let mut cells = std::collections::BTreeMap::new();
    for latitude_index in 0..i32::from(points_per_axis) {
        for longitude_index in 0..i32::from(points_per_axis) {
            let latitude_offset = latitude_index
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .context("ETOPO quadrature latitude index overflow")?;
            let longitude_offset = longitude_index
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .context("ETOPO quadrature longitude index overflow")?;
            let latitude = support
                .south_boundary_half_arcseconds
                .checked_add(
                    spacing
                        .checked_mul(latitude_offset)
                        .context("ETOPO quadrature latitude overflow")?,
                )
                .context("ETOPO quadrature latitude overflow")?;
            let longitude = support
                .west_boundary_half_arcseconds
                .checked_add(
                    spacing
                        .checked_mul(longitude_offset)
                        .context("ETOPO quadrature longitude overflow")?,
                )
                .context("ETOPO quadrature longitude overflow")?;
            let coordinate = GeographicCoordinateHalfArcsecond::new(latitude, longitude)
                .context("construct ETOPO quadrature coordinate")?;
            let target = route_half_arcsecond_to_s2(coordinate, s2_level)
                .context("route ETOPO quadrature point to S2")?;
            *cells.entry(target).or_insert(0) += 1;
        }
    }
    Ok(cells)
}

fn inspect_etopo_cell_quadrature(
    row: u32,
    column: u32,
    s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    let cells = etopo_cell_quadrature(row, column, s2_level, points_per_axis)?;
    println!(
        "{}",
        serde_json::to_string(&EtopoCellQuadratureInspection {
            inspection_schema_version: 1,
            source_grid: "NOAA ETOPO 2022 v1 60-arc-second WGS 84 / EGM2008",
            support_policy: "equal interior lattice points; approximate source-cell overlap, not exact spherical clipping",
            row,
            column,
            s2_level,
            points_per_axis,
            source_sample_count: u32::from(points_per_axis) * u32::from(points_per_axis),
            target_cells: cells
                .into_iter()
                .map(|(cell, equal_weight_samples)| EtopoQuadratureTargetCell {
                    s2_cell_id: cell.to_string(),
                    equal_weight_samples,
                })
                .collect(),
        })?
    );
    Ok(())
}

fn inspect_etopo_quadrature_throughput(
    start_row: u32,
    source_rows: u32,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    if source_rows == 0
        || u64::from(start_row)
            .checked_add(u64::from(source_rows))
            .is_none_or(|end| end > ETOPO_LATITUDE_CELLS)
    {
        bail!("requested ETOPO throughput rows are outside the source grid");
    }
    let started = Instant::now();
    let mut interior_points_routed = 0_u64;
    for row in start_row..start_row + source_rows {
        for column in 0..u32::try_from(ETOPO_LONGITUDE_CELLS)? {
            let samples = etopo_cell_quadrature(row, column, target_s2_level, points_per_axis)?;
            interior_points_routed = interior_points_routed
                .checked_add(samples.values().map(|count| u64::from(*count)).sum::<u64>())
                .context("ETOPO throughput point count overflow")?;
        }
    }
    let source_cells_routed = u64::from(source_rows)
        .checked_mul(ETOPO_LONGITUDE_CELLS)
        .context("ETOPO throughput source-cell count overflow")?;
    println!(
        "{}",
        serde_json::to_string(&EtopoQuadratureThroughputInspection {
            inspection_schema_version: 1,
            start_row,
            source_rows,
            target_s2_level,
            points_per_axis,
            source_cells_routed,
            interior_points_routed,
            elapsed_milliseconds: started.elapsed().as_millis(),
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct EtopoTerrainLayerInspection {
    inspection_schema_version: u16,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    quadrature_points_per_axis: u8,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
    tile_count: u64,
    target_cell_count: u64,
    tile_byte_length: u64,
}

/// Fully validate the flat L6→L10 terrain release before it is used as evidence.
///
/// This is intentionally independent from derivation: it rereads each canonical tile,
/// checks the root's content-addressed references, and requires uniform provenance and
/// packing parameters across the complete global layer.
fn inspect_etopo_terrain_layer(
    input_directory: &Path,
    layer_id: &str,
    container_s2_level: u8,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_bytes = read_release_file(input_directory, &root_relative_path)?;
    let root = TileTreeIndex::from_canonical_slice(&root_bytes)
        .context("decode canonical ETOPO terrain-layer root index")?;
    if root.layer_id != layer_id {
        bail!(
            "ETOPO terrain root declares layer {:?}, expected {:?}",
            root.layer_id,
            layer_id
        );
    }
    let expected_containers = global_s2_cells_at_level(container_s2_level)?;
    if root.entries.len() != expected_containers.len() {
        bail!(
            "ETOPO terrain root has {} tiles, expected {} at S2 level {container_s2_level}",
            root.entries.len(),
            expected_containers.len()
        );
    }

    let mut source_snapshot_digest = None;
    let mut source_artifact_digest = None;
    let mut tile_byte_length = 0_u64;
    let mut target_cell_count = 0_u64;
    for (entry, expected_container) in root.entries.iter().zip(expected_containers) {
        if entry.kind != TileTreeEntryKind::Tile
            || entry.s2_level != container_s2_level
            || entry.s2_cell_id != expected_container.to_string()
        {
            bail!(
                "ETOPO terrain root entry is not the expected L{container_s2_level} tile for {expected_container}"
            );
        }
        if entry.artifact.media_type != PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE {
            bail!(
                "ETOPO terrain tile {:?} has unexpected media type {:?}",
                entry.artifact.path,
                entry.artifact.media_type
            );
        }
        let bytes = read_release_file(input_directory, &entry.artifact.path)?;
        if u64::try_from(bytes.len())? != entry.artifact.byte_length
            || Digest::sha256(&bytes) != entry.artifact.content_hash
        {
            bail!(
                "ETOPO terrain tile {:?} fails its root reference",
                entry.artifact.path
            );
        }
        let tile = PackedScalarTerrainTile::from_canonical_slice(&bytes).with_context(|| {
            format!(
                "decode canonical ETOPO terrain tile {:?}",
                entry.artifact.path
            )
        })?;
        if tile.layer_id != layer_id
            || tile.container_s2_cell_id != expected_container
            || tile.target_s2_level != target_s2_level
            || tile.quadrature_points_per_axis != points_per_axis
        {
            bail!(
                "ETOPO terrain tile {:?} has inconsistent packing metadata",
                entry.artifact.path
            );
        }
        match source_snapshot_digest {
            Some(expected) if expected != tile.source_snapshot_digest => {
                bail!("ETOPO terrain tiles disagree on source snapshot digest")
            }
            None => source_snapshot_digest = Some(tile.source_snapshot_digest),
            _ => {}
        }
        match source_artifact_digest {
            Some(expected) if expected != tile.source_artifact_digest => {
                bail!("ETOPO terrain tiles disagree on source artifact digest")
            }
            None => source_artifact_digest = Some(tile.source_artifact_digest),
            _ => {}
        }
        tile_byte_length = tile_byte_length
            .checked_add(entry.artifact.byte_length)
            .context("ETOPO terrain tile byte total overflow")?;
        target_cell_count = target_cell_count
            .checked_add(u64::try_from(tile.cells.len())?)
            .context("ETOPO terrain target cell total overflow")?;
    }

    println!(
        "{}",
        serde_json::to_string(&EtopoTerrainLayerInspection {
            inspection_schema_version: 1,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            quadrature_points_per_axis: points_per_axis,
            source_snapshot_digest: source_snapshot_digest
                .context("ETOPO terrain root is empty")?,
            source_artifact_digest: source_artifact_digest
                .context("ETOPO terrain root is empty")?,
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
            tile_count: u64::try_from(root.entries.len())?,
            target_cell_count,
            tile_byte_length,
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct NaturalEarthLandReferenceLayerInspection {
    inspection_schema_version: u16,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    sample_policy: String,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
    tile_count: u64,
    target_cell_count: u64,
    tile_byte_length: u64,
}

/// Independently validate a complete generalized-land reference release.
fn inspect_natural_earth_land_reference_layer(
    input_directory: &Path,
    layer_id: &str,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<()> {
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_bytes = read_release_file(input_directory, &root_relative_path)?;
    let root = TileTreeIndex::from_canonical_slice(&root_bytes)
        .context("decode canonical Natural Earth land-reference root index")?;
    if root.layer_id != layer_id {
        bail!("land-reference root declares an unexpected layer identifier");
    }
    let expected_containers = global_s2_cells_at_level(container_s2_level)?;
    if root.entries.len() != expected_containers.len() {
        bail!("land-reference root does not cover every expected container");
    }
    let mut source_snapshot_digest = None;
    let mut source_artifact_digest = None;
    let mut sample_policy = None;
    let mut tile_byte_length = 0_u64;
    let mut target_cell_count = 0_u64;
    for (entry, expected_container) in root.entries.iter().zip(expected_containers) {
        if entry.kind != TileTreeEntryKind::Tile
            || entry.s2_level != container_s2_level
            || entry.s2_cell_id != expected_container.to_string()
            || entry.artifact.media_type != PACKED_BOOLEAN_FIELD_TILE_MEDIA_TYPE
        {
            bail!("land-reference root has an invalid tile entry");
        }
        let bytes = read_release_file(input_directory, &entry.artifact.path)?;
        if u64::try_from(bytes.len())? != entry.artifact.byte_length
            || Digest::sha256(&bytes) != entry.artifact.content_hash
        {
            bail!("land-reference tile fails its root reference");
        }
        let tile = PackedBooleanFieldTile::from_canonical_slice(&bytes)
            .context("decode canonical land-reference tile")?;
        if tile.layer_id != layer_id
            || tile.container_s2_cell_id != expected_container
            || tile.target_s2_level != target_s2_level
        {
            bail!("land-reference tile has inconsistent packing metadata");
        }
        for (name, observed, expected) in [
            (
                "source snapshot",
                tile.source_snapshot_digest,
                source_snapshot_digest,
            ),
            (
                "source artifact",
                tile.source_artifact_digest,
                source_artifact_digest,
            ),
        ] {
            if let Some(expected) = expected
                && observed != expected
            {
                bail!("land-reference tiles disagree on {name} provenance");
            }
        }
        if let Some(expected) = sample_policy.as_ref()
            && &tile.sample_policy != expected
        {
            bail!("land-reference tiles disagree on sample policy");
        }
        source_snapshot_digest.get_or_insert(tile.source_snapshot_digest);
        source_artifact_digest.get_or_insert(tile.source_artifact_digest);
        sample_policy.get_or_insert(tile.sample_policy);
        tile_byte_length = tile_byte_length
            .checked_add(u64::try_from(bytes.len())?)
            .context("land-reference tile byte total overflow")?;
        target_cell_count = target_cell_count
            .checked_add(u64::try_from(tile.cells.len())?)
            .context("land-reference target cell total overflow")?;
    }
    println!(
        "{}",
        serde_json::to_string(&NaturalEarthLandReferenceLayerInspection {
            inspection_schema_version: 1,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            sample_policy: sample_policy.context("land-reference root is empty")?,
            source_snapshot_digest: source_snapshot_digest
                .context("land-reference root is empty")?,
            source_artifact_digest: source_artifact_digest
                .context("land-reference root is empty")?,
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
            tile_count: u64::try_from(root.entries.len())?,
            target_cell_count,
            tile_byte_length,
        })?
    );
    Ok(())
}

fn read_release_file(root: &Path, relative_path: &str) -> Result<Vec<u8>> {
    if Path::new(relative_path)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("release artifact path {relative_path:?} is not portable");
    }
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("resolve release directory {}", root.display()))?;
    let path = canonical_root.join(relative_path);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("inspect release artifact {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("release artifact {} must be a regular file", path.display());
    }
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("resolve release artifact {}", path.display()))?;
    if !canonical_path.starts_with(&canonical_root) {
        bail!("release artifact {relative_path:?} escapes its release directory");
    }
    fs::read(canonical_path).with_context(|| format!("read release artifact {relative_path:?}"))
}

const ETOPO_GRID_MAGIC: &[u8; 8] = b"ATCETOP1";
const ETOPO_GRID_SCHEMA_VERSION: u16 = 1;
const ETOPO_CENTRE_INDEX_MAGIC: &[u8; 8] = b"ATCECI1\0";
const ETOPO_CENTRE_INDEX_SCHEMA_VERSION: u16 = 1;
const ETOPO_CENTRE_SUMMARY_MAGIC: &[u8; 8] = b"ATCECS1\0";
const ETOPO_CENTRE_SUMMARY_SCHEMA_VERSION: u16 = 1;
const ETOPO_LATITUDE_CELLS: u64 = 10_800;
const ETOPO_LONGITUDE_CELLS: u64 = 21_600;
const ETOPO_FIRST_LATITUDE_CENTER_HALF_ARCSECONDS: i32 = -647_940;
const ETOPO_FIRST_LONGITUDE_CENTER_HALF_ARCSECONDS: i32 = -1_295_940;
const ETOPO_CELL_STEP_HALF_ARCSECONDS: i32 = 120;
const ETOPO_GRID_HEADER_LENGTH: usize = 84;
const ETOPO_CENTRE_INDEX_HEADER_LENGTH: usize = 88;
const ETOPO_CENTRE_INDEX_RECORD_LENGTH: usize = 12;
const ETOPO_CENTRE_SUMMARY_HEADER_LENGTH: usize = 124;
const ETOPO_CENTRE_SUMMARY_RECORD_LENGTH: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EtopoCellSupport {
    centre: GeographicCoordinateHalfArcsecond,
    south_boundary_half_arcseconds: i32,
    north_boundary_half_arcseconds: i32,
    west_boundary_half_arcseconds: i32,
    east_boundary_half_arcseconds: i32,
}

fn etopo_cell_support(row: u32, column: u32) -> Result<EtopoCellSupport> {
    if u64::from(row) >= ETOPO_LATITUDE_CELLS || u64::from(column) >= ETOPO_LONGITUDE_CELLS {
        bail!("ETOPO row and column must be within its pinned 10800 by 21600 grid");
    }
    let south_boundary_half_arcseconds = (-648_000_i32)
        .checked_add(
            i32::try_from(row)?
                .checked_mul(ETOPO_CELL_STEP_HALF_ARCSECONDS)
                .context("ETOPO row coordinate overflow")?,
        )
        .context("ETOPO latitude coordinate overflow")?;
    let west_boundary_half_arcseconds = (-1_296_000_i32)
        .checked_add(
            i32::try_from(column)?
                .checked_mul(ETOPO_CELL_STEP_HALF_ARCSECONDS)
                .context("ETOPO column coordinate overflow")?,
        )
        .context("ETOPO longitude coordinate overflow")?;
    let north_boundary_half_arcseconds = south_boundary_half_arcseconds
        .checked_add(ETOPO_CELL_STEP_HALF_ARCSECONDS)
        .context("ETOPO north boundary overflow")?;
    let east_boundary_half_arcseconds = west_boundary_half_arcseconds
        .checked_add(ETOPO_CELL_STEP_HALF_ARCSECONDS)
        .context("ETOPO east boundary overflow")?;
    let latitude_half_arcseconds = south_boundary_half_arcseconds
        .checked_add(ETOPO_CELL_STEP_HALF_ARCSECONDS / 2)
        .context("ETOPO latitude centre overflow")?;
    let longitude_half_arcseconds = west_boundary_half_arcseconds
        .checked_add(ETOPO_CELL_STEP_HALF_ARCSECONDS / 2)
        .context("ETOPO longitude centre overflow")?;
    if latitude_half_arcseconds
        != ETOPO_FIRST_LATITUDE_CENTER_HALF_ARCSECONDS
            + i32::try_from(row)? * ETOPO_CELL_STEP_HALF_ARCSECONDS
        || longitude_half_arcseconds
            != ETOPO_FIRST_LONGITUDE_CENTER_HALF_ARCSECONDS
                + i32::try_from(column)? * ETOPO_CELL_STEP_HALF_ARCSECONDS
    {
        bail!("ETOPO source-centre lattice disagrees with its declared area support");
    }
    Ok(EtopoCellSupport {
        centre: GeographicCoordinateHalfArcsecond::new(
            latitude_half_arcseconds,
            longitude_half_arcseconds,
        )
        .context("derive exact ETOPO area-cell centre")?,
        south_boundary_half_arcseconds,
        north_boundary_half_arcseconds,
        west_boundary_half_arcseconds,
        east_boundary_half_arcseconds,
    })
}

#[derive(Serialize)]
struct EtopoGridDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    sample_arc_minutes: u16,
    latitude_cells: u32,
    longitude_cells: u32,
    output_path: String,
    output_hash: Digest,
    output_byte_length: u64,
}

#[derive(Serialize)]
struct EtopoCentreIndexDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    sample_arc_minutes: u16,
    s2_level: u8,
    latitude_cells: u32,
    longitude_cells: u32,
    output_path: String,
    output_hash: Digest,
    output_byte_length: u64,
}

#[derive(Serialize)]
struct EtopoCentreSummaryDerivation {
    derivation_schema_version: u16,
    input_path: String,
    input_hash: Digest,
    source_snapshot_digest: Digest,
    source_artifact_hash: Digest,
    source_sample_arc_minutes: u16,
    source_s2_level: u8,
    summary_s2_level: u8,
    summary_cells: u32,
    source_samples: u64,
    output_path: String,
    output_hash: Digest,
    output_byte_length: u64,
}

#[derive(Serialize)]
struct EtopoQuadratureContributionDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    row: u32,
    column: u32,
    raw_elevation_ieee754_le_hex: String,
    target_s2_level: u8,
    points_per_axis: u8,
    target_cells: Vec<EtopoQuadratureContributionCell>,
}

#[derive(Serialize)]
struct EtopoQuadratureContributionCell {
    s2_cell_id: String,
    support_samples: u64,
    minimum_millimetres: i64,
    mean_millimetres: i64,
    maximum_millimetres: i64,
}

#[derive(Serialize)]
struct EtopoTerrainLayerDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    points_per_axis: u8,
    target_cells: u64,
    output_directory: String,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
}

#[derive(Clone, Copy, Debug)]
struct EtopoCentreIndexHeader {
    sample_arc_minutes: u16,
    s2_level: u8,
    snapshot_digest: Digest,
    artifact_digest: Digest,
    latitude_cells: u32,
    longitude_cells: u32,
}

#[derive(Clone, Copy, Debug)]
struct EtopoCentreIndexRecord {
    cell: S2CellId,
    value_bits: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EtopoCentreSummaryStats {
    samples: u64,
    total_millimetres: i64,
    minimum_millimetres: i64,
    maximum_millimetres: i64,
}

impl EtopoCentreSummaryStats {
    fn add(&mut self, millimetres: i64) -> Result<()> {
        self.add_weighted(millimetres, 1)
    }

    fn add_weighted(&mut self, millimetres: i64, samples: u64) -> Result<()> {
        if samples == 0 {
            bail!("ETOPO summary cannot add zero support samples");
        }
        if self.samples == 0 {
            self.minimum_millimetres = millimetres;
            self.maximum_millimetres = millimetres;
        } else {
            self.minimum_millimetres = self.minimum_millimetres.min(millimetres);
            self.maximum_millimetres = self.maximum_millimetres.max(millimetres);
        }
        self.samples = self
            .samples
            .checked_add(samples)
            .context("ETOPO summary sample overflow")?;
        let weighted = millimetres
            .checked_mul(i64::try_from(samples).context("ETOPO support sample count exceeds i64")?)
            .context("ETOPO summary weighted total overflow")?;
        self.total_millimetres = self
            .total_millimetres
            .checked_add(weighted)
            .context("ETOPO summary total overflow")?;
        Ok(())
    }

    fn mean_millimetres(self) -> i64 {
        debug_assert!(self.samples > 0);
        round_divide_i64(self.total_millimetres, self.samples as i64)
    }
}

/// Add one raw ETOPO source cell to its target-level S2 quadrature summaries.
///
/// This is the common kernel for the eventual global streaming derivation. Values are
/// converted to integer millimetres before aggregation, so the emitted tile does not
/// depend on host floating-point accumulation order.
fn accumulate_etopo_cell_quadrature(
    summaries: &mut BTreeMap<S2CellId, EtopoCentreSummaryStats>,
    row: u32,
    column: u32,
    value_bits: u32,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    let millimetres = f32_bits_to_rounded_millimetres(value_bits)?;
    for (cell, support_samples) in
        etopo_cell_quadrature(row, column, target_s2_level, points_per_axis)?
    {
        summaries
            .entry(cell)
            .or_default()
            .add_weighted(millimetres, u64::from(support_samples))?;
    }
    Ok(())
}

/// Fast equivalent of [`accumulate_etopo_cell_quadrature`] for the long full-source
/// pass. Hash-table iteration never reaches canonical bytes: the completed summaries
/// are sorted into a `BTreeMap` before tile packing.
fn accumulate_etopo_cell_quadrature_unordered(
    summaries: &mut HashMap<S2CellId, EtopoCentreSummaryStats>,
    row: u32,
    column: u32,
    value_bits: u32,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    let millimetres = f32_bits_to_rounded_millimetres(value_bits)?;
    for (cell, support_samples) in
        etopo_cell_quadrature(row, column, target_s2_level, points_per_axis)?
    {
        summaries
            .entry(cell)
            .or_default()
            .add_weighted(millimetres, u64::from(support_samples))?;
    }
    Ok(())
}

fn derive_etopo_quadrature_contribution(
    manifest_path: &Path,
    artifact_root: &Path,
    row: u32,
    column: u32,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    etopo_cell_support(row, column)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = etopo_data_artifact(&snapshot)?;
    let source_snapshot_id = snapshot.snapshot_id.clone();
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;
    let selection = NcSliceInfo {
        selections: vec![
            NcSliceInfoElem::Index(u64::from(row)),
            NcSliceInfoElem::Index(u64::from(column)),
        ],
    };
    let values = file
        .read_variable_slice::<f32>("z", &selection)
        .context("read exact ETOPO source elevation cell")?;
    let value = values
        .as_slice()
        .context("ETOPO source-cell selection is not contiguous")?
        .first()
        .copied()
        .context("ETOPO source-cell selection is empty")?;
    let mut summaries = BTreeMap::new();
    accumulate_etopo_cell_quadrature(
        &mut summaries,
        row,
        column,
        value.to_bits(),
        target_s2_level,
        points_per_axis,
    )?;
    let target_cells = summaries
        .into_iter()
        .map(|(s2_cell_id, stats)| EtopoQuadratureContributionCell {
            s2_cell_id: s2_cell_id.to_string(),
            support_samples: stats.samples,
            minimum_millimetres: stats.minimum_millimetres,
            mean_millimetres: stats.mean_millimetres(),
            maximum_millimetres: stats.maximum_millimetres,
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string(&EtopoQuadratureContributionDerivation {
            derivation_schema_version: 1,
            source_snapshot_id,
            source_snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            row,
            column,
            raw_elevation_ieee754_le_hex: format!("{:08x}", value.to_bits()),
            target_s2_level,
            points_per_axis,
            target_cells,
        })?
    );
    Ok(())
}

fn derive_etopo_terrain_layer(
    manifest_path: &Path,
    artifact_root: &Path,
    layer_id: &str,
    output_directory: &Path,
    container_s2_level: u8,
    target_s2_level: u8,
    points_per_axis: u8,
) -> Result<()> {
    if container_s2_level != 6 || target_s2_level != 10 {
        bail!(
            "ETOPO terrain-layer v1 requires L6 containers and L10 target values; requested L{container_s2_level} to L{target_s2_level}"
        );
    }
    if points_per_axis == 0 || 60 % points_per_axis != 0 {
        bail!("points_per_axis must be a non-zero divisor of 60");
    }
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!(
            "ETOPO terrain output directory {} already exists",
            output_directory.display()
        );
    }
    let output_parent = output_directory
        .parent()
        .context("ETOPO terrain output directory has no parent")?;
    if !output_parent.is_dir() {
        bail!(
            "ETOPO terrain output parent {} is not a directory",
            output_parent.display()
        );
    }

    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = etopo_data_artifact(&snapshot)?;
    let source_snapshot_id = snapshot.snapshot_id.clone();
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;

    let mut summaries = HashMap::new();
    for row in 0..ETOPO_LATITUDE_CELLS {
        let selection = NcSliceInfo {
            selections: vec![
                NcSliceInfoElem::Slice {
                    start: row,
                    end: row.checked_add(1).context("ETOPO row selection overflow")?,
                    step: 1,
                },
                NcSliceInfoElem::Slice {
                    start: 0,
                    end: ETOPO_LONGITUDE_CELLS,
                    step: 1,
                },
            ],
        };
        let values = file
            .read_variable_slice::<f32>("z", &selection)
            .with_context(|| format!("read ETOPO row {row}"))?;
        let values = values
            .as_slice()
            .context("ETOPO row selection is not contiguous")?;
        if values.len() != usize::try_from(ETOPO_LONGITUDE_CELLS)? {
            bail!("ETOPO row {row} has an unexpected sample count");
        }
        for (column, value) in values.iter().copied().enumerate() {
            accumulate_etopo_cell_quadrature_unordered(
                &mut summaries,
                u32::try_from(row)?,
                u32::try_from(column)?,
                value.to_bits(),
                target_s2_level,
                points_per_axis,
            )?;
        }
        // Progress is operational diagnostics only. It is written to stderr and has no
        // effect on emitted canonical bytes or the final derivation record.
        let completed_rows = row + 1;
        if completed_rows % 360 == 0 || completed_rows == ETOPO_LATITUDE_CELLS {
            eprintln!(
                "ETOPO terrain normalization progress: {completed_rows}/{ETOPO_LATITUDE_CELLS} source rows"
            );
        }
    }

    // Canonical packing performs ordered point lookups. Sorting only after every
    // source contribution is accumulated retains bit-for-bit output while avoiding a
    // tree traversal on each interior quadrature update.
    let summaries = summaries.into_iter().collect::<BTreeMap<_, _>>();

    let staging_directory =
        prepare_or_resume_natural_earth_land_staging_directory(output_directory)?;
    let (root_relative_path, root_bytes) = write_packed_etopo_terrain_layer(
        &staging_directory,
        EtopoTerrainPackingProfile {
            layer_id,
            source_snapshot_digest,
            source_artifact_digest: artifact.content_hash,
            points_per_axis,
            container_s2_level,
            target_s2_level,
        },
        &summaries,
    )?;
    fs::rename(&staging_directory, output_directory).with_context(|| {
        format!(
            "atomically publish ETOPO terrain directory {}",
            output_directory.display()
        )
    })?;
    println!(
        "{}",
        serde_json::to_string(&EtopoTerrainLayerDerivation {
            derivation_schema_version: 1,
            source_snapshot_id,
            source_snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            points_per_axis,
            target_cells: u64::try_from(summaries.len())?,
            output_directory: output_directory.display().to_string(),
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct NaturalEarthLandReferenceLayerDerivation {
    derivation_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    source_artifact_path: String,
    source_artifact_hash: Digest,
    layer_id: String,
    container_s2_level: u8,
    target_s2_level: u8,
    sample_policy: &'static str,
    target_cells: u64,
    output_directory: String,
    root_index_path: String,
    root_index_hash: Digest,
    root_index_byte_length: u64,
}

/// Create a full global, centre-classified Natural Earth land-reference layer.
///
/// This is deliberately distinct from a coastline layer: source vertices are
/// generalized cartography. The Boolean payload says only whether the exact centre
/// of each target S2 cell is inside the pinned polygon stream.
fn derive_natural_earth_land_reference_layer(
    manifest_path: &Path,
    artifact_root: &Path,
    layer_id: &str,
    output_directory: &Path,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<()> {
    if container_s2_level != 6 || target_s2_level != 10 {
        bail!("Natural Earth land-reference v1 requires L6 containers and L10 target values");
    }
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!("land-reference output directory already exists");
    }
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".shp")
        })
        .context("source snapshot has no Natural Earth .shp data artifact")?;
    let bytes = fs::read(artifact_root.join(&artifact.artifact_path))?;
    let prepared_land = PreparedNaturalEarthLand::from_shapefile(&bytes)
        .context("validate and prepare Natural Earth polygon stream")?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let staging_directory = prepare_terrain_layer_staging_directory(output_directory)?;
    let (root_relative_path, root_bytes) = write_packed_natural_earth_land_reference_layer(
        &staging_directory,
        layer_id,
        source_snapshot_digest,
        artifact.content_hash,
        &prepared_land,
        container_s2_level,
        target_s2_level,
    )?;
    fs::rename(&staging_directory, output_directory).with_context(|| {
        format!(
            "atomically publish land-reference directory {}",
            output_directory.display()
        )
    })?;
    let target_cells = u64::try_from(global_s2_cells_at_level(target_s2_level)?.len())?;
    println!(
        "{}",
        serde_json::to_string(&NaturalEarthLandReferenceLayerDerivation {
            derivation_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            layer_id: layer_id.to_owned(),
            container_s2_level,
            target_s2_level,
            sample_policy: "s2-cell-centre-e7-v1",
            target_cells,
            output_directory: output_directory.display().to_string(),
            root_index_path: root_relative_path,
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len())?,
        })?
    );
    Ok(())
}

fn write_packed_natural_earth_land_reference_layer(
    output_directory: &Path,
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    prepared_land: &PreparedNaturalEarthLand,
    container_s2_level: u8,
    target_s2_level: u8,
) -> Result<(String, Vec<u8>)> {
    let level_directory = format!("l{container_s2_level}");
    let tile_directory = output_directory
        .join("layers")
        .join(layer_id)
        .join(&level_directory);
    fs::create_dir_all(&tile_directory)?;
    let mut entries = Vec::new();
    for (position, container) in global_s2_cells_at_level(container_s2_level)?
        .into_iter()
        .enumerate()
    {
        let relative_path = format!("layers/{layer_id}/{level_directory}/{container}.tile");
        let artifact_path = output_directory.join(&relative_path);
        let bytes = match fs::read(&artifact_path) {
            Ok(existing) => {
                validate_resumable_natural_earth_land_tile(
                    &existing,
                    layer_id,
                    source_snapshot_digest,
                    source_artifact_digest,
                    container,
                    target_s2_level,
                )?;
                existing
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let tile = pack_natural_earth_land_reference_tile(
                    layer_id,
                    source_snapshot_digest,
                    source_artifact_digest,
                    prepared_land,
                    container,
                    target_s2_level,
                )?;
                let bytes = tile.canonical_bytes()?;
                write_new_artifact(&artifact_path, &bytes)?;
                bytes
            }
            Err(error) => return Err(error).context("read staged land-reference tile"),
        };
        entries.push(TileTreeEntry {
            kind: TileTreeEntryKind::Tile,
            s2_cell_id: container.to_string(),
            s2_level: container_s2_level,
            artifact: TileArtifactReference {
                path: relative_path,
                media_type: PACKED_BOOLEAN_FIELD_TILE_MEDIA_TYPE.to_owned(),
                content_hash: Digest::sha256(&bytes),
                byte_length: u64::try_from(bytes.len())?,
            },
        });
        if (position + 1) % 1_024 == 0 {
            eprintln!(
                "Natural Earth land-reference normalization progress: {}/24576 containers",
                position + 1
            );
        }
    }
    let root = TileTreeIndex {
        index_schema_version: 1,
        layer_id: layer_id.to_owned(),
        entries,
    };
    let root_bytes = root.canonical_bytes()?;
    let root_relative_path = format!("layers/{layer_id}/root.index");
    let root_path = output_directory.join(&root_relative_path);
    match fs::read(&root_path) {
        Ok(existing) if existing == root_bytes => {}
        Ok(_) => bail!("staged land-reference root does not match the requested derivation"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_new_artifact(&root_path, &root_bytes)?;
        }
        Err(error) => return Err(error).context("read staged land-reference root"),
    }
    Ok((root_relative_path, root_bytes))
}

fn validate_resumable_natural_earth_land_tile(
    bytes: &[u8],
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
) -> Result<()> {
    let tile = PackedBooleanFieldTile::from_canonical_slice(bytes)
        .context("decode staged land-reference tile")?;
    if tile.canonical_bytes()? != bytes {
        bail!("staged land-reference tile is not canonical");
    }
    if tile.tile_schema_version != 1
        || tile.layer_id != layer_id
        || tile.source_snapshot_digest != source_snapshot_digest
        || tile.source_artifact_digest != source_artifact_digest
        || tile.sample_policy != "s2-cell-centre-e7-v1"
        || tile.container_s2_cell_id != container_s2_cell_id
        || tile.target_s2_level != target_s2_level
    {
        bail!("staged land-reference tile does not match the requested derivation");
    }
    tile.validate()
        .context("validate staged land-reference tile")
}

fn pack_natural_earth_land_reference_tile(
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    prepared_land: &PreparedNaturalEarthLand,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
) -> Result<PackedBooleanFieldTile> {
    let cells = enumerate_s2_descendants(container_s2_cell_id, target_s2_level)?
        .into_iter()
        .map(|s2_cell_id| {
            let coordinate = s2_ray_to_geographic_e7(s2_face_uv_to_ray(s2_face_ij_center_uv(
                decode_s2_face_ij(s2_cell_id),
            )?)?)?;
            Ok(BooleanFieldCell {
                s2_cell_id,
                support_samples: 1,
                true_samples: u64::from(
                    prepared_land
                        .contains_point(coordinate.longitude_e7(), coordinate.latitude_e7()),
                ),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let tile = PackedBooleanFieldTile {
        tile_schema_version: 1,
        layer_id: layer_id.to_owned(),
        source_snapshot_digest,
        source_artifact_digest,
        sample_policy: "s2-cell-centre-e7-v1".to_owned(),
        container_s2_cell_id,
        target_s2_level,
        cells,
    };
    tile.validate()
        .context("packed land-reference tile is invalid")?;
    Ok(tile)
}

/// Reserve a same-parent hidden staging directory. The final rename is atomic on the
/// release filesystem, so readers can observe either no release or a complete root and
/// its referenced tiles, never the writer's intermediate tree.
fn prepare_terrain_layer_staging_directory(output_directory: &Path) -> Result<PathBuf> {
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!(
            "ETOPO terrain output directory {} already exists",
            output_directory.display()
        );
    }
    let output_parent = output_directory
        .parent()
        .context("ETOPO terrain output directory has no parent")?;
    if !output_parent.is_dir() {
        bail!(
            "ETOPO terrain output parent {} is not a directory",
            output_parent.display()
        );
    }
    let output_name = output_directory
        .file_name()
        .and_then(OsStr::to_str)
        .context("ETOPO terrain output directory name is not UTF-8")?;
    let staging_directory = output_parent.join(format!(".{output_name}.staging"));
    if fs::symlink_metadata(&staging_directory).is_ok() {
        bail!(
            "ETOPO terrain staging directory {} already exists; inspect it before retrying",
            staging_directory.display()
        );
    }
    fs::create_dir(&staging_directory).with_context(|| {
        format!(
            "create hidden ETOPO terrain staging directory {}",
            staging_directory.display()
        )
    })?;
    Ok(staging_directory)
}

/// Resume only the unpublished, hidden Natural Earth staging tree. Every reused tile
/// is decoded, re-canonicalized, and checked against the current layer/source/profile
/// before it is admitted into the new root index. This makes an interrupted long global
/// derivation recoverable without ever exposing a partial release or replacing data.
fn prepare_or_resume_natural_earth_land_staging_directory(
    output_directory: &Path,
) -> Result<PathBuf> {
    if fs::symlink_metadata(output_directory).is_ok() {
        bail!(
            "land-reference output directory {} already exists",
            output_directory.display()
        );
    }
    let output_parent = output_directory
        .parent()
        .context("land-reference output directory has no parent")?;
    if !output_parent.is_dir() {
        bail!(
            "land-reference output parent {} is not a directory",
            output_parent.display()
        );
    }
    let output_name = output_directory
        .file_name()
        .and_then(OsStr::to_str)
        .context("land-reference output directory name is not UTF-8")?;
    let staging_directory = output_parent.join(format!(".{output_name}.staging"));
    match fs::symlink_metadata(&staging_directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => bail!(
            "land-reference staging path {} is not a real directory",
            staging_directory.display()
        ),
        Ok(_) => Ok(staging_directory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&staging_directory)?;
            Ok(staging_directory)
        }
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone, Copy)]
struct EtopoTerrainPackingProfile<'a> {
    layer_id: &'a str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    points_per_axis: u8,
    container_s2_level: u8,
    target_s2_level: u8,
}

fn write_packed_etopo_terrain_layer(
    output_directory: &Path,
    profile: EtopoTerrainPackingProfile<'_>,
    summaries: &std::collections::BTreeMap<S2CellId, EtopoCentreSummaryStats>,
) -> Result<(String, Vec<u8>)> {
    let level_directory = format!("l{}", profile.container_s2_level);
    let layer_directory = output_directory.join("layers").join(profile.layer_id);
    let tile_directory = layer_directory.join(&level_directory);
    fs::create_dir_all(&tile_directory).with_context(|| {
        format!(
            "create ETOPO terrain tile directory {}",
            tile_directory.display()
        )
    })?;

    let mut entries = Vec::new();
    for container in global_s2_cells_at_level(profile.container_s2_level)? {
        let tile = pack_etopo_terrain_tile(
            profile.layer_id,
            profile.source_snapshot_digest,
            profile.source_artifact_digest,
            profile.points_per_axis,
            container,
            profile.target_s2_level,
            summaries,
        )?;
        let bytes = tile.canonical_bytes()?;
        let relative_path = format!(
            "layers/{}/{level_directory}/{container}.tile",
            profile.layer_id
        );
        write_new_artifact(&output_directory.join(&relative_path), &bytes)?;
        entries.push(TileTreeEntry {
            kind: TileTreeEntryKind::Tile,
            s2_cell_id: container.to_string(),
            s2_level: profile.container_s2_level,
            artifact: TileArtifactReference {
                path: relative_path,
                media_type: PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE.to_owned(),
                content_hash: Digest::sha256(&bytes),
                byte_length: u64::try_from(bytes.len())?,
            },
        });
    }
    let root = TileTreeIndex {
        index_schema_version: 1,
        layer_id: profile.layer_id.to_owned(),
        entries,
    };
    let root_bytes = root.canonical_bytes()?;
    let root_relative_path = format!("layers/{}/root.index", profile.layer_id);
    write_new_artifact(&output_directory.join(&root_relative_path), &root_bytes)?;
    Ok((root_relative_path, root_bytes))
}

fn global_s2_cells_at_level(target_s2_level: u8) -> Result<Vec<S2CellId>> {
    if target_s2_level > MAX_S2_LEVEL {
        bail!("global S2 level must be within 0 through {MAX_S2_LEVEL}");
    }
    let mut cells = Vec::new();
    for face in 0_u64..6 {
        let root = S2CellId::new((face << 61) | (1_u64 << 60))?;
        if target_s2_level == 0 {
            cells.push(root);
        } else {
            cells.extend(enumerate_s2_descendants(root, target_s2_level)?);
        }
    }
    Ok(cells)
}

/// Pack all known target summaries in one coarse S2 container into a release tile.
fn pack_etopo_terrain_tile(
    layer_id: &str,
    source_snapshot_digest: Digest,
    source_artifact_digest: Digest,
    quadrature_points_per_axis: u8,
    container_s2_cell_id: S2CellId,
    target_s2_level: u8,
    summaries: &std::collections::BTreeMap<S2CellId, EtopoCentreSummaryStats>,
) -> Result<PackedScalarTerrainTile> {
    let cells = enumerate_s2_descendants(container_s2_cell_id, target_s2_level)?
        .into_iter()
        .map(|s2_cell_id| {
            let stats = summaries.get(&s2_cell_id).with_context(|| {
                format!("ETOPO quadrature has no support for expected target S2 cell {s2_cell_id}")
            })?;
            Ok(ScalarTerrainCell {
                s2_cell_id,
                support_samples: stats.samples,
                minimum_millimetres: stats.minimum_millimetres,
                mean_millimetres: stats.mean_millimetres(),
                maximum_millimetres: stats.maximum_millimetres,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let tile = PackedScalarTerrainTile {
        tile_schema_version: 1,
        layer_id: layer_id.to_owned(),
        source_snapshot_digest,
        source_artifact_digest,
        quadrature_points_per_axis,
        container_s2_cell_id,
        target_s2_level,
        cells,
    };
    tile.validate()
        .context("packed ETOPO terrain tile is invalid")?;
    Ok(tile)
}

fn enumerate_s2_descendants(root: S2CellId, target_s2_level: u8) -> Result<Vec<S2CellId>> {
    if target_s2_level <= root.level() || target_s2_level > MAX_S2_LEVEL {
        bail!(
            "target S2 level {target_s2_level} must be finer than container level {} and at most {MAX_S2_LEVEL}",
            root.level()
        );
    }
    let mut current = vec![root];
    while current
        .first()
        .is_some_and(|cell| cell.level() < target_s2_level)
    {
        let mut next = Vec::with_capacity(
            current
                .len()
                .checked_mul(4)
                .context("S2 descendant count overflow")?,
        );
        for cell in current {
            next.extend(cell.children().context("enumerate S2 children")?);
        }
        current = next;
    }
    Ok(current)
}

fn derive_etopo_grid(
    manifest_path: &Path,
    artifact_root: &Path,
    sample_arc_minutes: u16,
    output_path: &Path,
) -> Result<()> {
    let stride = validate_etopo_sample_stride(sample_arc_minutes)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = etopo_data_artifact(&snapshot)?;
    let snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;
    let selection = NcSliceInfo {
        selections: vec![
            NcSliceInfoElem::Slice {
                start: 0,
                end: ETOPO_LATITUDE_CELLS,
                step: stride,
            },
            NcSliceInfoElem::Slice {
                start: 0,
                end: ETOPO_LONGITUDE_CELLS,
                step: stride,
            },
        ],
    };
    let samples = file
        .read_variable_slice::<f32>("z", &selection)
        .context("read selected ETOPO elevation cells")?;
    let latitude_cells = u32::try_from(ETOPO_LATITUDE_CELLS / stride)?;
    let longitude_cells = u32::try_from(ETOPO_LONGITUDE_CELLS / stride)?;
    let expected_samples = usize::try_from(u64::from(latitude_cells) * u64::from(longitude_cells))?;
    let values = samples
        .as_slice()
        .context("ETOPO selection is not contiguous")?;
    if values.len() != expected_samples {
        bail!("ETOPO selection has an unexpected number of cells");
    }
    let bytes = encode_etopo_grid(
        sample_arc_minutes,
        snapshot_digest,
        artifact.content_hash,
        latitude_cells,
        longitude_cells,
        values,
    )?;
    write_new_artifact(output_path, &bytes)?;
    let output_hash = Digest::sha256(&bytes);
    println!(
        "{}",
        serde_json::to_string(&EtopoGridDerivation {
            derivation_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id.clone(),
            source_snapshot_digest: snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            sample_arc_minutes,
            latitude_cells,
            longitude_cells,
            output_path: output_path.display().to_string(),
            output_hash,
            output_byte_length: u64::try_from(bytes.len())?,
        })?
    );
    Ok(())
}

fn derive_etopo_centre_index(
    manifest_path: &Path,
    artifact_root: &Path,
    sample_arc_minutes: u16,
    s2_level: u8,
    output_path: &Path,
) -> Result<()> {
    let stride = validate_etopo_sample_stride(sample_arc_minutes)?;
    if s2_level > MAX_S2_LEVEL {
        bail!("s2_level must be within 0 through {MAX_S2_LEVEL}");
    }
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = etopo_data_artifact(&snapshot)?;
    let snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;
    let selection = NcSliceInfo {
        selections: vec![
            NcSliceInfoElem::Slice {
                start: 0,
                end: ETOPO_LATITUDE_CELLS,
                step: stride,
            },
            NcSliceInfoElem::Slice {
                start: 0,
                end: ETOPO_LONGITUDE_CELLS,
                step: stride,
            },
        ],
    };
    let samples = file
        .read_variable_slice::<f32>("z", &selection)
        .context("read selected ETOPO elevation cells")?;
    let latitude_cells = u32::try_from(ETOPO_LATITUDE_CELLS / stride)?;
    let longitude_cells = u32::try_from(ETOPO_LONGITUDE_CELLS / stride)?;
    let expected_samples = usize::try_from(u64::from(latitude_cells) * u64::from(longitude_cells))?;
    let values = samples
        .as_slice()
        .context("ETOPO selection is not contiguous")?;
    if values.len() != expected_samples {
        bail!("ETOPO selection has an unexpected number of cells");
    }
    let bytes = encode_etopo_centre_index(
        sample_arc_minutes,
        s2_level,
        snapshot_digest,
        artifact.content_hash,
        latitude_cells,
        longitude_cells,
        values,
    )?;
    write_new_artifact(output_path, &bytes)?;
    let output_hash = Digest::sha256(&bytes);
    println!(
        "{}",
        serde_json::to_string(&EtopoCentreIndexDerivation {
            derivation_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id.clone(),
            source_snapshot_digest: snapshot_digest,
            source_artifact_path: artifact.artifact_path.clone(),
            source_artifact_hash: artifact.content_hash,
            sample_arc_minutes,
            s2_level,
            latitude_cells,
            longitude_cells,
            output_path: output_path.display().to_string(),
            output_hash,
            output_byte_length: u64::try_from(bytes.len())?,
        })?
    );
    Ok(())
}

fn derive_etopo_centre_summary(
    input_path: &Path,
    summary_s2_level: u8,
    output_path: &Path,
) -> Result<()> {
    if summary_s2_level > MAX_S2_LEVEL {
        bail!("s2_level must be within 0 through {MAX_S2_LEVEL}");
    }
    let bytes = fs::read(input_path)
        .with_context(|| format!("read ETOPO centre index {}", input_path.display()))?;
    let input_hash = Digest::sha256(&bytes);
    let (header, records) = decode_etopo_centre_index(&bytes)?;
    if summary_s2_level > header.s2_level {
        bail!(
            "summary s2_level {summary_s2_level} is finer than centre index level {}",
            header.s2_level
        );
    }

    let mut cells = std::collections::BTreeMap::<S2CellId, EtopoCentreSummaryStats>::new();
    for (sample_index, record) in records.iter().copied().enumerate() {
        let source_cell = record.cell;
        let expected = expected_etopo_index_cell(sample_index, header)?;
        if source_cell != expected {
            bail!("centre-index record {sample_index} disagrees with the pinned source lattice");
        }
        let target = source_cell
            .ancestor(summary_s2_level)
            .context("derive coarser S2 source-attribution address")?;
        let millimetres = f32_bits_to_rounded_millimetres(record.value_bits)?;
        cells.entry(target).or_default().add(millimetres)?;
    }
    let source_samples = u64::from(header.latitude_cells)
        .checked_mul(u64::from(header.longitude_cells))
        .context("ETOPO centre index sample count overflow")?;
    let output =
        encode_etopo_centre_summary(input_hash, header, summary_s2_level, &cells, source_samples)?;
    write_new_artifact(output_path, &output)?;
    println!(
        "{}",
        serde_json::to_string(&EtopoCentreSummaryDerivation {
            derivation_schema_version: 1,
            input_path: input_path.display().to_string(),
            input_hash,
            source_snapshot_digest: header.snapshot_digest,
            source_artifact_hash: header.artifact_digest,
            source_sample_arc_minutes: header.sample_arc_minutes,
            source_s2_level: header.s2_level,
            summary_s2_level,
            summary_cells: u32::try_from(cells.len())?,
            source_samples,
            output_path: output_path.display().to_string(),
            output_hash: Digest::sha256(&output),
            output_byte_length: u64::try_from(output.len())?,
        })?
    );
    Ok(())
}

fn validate_etopo_sample_stride(sample_arc_minutes: u16) -> Result<u64> {
    let stride = u64::from(sample_arc_minutes);
    if sample_arc_minutes == 0
        || 60 % sample_arc_minutes != 0
        || !ETOPO_LATITUDE_CELLS.is_multiple_of(stride)
        || !ETOPO_LONGITUDE_CELLS.is_multiple_of(stride)
    {
        bail!("sample_arc_minutes must be a non-zero divisor of 60");
    }
    Ok(stride)
}

fn etopo_data_artifact(snapshot: &SourceSnapshotManifest) -> Result<&SourceSnapshotArtifact> {
    snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no ETOPO NetCDF data artifact")
}

fn validate_etopo_schema(file: &NcFile) -> Result<()> {
    let lat = file.variable("lat").context("ETOPO has no lat variable")?;
    let lon = file.variable("lon").context("ETOPO has no lon variable")?;
    let elevation = file.variable("z").context("ETOPO has no z variable")?;
    if lat.shape() != [ETOPO_LATITUDE_CELLS]
        || lon.shape() != [ETOPO_LONGITUDE_CELLS]
        || elevation.shape() != [ETOPO_LATITUDE_CELLS, ETOPO_LONGITUDE_CELLS]
    {
        bail!("ETOPO variables do not have the pinned global 60-arc-second shape");
    }
    validate_etopo_coordinate_axes(file)?;
    Ok(())
}

fn validate_etopo_coordinate_axes(file: &NcFile) -> Result<()> {
    validate_etopo_coordinate_axis(
        file,
        "lat",
        ETOPO_LATITUDE_CELLS,
        ETOPO_FIRST_LATITUDE_CENTER_HALF_ARCSECONDS,
        "latitude",
    )?;
    validate_etopo_coordinate_axis(
        file,
        "lon",
        ETOPO_LONGITUDE_CELLS,
        ETOPO_FIRST_LONGITUDE_CENTER_HALF_ARCSECONDS,
        "longitude",
    )
}

fn validate_etopo_coordinate_axis(
    file: &NcFile,
    variable_name: &str,
    expected_length: u64,
    first_half_arcseconds: i32,
    axis_name: &str,
) -> Result<()> {
    let values = file
        .read_variable::<f64>(variable_name)
        .with_context(|| format!("read ETOPO {axis_name} coordinate axis"))?;
    let values = values
        .as_slice()
        .with_context(|| format!("ETOPO {axis_name} coordinate axis is not contiguous"))?;
    if values.len() != usize::try_from(expected_length)? {
        bail!("ETOPO {axis_name} coordinate axis has an unexpected length");
    }
    for (index, value) in values.iter().copied().enumerate() {
        let expected = first_half_arcseconds
            .checked_add(
                i32::try_from(index)?
                    .checked_mul(ETOPO_CELL_STEP_HALF_ARCSECONDS)
                    .context("ETOPO axis index overflow")?,
            )
            .context("ETOPO coordinate axis overflow")?;
        let observed = f64_to_half_arcseconds(value).with_context(|| {
            format!("ETOPO {axis_name} coordinate {index} is not on the half-arcsecond lattice")
        })?;
        if observed != expected {
            bail!(
                "ETOPO {axis_name} coordinate {index} is {observed} half-arcseconds, expected {expected}"
            );
        }
    }
    Ok(())
}

fn f64_to_half_arcseconds(value: f64) -> Result<i32> {
    if !value.is_finite() {
        bail!("coordinate is not finite");
    }
    let scaled = value * 7_200.0;
    let rounded = scaled.round();
    if (scaled - rounded).abs() > 0.000_001 {
        bail!("coordinate is not within one millionth half-arcsecond of the source lattice");
    }
    i32::try_from(rounded as i64).context("coordinate is outside the half-arcsecond domain")
}

fn encode_etopo_grid(
    sample_arc_minutes: u16,
    snapshot_digest: Digest,
    artifact_digest: Digest,
    latitude_cells: u32,
    longitude_cells: u32,
    values: &[f32],
) -> Result<Vec<u8>> {
    let expected_values = usize::try_from(u64::from(latitude_cells) * u64::from(longitude_cells))?;
    if values.len() != expected_values {
        bail!("ETOPO grid value count disagrees with its declared dimensions");
    }
    let value_bytes = values
        .len()
        .checked_mul(std::mem::size_of::<u32>())
        .context("ETOPO grid byte length overflow")?;
    let total = ETOPO_GRID_HEADER_LENGTH
        .checked_add(value_bytes)
        .context("ETOPO grid total byte length overflow")?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(ETOPO_GRID_MAGIC);
    bytes.extend_from_slice(&ETOPO_GRID_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&sample_arc_minutes.to_le_bytes());
    bytes.extend_from_slice(snapshot_digest.as_bytes());
    bytes.extend_from_slice(artifact_digest.as_bytes());
    bytes.extend_from_slice(&latitude_cells.to_le_bytes());
    bytes.extend_from_slice(&longitude_cells.to_le_bytes());
    for value in values {
        bytes.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    Ok(bytes)
}

fn encode_etopo_centre_index(
    sample_arc_minutes: u16,
    s2_level: u8,
    snapshot_digest: Digest,
    artifact_digest: Digest,
    latitude_cells: u32,
    longitude_cells: u32,
    values: &[f32],
) -> Result<Vec<u8>> {
    let stride = validate_etopo_sample_stride(sample_arc_minutes)?;
    let expected_values = usize::try_from(u64::from(latitude_cells) * u64::from(longitude_cells))?;
    if values.len() != expected_values
        || u64::from(latitude_cells) * stride != ETOPO_LATITUDE_CELLS
        || u64::from(longitude_cells) * stride != ETOPO_LONGITUDE_CELLS
    {
        bail!("ETOPO centre index dimensions disagree with sampled source values");
    }
    let record_bytes = values
        .len()
        .checked_mul(ETOPO_CENTRE_INDEX_RECORD_LENGTH)
        .context("ETOPO centre index byte length overflow")?;
    let total = ETOPO_CENTRE_INDEX_HEADER_LENGTH
        .checked_add(record_bytes)
        .context("ETOPO centre index total byte length overflow")?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(ETOPO_CENTRE_INDEX_MAGIC);
    bytes.extend_from_slice(&ETOPO_CENTRE_INDEX_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&sample_arc_minutes.to_le_bytes());
    bytes.push(s2_level);
    bytes.extend_from_slice(&[0_u8; 3]);
    bytes.extend_from_slice(snapshot_digest.as_bytes());
    bytes.extend_from_slice(artifact_digest.as_bytes());
    bytes.extend_from_slice(&latitude_cells.to_le_bytes());
    bytes.extend_from_slice(&longitude_cells.to_le_bytes());

    for row in 0..latitude_cells {
        let source_row = u32::try_from(u64::from(row) * stride)?;
        for column in 0..longitude_cells {
            let source_column = u32::try_from(u64::from(column) * stride)?;
            let support = etopo_cell_support(source_row, source_column)?;
            let cell = route_half_arcsecond_to_s2(support.centre, s2_level)
                .context("route exact ETOPO source centre to S2")?;
            let value_index =
                usize::try_from(u64::from(row) * u64::from(longitude_cells) + u64::from(column))?;
            bytes.extend_from_slice(&cell.get().to_be_bytes());
            bytes.extend_from_slice(&values[value_index].to_bits().to_le_bytes());
        }
    }
    debug_assert_eq!(bytes.len(), total);
    Ok(bytes)
}

fn decode_etopo_centre_index(
    bytes: &[u8],
) -> Result<(EtopoCentreIndexHeader, Vec<EtopoCentreIndexRecord>)> {
    if bytes.len() < ETOPO_CENTRE_INDEX_HEADER_LENGTH {
        bail!("ETOPO centre index is shorter than its header");
    }
    if &bytes[..8] != ETOPO_CENTRE_INDEX_MAGIC {
        bail!("ETOPO centre index magic is invalid");
    }
    let schema_version = u16::from_le_bytes(bytes[8..10].try_into()?);
    if schema_version != ETOPO_CENTRE_INDEX_SCHEMA_VERSION {
        bail!("ETOPO centre index schema version {schema_version} is unsupported");
    }
    let sample_arc_minutes = u16::from_le_bytes(bytes[10..12].try_into()?);
    let s2_level = bytes[12];
    if bytes[13..16] != [0_u8; 3] {
        bail!("ETOPO centre index reserved bytes must be zero");
    }
    let header = EtopoCentreIndexHeader {
        sample_arc_minutes,
        s2_level,
        snapshot_digest: Digest::from_bytes(bytes[16..48].try_into()?),
        artifact_digest: Digest::from_bytes(bytes[48..80].try_into()?),
        latitude_cells: u32::from_le_bytes(bytes[80..84].try_into()?),
        longitude_cells: u32::from_le_bytes(bytes[84..88].try_into()?),
    };
    let stride = validate_etopo_sample_stride(header.sample_arc_minutes)?;
    if header.s2_level > MAX_S2_LEVEL
        || u64::from(header.latitude_cells) * stride != ETOPO_LATITUDE_CELLS
        || u64::from(header.longitude_cells) * stride != ETOPO_LONGITUDE_CELLS
    {
        bail!("ETOPO centre index header dimensions or S2 level are invalid");
    }
    let record_count = usize::try_from(
        u64::from(header.latitude_cells)
            .checked_mul(u64::from(header.longitude_cells))
            .context("ETOPO centre index record count overflow")?,
    )?;
    let expected_length = ETOPO_CENTRE_INDEX_HEADER_LENGTH
        .checked_add(
            record_count
                .checked_mul(ETOPO_CENTRE_INDEX_RECORD_LENGTH)
                .context("ETOPO centre index byte length overflow")?,
        )
        .context("ETOPO centre index total byte length overflow")?;
    if bytes.len() != expected_length {
        bail!("ETOPO centre index byte length disagrees with its header");
    }
    let records = bytes[ETOPO_CENTRE_INDEX_HEADER_LENGTH..]
        .chunks_exact(ETOPO_CENTRE_INDEX_RECORD_LENGTH)
        .map(|record| {
            Ok(EtopoCentreIndexRecord {
                cell: S2CellId::new(u64::from_be_bytes(
                    record[..8].try_into().expect("fixed record cell"),
                ))?,
                value_bits: u32::from_le_bytes(
                    record[8..12].try_into().expect("fixed record value"),
                ),
            })
        });
    Ok((header, records.collect::<Result<Vec<_>>>()?))
}

fn expected_etopo_index_cell(
    sample_index: usize,
    header: EtopoCentreIndexHeader,
) -> Result<S2CellId> {
    let columns = usize::try_from(header.longitude_cells)?;
    let row = u32::try_from(sample_index / columns)?;
    let column = u32::try_from(sample_index % columns)?;
    let stride = validate_etopo_sample_stride(header.sample_arc_minutes)?;
    let source_row = u32::try_from(u64::from(row) * stride)?;
    let source_column = u32::try_from(u64::from(column) * stride)?;
    route_half_arcsecond_to_s2(
        etopo_cell_support(source_row, source_column)?.centre,
        header.s2_level,
    )
    .context("route expected ETOPO centre-index source cell")
}

fn f32_bits_to_rounded_millimetres(bits: u32) -> Result<i64> {
    let sign = if bits >> 31 == 0 { 1_i128 } else { -1_i128 };
    let exponent = ((bits >> 23) & 0xff) as i32;
    let fraction = bits & 0x007f_ffff;
    if exponent == 0xff {
        bail!("ETOPO value is not finite");
    }
    let (significand, power) = if exponent == 0 {
        (i128::from(fraction), -149)
    } else {
        (i128::from((1_u32 << 23) | fraction), exponent - 150)
    };
    let numerator = sign
        .checked_mul(significand)
        .and_then(|value| value.checked_mul(1_000))
        .context("ETOPO millimetre conversion overflow")?;
    let millimetres = if power >= 0 {
        numerator
            .checked_shl(u32::try_from(power)?)
            .context("ETOPO millimetre conversion overflow")?
    } else {
        let divisor_shift = u32::try_from(-power)?;
        // A finite f32 significand times 1,000 is below 2^34. Any divisor at
        // 2^127 or greater therefore rounds to zero at millimetre precision and
        // cannot be a nearest-even tie. Avoid constructing an out-of-range i128.
        if divisor_shift >= 127 {
            0
        } else {
            round_divide_i128(numerator, 1_i128 << divisor_shift)
        }
    };
    i64::try_from(millimetres).context("ETOPO value is outside signed millimetre range")
}

fn round_divide_i128(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let twice_remainder = remainder.unsigned_abs() * 2;
    let denominator = denominator as u128;
    if twice_remainder > denominator || (twice_remainder == denominator && quotient % 2 != 0) {
        quotient + numerator.signum()
    } else {
        quotient
    }
}

fn round_divide_i64(numerator: i64, denominator: i64) -> i64 {
    i64::try_from(round_divide_i128(
        i128::from(numerator),
        i128::from(denominator),
    ))
    .expect("i64 division result remains within i64")
}

fn encode_etopo_centre_summary(
    input_hash: Digest,
    header: EtopoCentreIndexHeader,
    summary_s2_level: u8,
    cells: &std::collections::BTreeMap<S2CellId, EtopoCentreSummaryStats>,
    source_samples: u64,
) -> Result<Vec<u8>> {
    let record_bytes = cells
        .len()
        .checked_mul(ETOPO_CENTRE_SUMMARY_RECORD_LENGTH)
        .context("ETOPO centre summary record bytes overflow")?;
    let total = ETOPO_CENTRE_SUMMARY_HEADER_LENGTH
        .checked_add(record_bytes)
        .context("ETOPO centre summary total bytes overflow")?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(ETOPO_CENTRE_SUMMARY_MAGIC);
    bytes.extend_from_slice(&ETOPO_CENTRE_SUMMARY_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&header.sample_arc_minutes.to_le_bytes());
    bytes.push(header.s2_level);
    bytes.push(summary_s2_level);
    bytes.extend_from_slice(&[0_u8; 2]);
    bytes.extend_from_slice(input_hash.as_bytes());
    bytes.extend_from_slice(header.snapshot_digest.as_bytes());
    bytes.extend_from_slice(header.artifact_digest.as_bytes());
    bytes.extend_from_slice(&u32::try_from(cells.len())?.to_le_bytes());
    bytes.extend_from_slice(&source_samples.to_le_bytes());
    for (cell, stats) in cells {
        bytes.extend_from_slice(&cell.get().to_be_bytes());
        bytes.extend_from_slice(&stats.samples.to_le_bytes());
        bytes.extend_from_slice(&stats.minimum_millimetres.to_le_bytes());
        bytes.extend_from_slice(&stats.mean_millimetres().to_le_bytes());
        bytes.extend_from_slice(&stats.maximum_millimetres.to_le_bytes());
    }
    debug_assert_eq!(bytes.len(), total);
    Ok(bytes)
}

fn write_new_artifact(output_path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = output_path
        .parent()
        .context("output path has no parent directory")?;
    let parent = parent
        .canonicalize()
        .with_context(|| format!("failed to resolve output directory {}", parent.display()))?;
    let file_name = output_path
        .file_name()
        .and_then(OsStr::to_str)
        .context("output filename is not UTF-8")?;
    let destination = parent.join(file_name);
    if fs::symlink_metadata(&destination).is_ok() {
        bail!("derived artifact {} already exists", destination.display());
    }
    let mut partial = PartialDownload::create(&parent, file_name)?;
    partial
        .file
        .write_all(bytes)
        .with_context(|| format!("failed to write {}", partial.path.display()))?;
    partial
        .file
        .sync_all()
        .with_context(|| format!("failed to sync {}", partial.path.display()))?;
    partial.persist_without_replacement(&destination)
}

#[derive(Serialize)]
struct EtopoInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    latitude_endpoint_ieee754_le_hex: [String; 2],
    longitude_endpoint_ieee754_le_hex: [String; 2],
    variables: Vec<EtopoVariableInspection>,
}

#[derive(Serialize)]
struct ChelsaAttributeInspection {
    name: String,
    string_value: Option<String>,
    first_numeric_value: Option<String>,
}

#[derive(Serialize)]
struct EtopoVariableInspection {
    name: String,
    shape: Vec<u64>,
}

const CHELSA_LATITUDE_CELLS: u64 = 20_880;
const CHELSA_LONGITUDE_CELLS: u64 = 43_200;

#[derive(Serialize)]
struct ChelsaJanuaryTemperatureInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    data_variable: String,
    data_shape: Vec<u64>,
    latitude_endpoint_ieee754_le_hex: [String; 2],
    longitude_endpoint_ieee754_le_hex: [String; 2],
    data_attributes: Vec<ChelsaAttributeInspection>,
    variables: Vec<EtopoVariableInspection>,
}

#[derive(Serialize)]
struct ChelsaAnnualTemperatureInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    monthly_normals: Vec<ChelsaMonthlyTemperatureInspection>,
    shared_latitude_endpoint_ieee754_le_hex: [String; 2],
    shared_longitude_endpoint_ieee754_le_hex: [String; 2],
    documented_raw_to_millicelsius: &'static str,
}

#[derive(Serialize)]
struct ChelsaMonthlyTemperatureInspection {
    month: u8,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
}

const ERA5_ARCHIVE_MEMBERS: [&str; 2] = [
    "data_stream-moda_stepType-avgua.nc",
    "data_stream-moda_stepType-avgad.nc",
];

#[derive(Serialize)]
struct Era5AnnualArchiveInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    year: u16,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    members: Vec<Era5MemberInspection>,
}

#[derive(Serialize)]
struct Era5MemberInspection {
    name: String,
    uncompressed_byte_length: u64,
    latitude_endpoint_ieee754_le_hex: [String; 2],
    longitude_endpoint_ieee754_le_hex: [String; 2],
    variables: Vec<EtopoVariableInspection>,
}

const ERA5_MONTHS_PER_YEAR: u64 = 12;
const ERA5_LATITUDE_CELLS: u64 = 721;
const ERA5_LONGITUDE_CELLS: u64 = 1_440;

fn expected_era5_member_variables(member_name: &str) -> Result<BTreeMap<&'static str, Vec<u64>>> {
    let spatial = vec![
        ERA5_MONTHS_PER_YEAR,
        ERA5_LATITUDE_CELLS,
        ERA5_LONGITUDE_CELLS,
    ];
    let mut expected = BTreeMap::from([
        ("expver", vec![ERA5_MONTHS_PER_YEAR]),
        ("latitude", vec![ERA5_LATITUDE_CELLS]),
        ("longitude", vec![ERA5_LONGITUDE_CELLS]),
        ("number", Vec::new()),
        ("valid_time", vec![ERA5_MONTHS_PER_YEAR]),
    ]);
    match member_name {
        "data_stream-moda_stepType-avgua.nc" => {
            for name in ["siconc", "sst", "t2m", "u10", "v10"] {
                expected.insert(name, spatial.clone());
            }
        }
        "data_stream-moda_stepType-avgad.nc" => {
            expected.insert("tp", spatial);
        }
        _ => bail!("unrecognized ERA5 archive member {member_name}"),
    }
    Ok(expected)
}

fn validate_era5_member_schema(file: &NcFile, member_name: &str) -> Result<()> {
    let expected = expected_era5_member_variables(member_name)?;
    let observed = file
        .variables()
        .with_context(|| format!("enumerate variables in ERA5 member {member_name}"))?
        .iter()
        .map(|variable| (variable.name(), variable.shape().to_vec()))
        .collect::<BTreeMap<_, _>>();
    if observed != expected {
        bail!("ERA5 member {member_name} variable schema changed");
    }
    Ok(())
}

/// Inspect one verified ERA5 archive in memory. ZIP member bytes never become a
/// source artifact or a durable intermediate: this is a parser-boundary probe before
/// climate semantics and spatial normalization are specified.
fn inspect_era5_annual_archive(
    manifest_path: &Path,
    artifact_root: &Path,
    year: u16,
) -> Result<()> {
    if !(1981..=2010).contains(&year) {
        bail!("ERA5 inspection year must be inside the pinned 1981-2010 normal period");
    }
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let expected_name = format!("-{:04}.zip", year);
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(&expected_name)
        })
        .with_context(|| format!("source snapshot has no ERA5 archive for {year}"))?;
    let archive_path = artifact_root.join(&artifact.artifact_path);
    let archive_file = File::open(&archive_path)
        .with_context(|| format!("open verified ERA5 archive {}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .context("open verified ERA5 ZIP archive through the portable reader")?;
    if archive.len() != ERA5_ARCHIVE_MEMBERS.len() {
        bail!("ERA5 ZIP archive has an unexpected member count");
    }
    let mut members = Vec::with_capacity(ERA5_ARCHIVE_MEMBERS.len());
    for member_name in ERA5_ARCHIVE_MEMBERS {
        let mut member = archive
            .by_name(member_name)
            .with_context(|| format!("ERA5 ZIP archive is missing member {member_name}"))?;
        let uncompressed_byte_length = member.size();
        if uncompressed_byte_length == 0 {
            bail!("ERA5 ZIP member {member_name} is empty");
        }
        let capacity = usize::try_from(uncompressed_byte_length)
            .context("ERA5 ZIP member is too large for this platform")?;
        let mut bytes = Vec::with_capacity(capacity);
        member
            .read_to_end(&mut bytes)
            .with_context(|| format!("read ERA5 ZIP member {member_name}"))?;
        if u64::try_from(bytes.len())? != uncompressed_byte_length {
            bail!("ERA5 ZIP member {member_name} did not yield its declared byte length");
        }
        let file = NcFile::from_bytes(&bytes)
            .with_context(|| format!("parse ERA5 ZIP member {member_name} as NetCDF"))?;
        validate_era5_member_schema(&file, member_name)?;
        let latitude_endpoint_ieee754_le_hex =
            inspect_etopo_axis_endpoints(&file, "latitude", ERA5_LATITUDE_CELLS, "ERA5 latitude")?;
        let longitude_endpoint_ieee754_le_hex = inspect_etopo_axis_endpoints(
            &file,
            "longitude",
            ERA5_LONGITUDE_CELLS,
            "ERA5 longitude",
        )?;
        let mut variables = file
            .variables()
            .with_context(|| format!("enumerate variables in ERA5 member {member_name}"))?
            .iter()
            .map(|variable| EtopoVariableInspection {
                name: variable.name().to_owned(),
                shape: variable.shape().to_vec(),
            })
            .collect::<Vec<_>>();
        variables.sort_by(|left, right| left.name.cmp(&right.name));
        members.push(Era5MemberInspection {
            name: member_name.to_owned(),
            uncompressed_byte_length,
            latitude_endpoint_ieee754_le_hex,
            longitude_endpoint_ieee754_le_hex,
            variables,
        });
    }
    println!(
        "{}",
        serde_json::to_string(&Era5AnnualArchiveInspection {
            inspection_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            year,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            artifact_byte_length: artifact.byte_length,
            members,
        })?
    );
    Ok(())
}

const COPERNICUS_LAND_COVER_MEMBER: &str = "C3S-LC-L4-LCCS-Map-300m-P1Y-2022-v2.1.1.nc";
const COPERNICUS_LAND_COVER_SNAPSHOT_ID: &str = "copernicus-satellite-land-cover-v2-1-1-2022";
const COPERNICUS_LAND_COVER_ARTIFACT_PATH: &str =
    "copernicus-land-cover-2022/copernicus-satellite-land-cover-v2.1.1-2022.zip";
const COPERNICUS_LAND_COVER_ARTIFACT_HASH: &str =
    "993500e18307b5ea0811394355199937b8305081d08b6a7f6909d73a3eadbac7";
const COPERNICUS_LAND_COVER_ARTIFACT_BYTES: u64 = 2_352_123_142;
const COPERNICUS_LAND_COVER_MEMBER_BYTES: u64 = 2_351_763_989;
const COPERNICUS_LAND_COVER_MEMBER_CRC32: u32 = 3_844_043_699;
const COPERNICUS_LAND_COVER_MEMBER_HASH: &str =
    "38149d655e27c0d353dac61eb8e5997cf951566cb52071a3fc4a63b260063e42";
const COPERNICUS_LAND_COVER_LATITUDE_CELLS: u64 = 64_800;
const COPERNICUS_LAND_COVER_LONGITUDE_CELLS: u64 = 129_600;
const COPERNICUS_LAND_COVER_CHUNK_CELLS: u64 = 2_025;
const COPERNICUS_LAND_COVER_LATITUDE_ENDPOINT_BITS: [u64; 2] =
    [0x4056_7fe9_3e93_e940, 0xc056_7fe9_3e93_e93f];
const COPERNICUS_LAND_COVER_LONGITUDE_ENDPOINT_BITS: [u64; 2] =
    [0xc066_7ff4_9f49_f49f, 0x4066_7ff4_9f49_f4a0];

#[derive(Serialize)]
struct CopernicusLandCoverInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    archive_member: String,
    archive_member_byte_length: u64,
    archive_member_crc32: u32,
    archive_member_hash: Digest,
    latitude_endpoint_ieee754_bits_hex: [String; 2],
    longitude_endpoint_ieee754_bits_hex: [String; 2],
    global_attributes: BTreeMap<String, String>,
    variables: Vec<LandCoverVariableInspection>,
}

#[derive(Serialize)]
struct LandCoverVariableInspection {
    name: String,
    data_type: String,
    shape: Vec<u64>,
}

struct VerifiedCopernicusLandCover {
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    archive_member_crc32: u32,
    archive_member_hash: Digest,
    latitude_endpoint_bits: [u64; 2],
    longitude_endpoint_bits: [u64; 2],
    global_attributes: BTreeMap<String, String>,
    file: NcFile,
    _extracted: PartialDownload,
}

#[derive(Serialize)]
struct CopernicusLandCoverCensus {
    census_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    archive_member_hash: Digest,
    latitude_cells: u64,
    longitude_cells: u64,
    raster_cells: u64,
    source_chunk_shape: [u64; 2],
    chunks_scanned: u64,
    lccs_classes: Vec<LccsClassCensus>,
    processed_flag_counts: Vec<RasterValueCount>,
    current_pixel_state_counts: Vec<RasterValueCount>,
    observation_count_counts: Vec<RasterValueCount>,
    change_count_counts: Vec<RasterValueCount>,
}

#[derive(Serialize)]
struct LccsClassCensus {
    value: u8,
    meaning: &'static str,
    cells: u64,
}

#[derive(Serialize)]
struct RasterValueCount {
    value: i64,
    cells: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
struct CopernicusLandCoverSourceSample {
    latitude_e7: i32,
    longitude_e7: i32,
    source_row: u32,
    source_column: u32,
}

#[derive(Serialize)]
struct CopernicusLandCoverTargetSupportInspection {
    inspection_schema_version: u16,
    target_s2_cell_id: S2CellId,
    target_s2_level: u8,
    sample_policy: String,
    points_per_axis: u8,
    support_samples: u64,
    distinct_source_cells: u64,
    sample_fingerprint: Digest,
    first_sample: CopernicusLandCoverSourceSample,
    last_sample: CopernicusLandCoverSourceSample,
}

fn expected_copernicus_land_cover_variables() -> BTreeMap<&'static str, (NcType, Vec<u64>)> {
    let raster = vec![
        1,
        COPERNICUS_LAND_COVER_LATITUDE_CELLS,
        COPERNICUS_LAND_COVER_LONGITUDE_CELLS,
    ];
    BTreeMap::from([
        ("change_count", (NcType::UByte, raster.clone())),
        ("crs", (NcType::Int, Vec::new())),
        ("current_pixel_state", (NcType::Byte, raster.clone())),
        (
            "lat",
            (NcType::Double, vec![COPERNICUS_LAND_COVER_LATITUDE_CELLS]),
        ),
        (
            "lat_bounds",
            (
                NcType::Double,
                vec![COPERNICUS_LAND_COVER_LATITUDE_CELLS, 2],
            ),
        ),
        ("lccs_class", (NcType::UByte, raster.clone())),
        (
            "lon",
            (NcType::Double, vec![COPERNICUS_LAND_COVER_LONGITUDE_CELLS]),
        ),
        (
            "lon_bounds",
            (
                NcType::Double,
                vec![COPERNICUS_LAND_COVER_LONGITUDE_CELLS, 2],
            ),
        ),
        ("observation_count", (NcType::UShort, raster.clone())),
        ("processed_flag", (NcType::Byte, raster)),
        ("time", (NcType::Double, vec![1])),
        ("time_bounds", (NcType::Double, vec![1, 2])),
    ])
}

fn expected_copernicus_land_cover_global_attributes() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("id", "C3S-LC-L4-LCCS-Map-300m-P1Y-2022-v2.1.1"),
        ("license", "EC C3S Land cover Data Policy"),
        ("product_version", "2.1.1"),
        ("source", "Sentinel-3 OLCI"),
        ("spatial_resolution", "300m"),
        ("time_coverage_end", "20221231"),
        ("time_coverage_start", "20220101"),
        ("tracking_id", "cbc0983e-a0fd-4277-9023-2e618c0c2067"),
    ])
}

fn required_variable_attribute<'a>(
    file: &'a NcFile,
    variable_name: &str,
    attribute_name: &str,
) -> Result<&'a NcAttrValue> {
    file.variable(variable_name)
        .with_context(|| format!("find Copernicus land-cover variable {variable_name}"))?
        .attribute(attribute_name)
        .with_context(|| {
            format!(
                "Copernicus land-cover variable {variable_name} is missing attribute {attribute_name}"
            )
        })
        .map(|attribute| &attribute.value)
}

fn validate_copernicus_land_cover_value_semantics(file: &NcFile) -> Result<()> {
    let expected_class_values = COPERNICUS_LCCS_CLASSES
        .iter()
        .map(|(value, _)| *value)
        .collect::<Vec<_>>();
    let expected_class_meanings = COPERNICUS_LCCS_CLASSES
        .iter()
        .map(|(_, meaning)| *meaning)
        .collect::<Vec<_>>()
        .join(" ");
    let expectations = [
        (
            "lccs_class",
            "flag_values",
            NcAttrValue::UBytes(expected_class_values),
        ),
        (
            "lccs_class",
            "flag_meanings",
            NcAttrValue::Strings(vec![expected_class_meanings]),
        ),
        (
            "lccs_class",
            "ancillary_variables",
            NcAttrValue::Strings(vec![
                "processed_flag current_pixel_state observation_count change_count".to_owned(),
            ]),
        ),
        (
            "processed_flag",
            "flag_values",
            NcAttrValue::Bytes(vec![0, 1]),
        ),
        (
            "processed_flag",
            "flag_meanings",
            NcAttrValue::Strings(vec!["not_processed processed".to_owned()]),
        ),
        ("processed_flag", "_FillValue", NcAttrValue::Bytes(vec![-1])),
        (
            "current_pixel_state",
            "flag_values",
            NcAttrValue::Bytes(vec![0, 1, 2, 3, 4, 5]),
        ),
        (
            "current_pixel_state",
            "flag_meanings",
            NcAttrValue::Strings(vec![
                "invalid clear_land clear_water clear_snow_ice cloud cloud_shadow".to_owned(),
            ]),
        ),
        (
            "current_pixel_state",
            "_FillValue",
            NcAttrValue::Bytes(vec![-1]),
        ),
    ];
    for (variable_name, attribute_name, expected) in expectations {
        let observed = required_variable_attribute(file, variable_name, attribute_name)?;
        if observed != &expected {
            bail!(
                "Copernicus land-cover {variable_name}:{attribute_name} changed: expected {expected:?}, observed {observed:?}"
            );
        }
    }
    Ok(())
}

fn validate_copernicus_land_cover_schema(file: &NcFile) -> Result<()> {
    let expected = expected_copernicus_land_cover_variables();
    let observed = file
        .variables()
        .context("enumerate Copernicus land-cover variables")?
        .iter()
        .map(|variable| {
            (
                variable.name(),
                (variable.dtype().clone(), variable.shape().to_vec()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if observed != expected {
        bail!(
            "Copernicus land-cover variable schema changed: expected {expected:?}, observed {observed:?}"
        );
    }
    Ok(())
}

fn netcdf_type_name(value: &NcType) -> Result<&'static str> {
    match value {
        NcType::Byte => Ok("i8"),
        NcType::Char => Ok("char"),
        NcType::Short => Ok("i16"),
        NcType::Int => Ok("i32"),
        NcType::Float => Ok("f32"),
        NcType::Double => Ok("f64"),
        NcType::UByte => Ok("u8"),
        NcType::UShort => Ok("u16"),
        NcType::UInt => Ok("u32"),
        NcType::Int64 => Ok("i64"),
        NcType::UInt64 => Ok("u64"),
        NcType::String => Ok("string"),
        _ => bail!("land-cover schema contains an unsupported compound NetCDF type"),
    }
}

fn required_global_attribute(file: &NcFile, name: &str) -> Result<String> {
    file.global_attributes()
        .context("enumerate Copernicus land-cover global attributes")?
        .iter()
        .find(|attribute| attribute.name == name)
        .with_context(|| format!("Copernicus land-cover file is missing global attribute {name}"))?
        .value
        .as_string()
        .with_context(|| format!("Copernicus land-cover global attribute {name} is not text"))
}

fn inspect_land_cover_axis_endpoints(
    file: &NcFile,
    variable_name: &str,
    expected_length: u64,
) -> Result<[u64; 2]> {
    let values = file
        .read_variable::<f64>(variable_name)
        .with_context(|| format!("read Copernicus land-cover {variable_name} coordinate axis"))?;
    let values = values.as_slice().with_context(|| {
        format!("Copernicus land-cover {variable_name} coordinate axis is not contiguous")
    })?;
    if values.len() != usize::try_from(expected_length)? {
        bail!("Copernicus land-cover {variable_name} axis has an unexpected length");
    }
    let first = values
        .first()
        .context("Copernicus land-cover coordinate axis is empty")?;
    let last = values
        .last()
        .context("Copernicus land-cover coordinate axis is empty")?;
    Ok([first.to_bits(), last.to_bits()])
}

fn open_verified_copernicus_land_cover(
    manifest_path: &Path,
    artifact_root: &Path,
) -> Result<VerifiedCopernicusLandCover> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    if snapshot.snapshot_id != COPERNICUS_LAND_COVER_SNAPSHOT_ID {
        bail!(
            "expected Copernicus land-cover snapshot {COPERNICUS_LAND_COVER_SNAPSHOT_ID}, observed {}",
            snapshot.snapshot_id
        );
    }
    let source_snapshot_digest = snapshot.content_digest()?;
    let mut data_artifacts = snapshot.artifacts.iter().filter(|artifact| {
        artifact.role == world_data::SourceSnapshotArtifactRole::Data
            && artifact.artifact_path.ends_with(".zip")
    });
    let artifact = data_artifacts
        .next()
        .context("source snapshot has no Copernicus land-cover ZIP response")?;
    if data_artifacts.next().is_some() {
        bail!("Copernicus land-cover source snapshot has multiple data responses");
    }
    let expected_artifact_hash = COPERNICUS_LAND_COVER_ARTIFACT_HASH
        .parse::<Digest>()
        .context("parse pinned Copernicus land-cover artifact digest")?;
    if artifact.artifact_path != COPERNICUS_LAND_COVER_ARTIFACT_PATH
        || artifact.content_hash != expected_artifact_hash
        || artifact.byte_length != COPERNICUS_LAND_COVER_ARTIFACT_BYTES
    {
        bail!("Copernicus land-cover data artifact identity changed");
    }
    let archive_path = artifact_root.join(&artifact.artifact_path);
    let archive_file = File::open(&archive_path).with_context(|| {
        format!(
            "open verified Copernicus land-cover archive {}",
            archive_path.display()
        )
    })?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .context("open verified Copernicus land-cover ZIP archive")?;
    if archive.len() != 1 {
        bail!("Copernicus land-cover ZIP archive has an unexpected member count");
    }
    let mut member = archive
        .by_index(0)
        .context("open Copernicus land-cover ZIP member")?;
    if member.name() != COPERNICUS_LAND_COVER_MEMBER {
        bail!("Copernicus land-cover ZIP member name changed");
    }
    if member.size() != COPERNICUS_LAND_COVER_MEMBER_BYTES {
        bail!("Copernicus land-cover ZIP member byte length changed");
    }
    let archive_member_crc32 = member.crc32();
    if archive_member_crc32 != COPERNICUS_LAND_COVER_MEMBER_CRC32 {
        bail!("Copernicus land-cover ZIP member CRC-32 changed");
    }
    let mut extracted = PartialDownload::create(
        &std::env::temp_dir(),
        "a-tiny-civilization-copernicus-land-cover-inspection.nc",
    )?;
    let mut member_hasher = Sha256::new();
    let mut member_bytes = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = member
            .read(&mut buffer)
            .context("read Copernicus land-cover ZIP member")?;
        if read == 0 {
            break;
        }
        member_bytes = member_bytes
            .checked_add(u64::try_from(read)?)
            .context("Copernicus land-cover member length overflow")?;
        member_hasher.update(&buffer[..read]);
        extracted
            .file
            .write_all(&buffer[..read])
            .context("write temporary Copernicus land-cover NetCDF")?;
    }
    if member_bytes != COPERNICUS_LAND_COVER_MEMBER_BYTES {
        bail!("Copernicus land-cover ZIP member yielded an unexpected byte length");
    }
    extracted
        .file
        .sync_all()
        .context("sync temporary Copernicus land-cover NetCDF")?;
    let archive_member_hash = Digest::from_bytes(member_hasher.finalize().into());
    let expected_member_hash = COPERNICUS_LAND_COVER_MEMBER_HASH
        .parse::<Digest>()
        .context("parse pinned Copernicus land-cover member digest")?;
    if archive_member_hash != expected_member_hash {
        bail!("Copernicus land-cover ZIP member SHA-256 changed");
    }
    drop(member);
    drop(archive);

    let file = NcFile::open(&extracted.path)
        .context("parse Copernicus land-cover NetCDF through the pure-Rust reader")?;
    validate_copernicus_land_cover_schema(&file)?;
    validate_copernicus_land_cover_value_semantics(&file)?;
    let latitude_endpoint_bits =
        inspect_land_cover_axis_endpoints(&file, "lat", COPERNICUS_LAND_COVER_LATITUDE_CELLS)?;
    if latitude_endpoint_bits != COPERNICUS_LAND_COVER_LATITUDE_ENDPOINT_BITS {
        bail!("Copernicus land-cover latitude endpoints changed");
    }
    let longitude_endpoint_bits =
        inspect_land_cover_axis_endpoints(&file, "lon", COPERNICUS_LAND_COVER_LONGITUDE_CELLS)?;
    if longitude_endpoint_bits != COPERNICUS_LAND_COVER_LONGITUDE_ENDPOINT_BITS {
        bail!("Copernicus land-cover longitude endpoints changed");
    }
    let expected_global_attributes = expected_copernicus_land_cover_global_attributes();
    let global_attributes = expected_global_attributes
        .keys()
        .map(|name| Ok(((*name).to_owned(), required_global_attribute(&file, name)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let observed_global_attributes = global_attributes
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<BTreeMap<_, _>>();
    if observed_global_attributes != expected_global_attributes {
        bail!(
            "Copernicus land-cover global attributes changed: expected {expected_global_attributes:?}, observed {observed_global_attributes:?}"
        );
    }
    Ok(VerifiedCopernicusLandCover {
        source_snapshot_id: snapshot.snapshot_id,
        source_snapshot_digest,
        artifact_path: artifact.artifact_path.clone(),
        artifact_hash: artifact.content_hash,
        artifact_byte_length: artifact.byte_length,
        archive_member_crc32,
        archive_member_hash,
        latitude_endpoint_bits,
        longitude_endpoint_bits,
        global_attributes,
        file,
        _extracted: extracted,
    })
}

fn inspect_copernicus_land_cover(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let source = open_verified_copernicus_land_cover(manifest_path, artifact_root)?;
    let mut variables = source
        .file
        .variables()
        .context("enumerate Copernicus land-cover variables")?
        .iter()
        .map(|variable| {
            Ok(LandCoverVariableInspection {
                name: variable.name().to_owned(),
                data_type: netcdf_type_name(variable.dtype())?.to_owned(),
                shape: variable.shape().to_vec(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    variables.sort_by(|left, right| left.name.cmp(&right.name));
    println!(
        "{}",
        serde_json::to_string(&CopernicusLandCoverInspection {
            inspection_schema_version: 1,
            source_snapshot_id: source.source_snapshot_id.clone(),
            source_snapshot_digest: source.source_snapshot_digest,
            artifact_path: source.artifact_path.clone(),
            artifact_hash: source.artifact_hash,
            artifact_byte_length: source.artifact_byte_length,
            archive_member: COPERNICUS_LAND_COVER_MEMBER.to_owned(),
            archive_member_byte_length: COPERNICUS_LAND_COVER_MEMBER_BYTES,
            archive_member_crc32: source.archive_member_crc32,
            archive_member_hash: source.archive_member_hash,
            latitude_endpoint_ieee754_bits_hex: source
                .latitude_endpoint_bits
                .map(|value| format!("{value:016x}")),
            longitude_endpoint_ieee754_bits_hex: source
                .longitude_endpoint_bits
                .map(|value| format!("{value:016x}")),
            global_attributes: source.global_attributes.clone(),
            variables,
        })?
    );
    Ok(())
}

fn copernicus_land_cover_chunk_selection(chunk_row: u64, chunk_column: u64) -> NcSliceInfo {
    let latitude_start = chunk_row * COPERNICUS_LAND_COVER_CHUNK_CELLS;
    let longitude_start = chunk_column * COPERNICUS_LAND_COVER_CHUNK_CELLS;
    NcSliceInfo {
        selections: vec![
            NcSliceInfoElem::Index(0),
            NcSliceInfoElem::Slice {
                start: latitude_start,
                end: latitude_start + COPERNICUS_LAND_COVER_CHUNK_CELLS,
                step: 1,
            },
            NcSliceInfoElem::Slice {
                start: longitude_start,
                end: longitude_start + COPERNICUS_LAND_COVER_CHUNK_CELLS,
                step: 1,
            },
        ],
    }
}

fn accumulate_u8_counts(values: &[u8], counts: &mut [u64; 256]) {
    for value in values {
        counts[usize::from(*value)] += 1;
    }
}

fn accumulate_i8_counts(values: &[i8], counts: &mut [u64; 256]) {
    for value in values {
        let index = usize::try_from(i16::from(*value) + 128).expect("i8 offset fits usize");
        counts[index] += 1;
    }
}

fn accumulate_u16_counts(values: &[u16], counts: &mut [u64]) {
    debug_assert_eq!(counts.len(), usize::from(u16::MAX) + 1);
    for value in values {
        counts[usize::from(*value)] += 1;
    }
}

fn raster_value_counts_u8(counts: &[u64; 256]) -> Vec<RasterValueCount> {
    counts
        .iter()
        .enumerate()
        .filter(|(_, cells)| **cells != 0)
        .map(|(value, cells)| RasterValueCount {
            value: i64::try_from(value).expect("u8 index fits i64"),
            cells: *cells,
        })
        .collect()
}

fn raster_value_counts_i8(counts: &[u64; 256]) -> Vec<RasterValueCount> {
    counts
        .iter()
        .enumerate()
        .filter(|(_, cells)| **cells != 0)
        .map(|(index, cells)| RasterValueCount {
            value: i64::try_from(index).expect("i8 count index fits i64") - 128,
            cells: *cells,
        })
        .collect()
}

fn raster_value_counts_u16(counts: &[u64]) -> Vec<RasterValueCount> {
    counts
        .iter()
        .enumerate()
        .filter(|(_, cells)| **cells != 0)
        .map(|(value, cells)| RasterValueCount {
            value: i64::try_from(value).expect("u16 index fits i64"),
            cells: *cells,
        })
        .collect()
}

fn require_census_total(label: &str, counts: &[u64], expected: u64) -> Result<()> {
    let observed = counts.iter().try_fold(0_u64, |total, count| {
        total.checked_add(*count).context("census total overflow")
    })?;
    if observed != expected {
        bail!("{label} census covered {observed} cells instead of {expected}");
    }
    Ok(())
}

fn inspect_copernicus_land_cover_census(
    manifest_path: &Path,
    artifact_root: &Path,
    output_path: Option<&Path>,
) -> Result<()> {
    if !COPERNICUS_LAND_COVER_LATITUDE_CELLS.is_multiple_of(COPERNICUS_LAND_COVER_CHUNK_CELLS)
        || !COPERNICUS_LAND_COVER_LONGITUDE_CELLS.is_multiple_of(COPERNICUS_LAND_COVER_CHUNK_CELLS)
    {
        bail!("pinned Copernicus land-cover grid is not divisible by its source chunk shape");
    }
    let source = open_verified_copernicus_land_cover(manifest_path, artifact_root)?;
    let raster_cells = COPERNICUS_LAND_COVER_LATITUDE_CELLS
        .checked_mul(COPERNICUS_LAND_COVER_LONGITUDE_CELLS)
        .context("Copernicus land-cover raster cell count overflow")?;
    let chunk_rows = COPERNICUS_LAND_COVER_LATITUDE_CELLS / COPERNICUS_LAND_COVER_CHUNK_CELLS;
    let chunk_columns = COPERNICUS_LAND_COVER_LONGITUDE_CELLS / COPERNICUS_LAND_COVER_CHUNK_CELLS;
    let chunks_scanned = chunk_rows
        .checked_mul(chunk_columns)
        .context("Copernicus land-cover chunk count overflow")?;
    let expected_chunk_cells =
        usize::try_from(COPERNICUS_LAND_COVER_CHUNK_CELLS * COPERNICUS_LAND_COVER_CHUNK_CELLS)?;
    let mut lccs_counts = [0_u64; 256];
    let mut processed_counts = [0_u64; 256];
    let mut state_counts = [0_u64; 256];
    let mut observation_counts = vec![0_u64; usize::from(u16::MAX) + 1];
    let mut change_counts = [0_u64; 256];

    for chunk_row in 0..chunk_rows {
        for chunk_column in 0..chunk_columns {
            let selection = copernicus_land_cover_chunk_selection(chunk_row, chunk_column);
            let classes = source
                .file
                .read_variable_slice::<u8>("lccs_class", &selection)
                .context("read Copernicus lccs_class source chunk")?;
            let processed = source
                .file
                .read_variable_slice::<i8>("processed_flag", &selection)
                .context("read Copernicus processed_flag source chunk")?;
            let states = source
                .file
                .read_variable_slice::<i8>("current_pixel_state", &selection)
                .context("read Copernicus current_pixel_state source chunk")?;
            let observations = source
                .file
                .read_variable_slice::<u16>("observation_count", &selection)
                .context("read Copernicus observation_count source chunk")?;
            let changes = source
                .file
                .read_variable_slice::<u8>("change_count", &selection)
                .context("read Copernicus change_count source chunk")?;
            let classes = classes
                .as_slice()
                .context("Copernicus lccs_class chunk is not contiguous")?;
            let processed = processed
                .as_slice()
                .context("Copernicus processed_flag chunk is not contiguous")?;
            let states = states
                .as_slice()
                .context("Copernicus current_pixel_state chunk is not contiguous")?;
            let observations = observations
                .as_slice()
                .context("Copernicus observation_count chunk is not contiguous")?;
            let changes = changes
                .as_slice()
                .context("Copernicus change_count chunk is not contiguous")?;
            for (name, length) in [
                ("lccs_class", classes.len()),
                ("processed_flag", processed.len()),
                ("current_pixel_state", states.len()),
                ("observation_count", observations.len()),
                ("change_count", changes.len()),
            ] {
                if length != expected_chunk_cells {
                    bail!(
                        "Copernicus {name} chunk yielded {length} cells instead of {expected_chunk_cells}"
                    );
                }
            }
            accumulate_u8_counts(classes, &mut lccs_counts);
            accumulate_i8_counts(processed, &mut processed_counts);
            accumulate_i8_counts(states, &mut state_counts);
            accumulate_u16_counts(observations, &mut observation_counts);
            accumulate_u8_counts(changes, &mut change_counts);
        }
        eprintln!(
            "censused Copernicus land-cover source chunk row {}/{}",
            chunk_row + 1,
            chunk_rows
        );
    }

    require_census_total("lccs_class", &lccs_counts, raster_cells)?;
    require_census_total("processed_flag", &processed_counts, raster_cells)?;
    require_census_total("current_pixel_state", &state_counts, raster_cells)?;
    require_census_total("observation_count", &observation_counts, raster_cells)?;
    require_census_total("change_count", &change_counts, raster_cells)?;
    let allowed_classes = COPERNICUS_LCCS_CLASSES
        .iter()
        .map(|(value, _)| *value)
        .collect::<std::collections::BTreeSet<_>>();
    for (value, cells) in lccs_counts.iter().enumerate() {
        if *cells != 0 && !allowed_classes.contains(&u8::try_from(value)?) {
            bail!("Copernicus lccs_class contains unsupported value {value}");
        }
    }
    for value in -128_i16..=127_i16 {
        let cells = processed_counts[usize::try_from(value + 128)?];
        if cells != 0 && ![-1_i16, 0, 1].contains(&value) {
            bail!("Copernicus processed_flag contains unsupported value {value}");
        }
        let cells = state_counts[usize::try_from(value + 128)?];
        if cells != 0 && ![-1_i16, 0, 1, 2, 3, 4, 5].contains(&value) {
            bail!("Copernicus current_pixel_state contains unsupported value {value}");
        }
    }
    if let Some((value, _)) = observation_counts
        .iter()
        .enumerate()
        .find(|(value, cells)| **cells != 0 && *value > 32_767)
    {
        bail!("Copernicus observation_count contains unsupported value {value}");
    }
    if let Some((value, _)) = change_counts
        .iter()
        .enumerate()
        .find(|(value, cells)| **cells != 0 && *value > 100)
    {
        bail!("Copernicus change_count contains unsupported value {value}");
    }

    let census = CopernicusLandCoverCensus {
        census_schema_version: 1,
        source_snapshot_id: source.source_snapshot_id.clone(),
        source_snapshot_digest: source.source_snapshot_digest,
        archive_member_hash: source.archive_member_hash,
        latitude_cells: COPERNICUS_LAND_COVER_LATITUDE_CELLS,
        longitude_cells: COPERNICUS_LAND_COVER_LONGITUDE_CELLS,
        raster_cells,
        source_chunk_shape: [
            COPERNICUS_LAND_COVER_CHUNK_CELLS,
            COPERNICUS_LAND_COVER_CHUNK_CELLS,
        ],
        chunks_scanned,
        lccs_classes: COPERNICUS_LCCS_CLASSES
            .iter()
            .map(|(value, meaning)| LccsClassCensus {
                value: *value,
                meaning,
                cells: lccs_counts[usize::from(*value)],
            })
            .collect(),
        processed_flag_counts: raster_value_counts_i8(&processed_counts),
        current_pixel_state_counts: raster_value_counts_i8(&state_counts),
        observation_count_counts: raster_value_counts_u16(&observation_counts),
        change_count_counts: raster_value_counts_u8(&change_counts),
    };
    if let Some(output_path) = output_path {
        let mut pretty = serde_json::to_vec_pretty(&census)?;
        pretty.push(b'\n');
        write_new_artifact(output_path, &pretty)?;
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "census_schema_version": 1,
                "output_path": output_path,
                "output_hash": Digest::sha256(&pretty),
                "raster_cells": raster_cells,
                "chunks_scanned": chunks_scanned,
            }))?
        );
    } else {
        println!("{}", serde_json::to_string(&census)?);
    }
    Ok(())
}

fn interpolate_s2_face_uv_midpoint(
    lower: S2FaceUv,
    upper: S2FaceUv,
    index: u8,
    points_per_axis: u8,
    use_u_axis: bool,
) -> Result<(i128, i128)> {
    if points_per_axis == 0 || points_per_axis > 64 || index >= points_per_axis {
        bail!("Copernicus target-support quadrature must use 1..=64 points per axis");
    }
    if lower.face != upper.face || lower.denominator != upper.denominator {
        bail!("S2 target-support vertices do not share one face projection");
    }
    let high_weight = i128::from(index)
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .context("target-support midpoint weight overflow")?;
    let denominator_weight = i128::from(points_per_axis)
        .checked_mul(2)
        .context("target-support denominator weight overflow")?;
    let low_weight = denominator_weight
        .checked_sub(high_weight)
        .context("target-support low weight underflow")?;
    let (low, high) = if use_u_axis {
        (lower.u_numerator, upper.u_numerator)
    } else {
        (lower.v_numerator, upper.v_numerator)
    };
    let numerator = low
        .checked_mul(low_weight)
        .and_then(|value| {
            high.checked_mul(high_weight)
                .and_then(|high| value.checked_add(high))
        })
        .context("target-support interpolation overflow")?;
    let denominator = lower
        .denominator
        .checked_mul(denominator_weight)
        .context("target-support denominator overflow")?;
    Ok((numerator, denominator))
}

fn copernicus_source_area_cell(coordinate: GeographicCoordinateE7) -> Result<(u32, u32)> {
    const E7_PER_DEGREE: i64 = 10_000_000;
    const SOURCE_CELLS_PER_DEGREE: i64 = 360;
    let latitude_distance_from_north = 900_000_000_i64
        .checked_sub(i64::from(coordinate.latitude_e7()))
        .context("Copernicus latitude lookup overflow")?;
    let mut row = latitude_distance_from_north
        .checked_mul(SOURCE_CELLS_PER_DEGREE)
        .context("Copernicus latitude lookup overflow")?
        / E7_PER_DEGREE;
    if row == i64::try_from(COPERNICUS_LAND_COVER_LATITUDE_CELLS)? {
        row -= 1;
    }
    let longitude_distance_from_west = i64::from(coordinate.longitude_e7())
        .checked_add(1_800_000_000)
        .context("Copernicus longitude lookup overflow")?;
    let column = longitude_distance_from_west
        .checked_mul(SOURCE_CELLS_PER_DEGREE)
        .context("Copernicus longitude lookup overflow")?
        / E7_PER_DEGREE;
    if row < 0
        || row >= i64::try_from(COPERNICUS_LAND_COVER_LATITUDE_CELLS)?
        || column < 0
        || column >= i64::try_from(COPERNICUS_LAND_COVER_LONGITUDE_CELLS)?
    {
        bail!("geographic coordinate does not select a Copernicus source area cell");
    }
    Ok((u32::try_from(row)?, u32::try_from(column)?))
}

fn copernicus_land_cover_target_support_samples(
    target: S2CellId,
    points_per_axis: u8,
) -> Result<Vec<CopernicusLandCoverSourceSample>> {
    if points_per_axis == 0 || points_per_axis > 64 {
        bail!("Copernicus target-support quadrature must use 1..=64 points per axis");
    }
    let ij = decode_s2_face_ij(target);
    let upper_i = ij.i.checked_add(1).context("S2 target i overflow")?;
    let upper_j = ij.j.checked_add(1).context("S2 target j overflow")?;
    let lower = s2_face_ij_vertex_uv(ij, ij.i, ij.j)?;
    let upper = s2_face_ij_vertex_uv(ij, upper_i, upper_j)?;
    let sample_count = usize::from(points_per_axis)
        .checked_mul(usize::from(points_per_axis))
        .context("Copernicus target-support sample count overflow")?;
    let mut samples = Vec::with_capacity(sample_count);
    for v_index in 0..points_per_axis {
        let (v_numerator, denominator) =
            interpolate_s2_face_uv_midpoint(lower, upper, v_index, points_per_axis, false)?;
        for u_index in 0..points_per_axis {
            let (u_numerator, u_denominator) =
                interpolate_s2_face_uv_midpoint(lower, upper, u_index, points_per_axis, true)?;
            if u_denominator != denominator {
                bail!("target-support axes produced different denominators");
            }
            let coordinate = s2_ray_to_geographic_e7(s2_face_uv_to_ray(S2FaceUv {
                face: ij.face,
                u_numerator,
                v_numerator,
                denominator,
            })?)?;
            let (source_row, source_column) = copernicus_source_area_cell(coordinate)?;
            samples.push(CopernicusLandCoverSourceSample {
                latitude_e7: coordinate.latitude_e7(),
                longitude_e7: coordinate.longitude_e7(),
                source_row,
                source_column,
            });
        }
    }
    Ok(samples)
}

fn copernicus_target_support_fingerprint(samples: &[CopernicusLandCoverSourceSample]) -> Digest {
    let mut bytes = Vec::with_capacity(samples.len() * 16);
    for sample in samples {
        bytes.extend_from_slice(&sample.latitude_e7.to_le_bytes());
        bytes.extend_from_slice(&sample.longitude_e7.to_le_bytes());
        bytes.extend_from_slice(&sample.source_row.to_le_bytes());
        bytes.extend_from_slice(&sample.source_column.to_le_bytes());
    }
    Digest::sha256(&bytes)
}

fn inspect_copernicus_land_cover_target_support(
    target: S2CellId,
    points_per_axis: u8,
) -> Result<()> {
    let samples = copernicus_land_cover_target_support_samples(target, points_per_axis)?;
    let distinct_source_cells = samples
        .iter()
        .map(|sample| (sample.source_row, sample.source_column))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    println!(
        "{}",
        serde_json::to_string(&CopernicusLandCoverTargetSupportInspection {
            inspection_schema_version: 1,
            target_s2_cell_id: target,
            target_s2_level: target.level(),
            sample_policy: format!("s2-face-uv-q{points_per_axis}-e7-source-area-v1"),
            points_per_axis,
            support_samples: u64::try_from(samples.len())?,
            distinct_source_cells: u64::try_from(distinct_source_cells)?,
            sample_fingerprint: copernicus_target_support_fingerprint(&samples),
            first_sample: *samples.first().context("target support is empty")?,
            last_sample: *samples.last().context("target support is empty")?,
        })?
    );
    Ok(())
}

#[derive(Serialize)]
struct ChelsaJanuaryCellInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    row: u64,
    column: u64,
    latitude_ieee754_le_hex: String,
    longitude_ieee754_le_hex: String,
    raw_band1_ieee754_le_hex: String,
    documented_temperature_millicelsius: i64,
}

#[derive(Serialize)]
struct ChelsaNearestCellInspection {
    inspection_schema_version: u16,
    source_snapshot_digest: Digest,
    requested_latitude_e7: i32,
    requested_longitude_e7: i32,
    row: u64,
    column: u64,
    source_latitude_e7: i32,
    source_longitude_e7: i32,
}

#[derive(Serialize)]
struct ChelsaAnnualCellInspection {
    inspection_schema_version: u16,
    source_snapshot_digest: Digest,
    requested_latitude_e7: i32,
    requested_longitude_e7: i32,
    row: u64,
    column: u64,
    source_latitude_e7: i32,
    source_longitude_e7: i32,
    monthly_raw_ieee754_le_hex: Vec<String>,
    monthly_millicelsius: Vec<i64>,
    monthly_artifact_digests: Vec<Digest>,
}

#[derive(Debug)]
struct ChelsaGridAxes {
    latitudes_e7: Vec<i32>,
    longitudes_e7: Vec<i32>,
}

impl ChelsaGridAxes {
    fn nearest_cell(&self, coordinate: GeographicCoordinateE7) -> Result<(u64, u64)> {
        let row = nearest_sorted_e7_index(&self.latitudes_e7, coordinate.latitude_e7())?;
        let column = nearest_sorted_e7_index(&self.longitudes_e7, coordinate.longitude_e7())?;
        Ok((u64::try_from(row)?, u64::try_from(column)?))
    }
}

fn inspect_chelsa_january_temperature(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no CHELSA NetCDF data artifact")?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified CHELSA NetCDF through the pure-Rust reader")?;
    validate_chelsa_january_temperature_schema(&file)?;
    let data_attributes = file
        .variable("Band1")
        .context("CHELSA has no January-temperature data variable")?
        .attributes()
        .iter()
        .map(|attribute| ChelsaAttributeInspection {
            name: attribute.name.clone(),
            string_value: attribute.value.as_string(),
            first_numeric_value: attribute.value.as_f64().map(|value| value.to_string()),
        })
        .collect::<Vec<_>>();
    let latitude_endpoint_ieee754_le_hex =
        inspect_etopo_axis_endpoints(&file, "lat", CHELSA_LATITUDE_CELLS, "CHELSA latitude")?;
    let longitude_endpoint_ieee754_le_hex =
        inspect_etopo_axis_endpoints(&file, "lon", CHELSA_LONGITUDE_CELLS, "CHELSA longitude")?;
    let mut variables = file
        .variables()
        .context("enumerate CHELSA variables")?
        .iter()
        .map(|variable| EtopoVariableInspection {
            name: variable.name().to_owned(),
            shape: variable.shape().to_vec(),
        })
        .collect::<Vec<_>>();
    variables.sort_by(|left, right| left.name.cmp(&right.name));
    println!(
        "{}",
        serde_json::to_string(&ChelsaJanuaryTemperatureInspection {
            inspection_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            artifact_byte_length: artifact.byte_length,
            data_variable: "Band1".to_owned(),
            data_shape: vec![CHELSA_LATITUDE_CELLS, CHELSA_LONGITUDE_CELLS],
            latitude_endpoint_ieee754_le_hex,
            longitude_endpoint_ieee754_le_hex,
            data_attributes,
            variables,
        })?
    );
    Ok(())
}

/// Verify that every retained monthly normal has the same declared CHELSA grid and
/// exact coordinate axes before a climate normalizer can treat them as one annual
/// cycle. This reads metadata and coordinate endpoints, not the full raster payload.
fn inspect_chelsa_annual_temperature(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let artifacts = chelsa_annual_temperature_artifacts(&snapshot)?;
    let mut shared_latitude = None;
    let mut shared_longitude = None;
    let mut monthly_normals = Vec::with_capacity(12);
    for (offset, artifact) in artifacts.into_iter().enumerate() {
        let expected_month = u8::try_from(offset + 1)?;
        let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
            .context("parse verified CHELSA NetCDF through the pure-Rust reader")?;
        validate_chelsa_january_temperature_schema(&file)?;
        let latitude =
            inspect_etopo_axis_endpoints(&file, "lat", CHELSA_LATITUDE_CELLS, "CHELSA latitude")?;
        let longitude =
            inspect_etopo_axis_endpoints(&file, "lon", CHELSA_LONGITUDE_CELLS, "CHELSA longitude")?;
        if let Some(expected) = shared_latitude.as_ref()
            && &latitude != expected
        {
            bail!("CHELSA monthly normals disagree on the latitude coordinate axis");
        }
        if let Some(expected) = shared_longitude.as_ref()
            && &longitude != expected
        {
            bail!("CHELSA monthly normals disagree on the longitude coordinate axis");
        }
        shared_latitude.get_or_insert(latitude);
        shared_longitude.get_or_insert(longitude);
        monthly_normals.push(ChelsaMonthlyTemperatureInspection {
            month: expected_month,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            artifact_byte_length: artifact.byte_length,
        });
    }
    println!(
        "{}",
        serde_json::to_string(&ChelsaAnnualTemperatureInspection {
            inspection_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            monthly_normals,
            shared_latitude_endpoint_ieee754_le_hex: shared_latitude
                .context("annual CHELSA snapshot is empty")?,
            shared_longitude_endpoint_ieee754_le_hex: shared_longitude
                .context("annual CHELSA snapshot is empty")?,
            documented_raw_to_millicelsius: "stored value * 100 - 273150",
        })?
    );
    Ok(())
}

fn validate_chelsa_january_temperature_schema(file: &NcFile) -> Result<()> {
    let latitude = file.variable("lat").context("CHELSA has no lat variable")?;
    let longitude = file.variable("lon").context("CHELSA has no lon variable")?;
    let temperature = file
        .variable("Band1")
        .context("CHELSA has no January-temperature data variable")?;
    if latitude.shape() != [CHELSA_LATITUDE_CELLS]
        || longitude.shape() != [CHELSA_LONGITUDE_CELLS]
        || temperature.shape() != [CHELSA_LATITUDE_CELLS, CHELSA_LONGITUDE_CELLS]
    {
        bail!("CHELSA variables do not have the pinned January-temperature grid shape");
    }
    Ok(())
}

fn inspect_chelsa_january_cell(
    manifest_path: &Path,
    artifact_root: &Path,
    row: u64,
    column: u64,
) -> Result<()> {
    validate_chelsa_cell_address(row, column)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no CHELSA NetCDF data artifact")?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified CHELSA NetCDF through the pure-Rust reader")?;
    validate_chelsa_january_temperature_schema(&file)?;
    let latitude = read_chelsa_coordinate(&file, "lat", row, "latitude")?;
    let longitude = read_chelsa_coordinate(&file, "lon", column, "longitude")?;
    let selection = NcSliceInfo {
        selections: vec![NcSliceInfoElem::Index(row), NcSliceInfoElem::Index(column)],
    };
    let values = file
        .read_variable_slice::<f32>("Band1", &selection)
        .context("read one raw CHELSA January-temperature sample")?;
    let value = values
        .as_slice()
        .context("CHELSA sample selection is not contiguous")?
        .first()
        .copied()
        .context("CHELSA sample selection is empty")?;
    let documented_temperature_millicelsius = chelsa_raw_tas_to_millicelsius(value)?;
    println!(
        "{}",
        serde_json::to_string(&ChelsaJanuaryCellInspection {
            inspection_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            row,
            column,
            latitude_ieee754_le_hex: format!("{:016x}", latitude.to_bits()),
            longitude_ieee754_le_hex: format!("{:016x}", longitude.to_bits()),
            raw_band1_ieee754_le_hex: format!("{:08x}", value.to_bits()),
            documented_temperature_millicelsius,
        })?
    );
    Ok(())
}

/// Resolve a geography coordinate through the retained axis coordinates themselves.
/// No assumed origin or floating-point increment is permitted: the NetCDF coordinate
/// vectors are the source contract, converted from their IEEE bits to the E7 lattice.
fn inspect_chelsa_nearest_cell(
    manifest_path: &Path,
    artifact_root: &Path,
    latitude_e7: i32,
    longitude_e7: i32,
) -> Result<()> {
    let coordinate = GeographicCoordinateE7::new(latitude_e7, longitude_e7)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no CHELSA NetCDF data artifact")?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified CHELSA NetCDF through the pure-Rust reader")?;
    let axes = read_chelsa_grid_axes(&file)?;
    let (row, column) = axes.nearest_cell(coordinate)?;
    println!(
        "{}",
        serde_json::to_string(&ChelsaNearestCellInspection {
            inspection_schema_version: 1,
            source_snapshot_digest: snapshot.content_digest()?,
            requested_latitude_e7: latitude_e7,
            requested_longitude_e7: longitude_e7,
            row,
            column,
            source_latitude_e7: axes.latitudes_e7[usize::try_from(row)?],
            source_longitude_e7: axes.longitudes_e7[usize::try_from(column)?],
        })?
    );
    Ok(())
}

/// Read the retained January-through-December normal vector for one mapped source
/// location. This is an auditable source inspection, not a climate-layer derivation:
/// it deliberately exposes exact stored samples and phase provenance before any
/// spatial aggregation can summarize them.
fn inspect_chelsa_annual_cell(
    manifest_path: &Path,
    artifact_root: &Path,
    latitude_e7: i32,
    longitude_e7: i32,
) -> Result<()> {
    let coordinate = GeographicCoordinateE7::new(latitude_e7, longitude_e7)?;
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifacts = chelsa_annual_temperature_artifacts(&snapshot)?;
    let first = artifacts
        .first()
        .context("annual CHELSA snapshot is empty")?;
    let first_file = NcFile::open(artifact_root.join(&first.artifact_path))
        .context("parse first verified CHELSA monthly NetCDF")?;
    let axes = read_chelsa_grid_axes(&first_file)?;
    let (row, column) = axes.nearest_cell(coordinate)?;
    let selection = NcSliceInfo {
        selections: vec![NcSliceInfoElem::Index(row), NcSliceInfoElem::Index(column)],
    };
    let mut monthly_raw_ieee754_le_hex = Vec::with_capacity(12);
    let mut monthly_millicelsius = Vec::with_capacity(12);
    let mut monthly_artifact_digests = Vec::with_capacity(12);
    for artifact in artifacts {
        let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
            .context("parse verified CHELSA monthly NetCDF")?;
        validate_chelsa_january_temperature_schema(&file)?;
        let values = file
            .read_variable_slice::<f32>("Band1", &selection)
            .context("read CHELSA monthly sample")?;
        let value = values
            .as_slice()
            .context("CHELSA sample selection is not contiguous")?
            .first()
            .copied()
            .context("CHELSA sample selection is empty")?;
        monthly_raw_ieee754_le_hex.push(format!("{:08x}", value.to_bits()));
        monthly_millicelsius.push(chelsa_raw_tas_to_millicelsius(value)?);
        monthly_artifact_digests.push(artifact.content_hash);
    }
    println!(
        "{}",
        serde_json::to_string(&ChelsaAnnualCellInspection {
            inspection_schema_version: 1,
            source_snapshot_digest: snapshot.content_digest()?,
            requested_latitude_e7: latitude_e7,
            requested_longitude_e7: longitude_e7,
            row,
            column,
            source_latitude_e7: axes.latitudes_e7[usize::try_from(row)?],
            source_longitude_e7: axes.longitudes_e7[usize::try_from(column)?],
            monthly_raw_ieee754_le_hex,
            monthly_millicelsius,
            monthly_artifact_digests,
        })?
    );
    Ok(())
}

fn chelsa_annual_temperature_artifacts(
    snapshot: &SourceSnapshotManifest,
) -> Result<Vec<&SourceSnapshotArtifact>> {
    let mut artifacts = snapshot
        .artifacts
        .iter()
        .filter(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.artifact_path.cmp(&right.artifact_path));
    if artifacts.len() != 12 {
        bail!("annual CHELSA snapshot must retain exactly twelve monthly NetCDF artifacts");
    }
    for (offset, artifact) in artifacts.iter().enumerate() {
        let expected_month = u8::try_from(offset + 1)?;
        let expected_suffix = format!("tas_{expected_month:02}_1981-2010_v.2.1.nc");
        if !artifact.artifact_path.ends_with(&expected_suffix) {
            bail!("annual CHELSA artifacts are not canonical January-through-December order");
        }
    }
    Ok(artifacts)
}

fn read_chelsa_grid_axes(file: &NcFile) -> Result<ChelsaGridAxes> {
    validate_chelsa_january_temperature_schema(file)?;
    let read_axis = |variable: &str, expected: u64| -> Result<Vec<i32>> {
        let values = file
            .read_variable_slice::<f64>(variable, &NcSliceInfo::all(1))
            .with_context(|| format!("read complete CHELSA {variable} axis"))?;
        let values = values
            .as_slice()
            .context("CHELSA axis selection is not contiguous")?;
        if u64::try_from(values.len())? != expected {
            bail!("CHELSA axis length disagrees with declared shape");
        }
        values
            .iter()
            .map(|value| {
                i32::try_from(ieee754_degrees_bits_to_e7(value.to_bits())?)
                    .context("CHELSA E7 coordinate does not fit i32")
            })
            .collect()
    };
    let latitudes_e7 = read_axis("lat", CHELSA_LATITUDE_CELLS)?;
    let longitudes_e7 = read_axis("lon", CHELSA_LONGITUDE_CELLS)?;
    if !latitudes_e7.windows(2).all(|pair| pair[0] < pair[1])
        || !longitudes_e7.windows(2).all(|pair| pair[0] < pair[1])
    {
        bail!("CHELSA coordinate axes are not strictly increasing on the E7 lattice");
    }
    Ok(ChelsaGridAxes {
        latitudes_e7,
        longitudes_e7,
    })
}

fn nearest_sorted_e7_index(axis: &[i32], value: i32) -> Result<usize> {
    if axis.is_empty() || value < axis[0] || value > *axis.last().context("empty CHELSA axis")? {
        bail!("requested coordinate lies outside retained CHELSA source coverage");
    }
    match axis.binary_search(&value) {
        Ok(index) => Ok(index),
        Err(upper) => {
            let lower = upper
                .checked_sub(1)
                .context("CHELSA axis lower bound underflow")?;
            let lower_distance = i64::from(value) - i64::from(axis[lower]);
            let upper_distance = i64::from(axis[upper]) - i64::from(value);
            // Ties choose the smaller raw index, a declared deterministic policy.
            Ok(if lower_distance <= upper_distance {
                lower
            } else {
                upper
            })
        }
    }
}

/// The retained CHELSA technical specification defines `tas_01` through `tas_12`
/// as degrees Celsius after multiplying stored values by 0.1 and adding -273.15.
fn chelsa_raw_tas_to_millicelsius(raw: f32) -> Result<i64> {
    if !raw.is_finite() || raw.fract() != 0.0 {
        bail!("CHELSA tas sample must be a finite integral stored value");
    }
    let stored = raw as i64;
    stored
        .checked_mul(100)
        .and_then(|value| value.checked_sub(273_150))
        .context("CHELSA tas millidegree conversion overflow")
}

fn validate_chelsa_cell_address(row: u64, column: u64) -> Result<()> {
    if row >= CHELSA_LATITUDE_CELLS || column >= CHELSA_LONGITUDE_CELLS {
        bail!(
            "CHELSA cell ({row}, {column}) is outside the pinned {} by {} source grid",
            CHELSA_LATITUDE_CELLS,
            CHELSA_LONGITUDE_CELLS
        );
    }
    Ok(())
}

fn read_chelsa_coordinate(file: &NcFile, variable: &str, index: u64, axis: &str) -> Result<f64> {
    let selection = NcSliceInfo {
        selections: vec![NcSliceInfoElem::Index(index)],
    };
    let coordinates = file
        .read_variable_slice::<f64>(variable, &selection)
        .with_context(|| format!("read CHELSA {axis} coordinate"))?;
    coordinates
        .as_slice()
        .with_context(|| format!("CHELSA {axis} coordinate selection is not contiguous"))?
        .first()
        .copied()
        .with_context(|| format!("CHELSA {axis} coordinate selection is empty"))
}

fn inspect_etopo(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".nc")
        })
        .context("source snapshot has no ETOPO NetCDF data artifact")?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let file = NcFile::open(artifact_root.join(&artifact.artifact_path))
        .context("parse verified ETOPO NetCDF through the pure-Rust reader")?;
    validate_etopo_schema(&file)?;
    let latitude_endpoint_ieee754_le_hex =
        inspect_etopo_axis_endpoints(&file, "lat", ETOPO_LATITUDE_CELLS, "latitude")?;
    let longitude_endpoint_ieee754_le_hex =
        inspect_etopo_axis_endpoints(&file, "lon", ETOPO_LONGITUDE_CELLS, "longitude")?;
    let mut variables = file
        .variables()
        .context("enumerate ETOPO variables")?
        .iter()
        .map(|variable| EtopoVariableInspection {
            name: variable.name().to_owned(),
            shape: variable.shape().to_vec(),
        })
        .collect::<Vec<_>>();
    variables.sort_by(|left, right| left.name.cmp(&right.name));
    println!(
        "{}",
        serde_json::to_string(&EtopoInspection {
            inspection_schema_version: 2,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            artifact_byte_length: artifact.byte_length,
            latitude_endpoint_ieee754_le_hex,
            longitude_endpoint_ieee754_le_hex,
            variables,
        })?
    );
    Ok(())
}

fn inspect_etopo_axis_endpoints(
    file: &NcFile,
    variable_name: &str,
    expected_length: u64,
    axis_name: &str,
) -> Result<[String; 2]> {
    let values = file
        .read_variable::<f64>(variable_name)
        .with_context(|| format!("read ETOPO {axis_name} coordinate axis"))?;
    let values = values
        .as_slice()
        .with_context(|| format!("ETOPO {axis_name} coordinate axis is not contiguous"))?;
    if values.len() != usize::try_from(expected_length)? {
        bail!("ETOPO {axis_name} coordinate axis has an unexpected length");
    }
    let first = values.first().context("ETOPO coordinate axis is empty")?;
    let last = values.last().context("ETOPO coordinate axis is empty")?;
    Ok([
        format!("{:016x}", first.to_bits()),
        format!("{:016x}", last.to_bits()),
    ])
}

#[derive(Serialize)]
struct NaturalEarthLandInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
    shapefile_version: u32,
    declared_shape_type: u32,
    bounding_box_ieee754_le_hex: [String; 4],
    record_count: u64,
    polygon_record_count: u64,
    part_count: u64,
    point_count: u64,
}

fn inspect_natural_earth_land(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".shp")
        })
        .context("source snapshot has no Natural Earth .shp data artifact")?;
    let bytes = fs::read(artifact_root.join(&artifact.artifact_path))?;
    let parsed = parse_polygon_shapefile(&bytes)?;
    let source_snapshot_digest = snapshot.content_digest()?;
    let inspection = NaturalEarthLandInspection {
        inspection_schema_version: 1,
        source_snapshot_id: snapshot.snapshot_id,
        source_snapshot_digest,
        artifact_path: artifact.artifact_path.clone(),
        artifact_hash: artifact.content_hash,
        artifact_byte_length: artifact.byte_length,
        shapefile_version: parsed.version,
        declared_shape_type: parsed.shape_type,
        bounding_box_ieee754_le_hex: parsed.bounding_box.map(|bits| format!("{bits:016x}")),
        record_count: parsed.records,
        polygon_record_count: parsed.polygons,
        part_count: parsed.parts,
        point_count: parsed.points,
    };
    println!("{}", serde_json::to_string(&inspection)?);
    Ok(())
}

#[derive(Serialize)]
struct NaturalEarthLandPointInspection {
    inspection_schema_version: u16,
    source_snapshot_digest: Digest,
    latitude_e7: i32,
    longitude_e7: i32,
    inside_land_polygon: bool,
}

fn inspect_natural_earth_land_point(
    manifest_path: &Path,
    artifact_root: &Path,
    latitude_e7: i32,
    longitude_e7: i32,
) -> Result<()> {
    if !(-900_000_000..=900_000_000).contains(&latitude_e7)
        || !(-1_800_000_000..1_800_000_000).contains(&longitude_e7)
    {
        bail!("Natural Earth point is outside exact WGS 84 E7 bounds");
    }
    let snapshot = load_source_manifest(manifest_path)?;
    verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.role == world_data::SourceSnapshotArtifactRole::Data
                && artifact.artifact_path.ends_with(".shp")
        })
        .context("source snapshot has no Natural Earth .shp data artifact")?;
    let bytes = fs::read(artifact_root.join(&artifact.artifact_path))?;
    let inside_land_polygon = natural_earth_contains_point(&bytes, longitude_e7, latitude_e7)?;
    println!(
        "{}",
        serde_json::to_string(&NaturalEarthLandPointInspection {
            inspection_schema_version: 1,
            source_snapshot_digest: snapshot.content_digest()?,
            latitude_e7,
            longitude_e7,
            inside_land_polygon,
        })?
    );
    Ok(())
}

/// Classify an exact E7 coordinate using the generalized Natural Earth polygons.
///
/// Shapefiles store vertices as IEEE-754 doubles.  The classifier converts each
/// source bit pattern once to its nearest E7 lattice point with integer arithmetic,
/// then performs the even-odd test using integer cross-products. This prevents host
/// floating-point behavior from becoming a normalization input.
fn natural_earth_contains_point(bytes: &[u8], longitude_e7: i32, latitude_e7: i32) -> Result<bool> {
    parse_polygon_shapefile(bytes)?;
    natural_earth_contains_point_unvalidated(bytes, longitude_e7, latitude_e7)
}

// A one-degree latitude bucket is deliberately only an execution index.  It has no
// representation in a derived tile: the classifier below performs exactly the same
// half-open horizontal-ray test as `point_in_shapefile_ring`, using the same E7
// conversion.  It prevents the full source stream (and its IEEE-754 decoding) from
// being traversed once per L10 target centre.
const NATURAL_EARTH_LATITUDE_BUCKETS: usize = 180;
const NATURAL_EARTH_E7_PER_DEGREE: i128 = 10_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NaturalEarthEdge {
    current_x: i128,
    current_y: i128,
    previous_x: i128,
    previous_y: i128,
}

#[derive(Debug)]
struct PreparedNaturalEarthLand {
    // Edges are present in every degree band that can satisfy
    // min_y <= query_y < max_y. Order is source order, though parity makes the
    // result independent of that order.
    edges_by_latitude_bucket: Vec<Vec<NaturalEarthEdge>>,
}

impl PreparedNaturalEarthLand {
    fn from_shapefile(bytes: &[u8]) -> Result<Self> {
        parse_polygon_shapefile(bytes)?;
        let mut edges_by_latitude_bucket = vec![Vec::new(); NATURAL_EARTH_LATITUDE_BUCKETS];
        let mut offset = 100_usize;
        while offset < bytes.len() {
            let content_length = usize::try_from(be_u32(&bytes[offset + 4..offset + 8])?)?
                .checked_mul(2)
                .context("record length overflow")?;
            let body = &bytes[offset + 8..offset + 8 + content_length];
            if le_u32(&body[..4])? == 5 {
                let parts = usize::try_from(le_u32(&body[36..40])?)?;
                let points = usize::try_from(le_u32(&body[40..44])?)?;
                let part_start = 44;
                let point_start = part_start + parts * 4;
                let source_points = &body[point_start..];
                for part in 0..parts {
                    let start = usize::try_from(le_u32(
                        &body[part_start + part * 4..part_start + part * 4 + 4],
                    )?)?;
                    let end = if part + 1 == parts {
                        points
                    } else {
                        usize::try_from(le_u32(
                            &body[part_start + (part + 1) * 4..part_start + (part + 1) * 4 + 4],
                        )?)?
                    };
                    if end <= start + 2 {
                        continue;
                    }
                    let point = |index: usize| -> Result<(i128, i128)> {
                        let point_offset =
                            index.checked_mul(16).context("point offset overflow")?;
                        Ok((
                            ieee754_degrees_bits_to_e7(le_u64(
                                &source_points[point_offset..point_offset + 8],
                            )?)?,
                            ieee754_degrees_bits_to_e7(le_u64(
                                &source_points[point_offset + 8..point_offset + 16],
                            )?)?,
                        ))
                    };
                    let mut previous = point(end - 1)?;
                    for index in start..end {
                        let current = point(index)?;
                        let edge = NaturalEarthEdge {
                            current_x: current.0,
                            current_y: current.1,
                            previous_x: previous.0,
                            previous_y: previous.1,
                        };
                        if edge.current_y != edge.previous_y {
                            let lower = edge.current_y.min(edge.previous_y);
                            let upper = edge.current_y.max(edge.previous_y) - 1;
                            let first = natural_earth_latitude_bucket(lower)?;
                            let last = natural_earth_latitude_bucket(upper)?;
                            for bucket_edges in edges_by_latitude_bucket
                                .iter_mut()
                                .take(last + 1)
                                .skip(first)
                            {
                                bucket_edges.push(edge);
                            }
                        }
                        previous = current;
                    }
                }
            }
            offset += 8 + content_length;
        }
        Ok(Self {
            edges_by_latitude_bucket,
        })
    }

    fn contains_point(&self, longitude_e7: i32, latitude_e7: i32) -> bool {
        let x = i128::from(longitude_e7);
        let y = i128::from(latitude_e7);
        let bucket = natural_earth_latitude_bucket(y)
            .expect("validated geographic E7 coordinate must select a latitude bucket");
        self.edges_by_latitude_bucket[bucket]
            .iter()
            .filter(|edge| (edge.current_y > y) != (edge.previous_y > y))
            .fold(false, |inside, edge| {
                let denominator = edge.previous_y - edge.current_y;
                let left = (x - edge.current_x) * denominator;
                let right = (edge.previous_x - edge.current_x) * (y - edge.current_y);
                if if denominator > 0 {
                    left < right
                } else {
                    left > right
                } {
                    !inside
                } else {
                    inside
                }
            })
    }
}

fn natural_earth_latitude_bucket(latitude_e7: i128) -> Result<usize> {
    if !(-900_000_000..=900_000_000).contains(&latitude_e7) {
        bail!("Natural Earth latitude is outside exact WGS 84 E7 bounds");
    }
    // 90° has no non-horizontal crossing edge, but route it to the final bucket
    // so direct classification remains total at the closed geographic pole.
    let bucket = ((latitude_e7 + 900_000_000) / NATURAL_EARTH_E7_PER_DEGREE)
        .min(i128::try_from(NATURAL_EARTH_LATITUDE_BUCKETS - 1)?);
    usize::try_from(bucket).context("latitude bucket does not fit usize")
}

/// The caller has already validated the complete polygon stream with
/// `parse_polygon_shapefile`; this avoids a full source traversal per target cell.
fn natural_earth_contains_point_unvalidated(
    bytes: &[u8],
    longitude_e7: i32,
    latitude_e7: i32,
) -> Result<bool> {
    let mut inside = false;
    let mut offset = 100_usize;
    while offset < bytes.len() {
        let content_length = usize::try_from(be_u32(&bytes[offset + 4..offset + 8])?)?
            .checked_mul(2)
            .context("record length overflow")?;
        let body = &bytes[offset + 8..offset + 8 + content_length];
        if le_u32(&body[..4])? == 5 {
            let parts = usize::try_from(le_u32(&body[36..40])?)?;
            let points = usize::try_from(le_u32(&body[40..44])?)?;
            let part_start = 44;
            let point_start = part_start + parts * 4;
            for part in 0..parts {
                let start = usize::try_from(le_u32(
                    &body[part_start + part * 4..part_start + part * 4 + 4],
                )?)?;
                let end = if part + 1 == parts {
                    points
                } else {
                    usize::try_from(le_u32(
                        &body[part_start + (part + 1) * 4..part_start + (part + 1) * 4 + 4],
                    )?)?
                };
                if point_in_shapefile_ring(
                    &body[point_start..],
                    start,
                    end,
                    i128::from(longitude_e7),
                    i128::from(latitude_e7),
                )? {
                    inside = !inside;
                }
            }
        }
        offset += 8 + content_length;
    }
    Ok(inside)
}

fn point_in_shapefile_ring(
    points: &[u8],
    start: usize,
    end: usize,
    x: i128,
    y: i128,
) -> Result<bool> {
    if end <= start + 2 {
        return Ok(false);
    }
    let point = |index: usize| -> Result<(i128, i128)> {
        let offset = index.checked_mul(16).context("point offset overflow")?;
        Ok((
            ieee754_degrees_bits_to_e7(le_u64(&points[offset..offset + 8])?)?,
            ieee754_degrees_bits_to_e7(le_u64(&points[offset + 8..offset + 16])?)?,
        ))
    };
    let mut crossed = false;
    let mut previous = point(end - 1)?;
    for index in start..end {
        let current = point(index)?;
        let crosses_horizontal_ray = (current.1 > y) != (previous.1 > y);
        let denominator = previous.1 - current.1;
        let left = (x - current.0) * denominator;
        let right = (previous.0 - current.0) * (y - current.1);
        if crosses_horizontal_ray
            && if denominator > 0 {
                left < right
            } else {
                left > right
            }
        {
            crossed = !crossed;
        }
        previous = current;
    }
    Ok(crossed)
}

fn ieee754_degrees_bits_to_e7(bits: u64) -> Result<i128> {
    let sign = if bits >> 63 == 0 { 1_i128 } else { -1_i128 };
    let exponent = i32::try_from((bits >> 52) & 0x7ff)?;
    let fraction = bits & ((1_u64 << 52) - 1);
    if exponent == 0x7ff {
        bail!("Natural Earth coordinate is not finite");
    }
    if exponent == 0 && fraction == 0 {
        return Ok(0);
    }
    let (mantissa, binary_exponent) = if exponent == 0 {
        (i128::from(fraction), -1074_i32)
    } else {
        (i128::from((1_u64 << 52) | fraction), exponent - 1075)
    };
    let scaled = mantissa
        .checked_mul(10_000_000)
        .context("Natural Earth E7 mantissa overflow")?;
    let rounded = if binary_exponent >= 0 {
        scaled
            .checked_shl(u32::try_from(binary_exponent)?)
            .context("Natural Earth E7 exponent overflow")?
    } else {
        let divisor = 1_i128
            .checked_shl(u32::try_from(-binary_exponent)?)
            .context("Natural Earth E7 divisor overflow")?;
        let quotient = scaled / divisor;
        let remainder = scaled % divisor;
        quotient + i128::from(remainder * 2 >= divisor)
    };
    sign.checked_mul(rounded)
        .context("Natural Earth E7 sign overflow")
}

#[derive(Debug, Eq, PartialEq)]
struct PolygonShapefileSummary {
    version: u32,
    shape_type: u32,
    bounding_box: [u64; 4],
    records: u64,
    polygons: u64,
    parts: u64,
    points: u64,
}

fn parse_polygon_shapefile(bytes: &[u8]) -> Result<PolygonShapefileSummary> {
    if bytes.len() < 100 {
        bail!("shapefile is shorter than its header");
    }
    if be_u32(&bytes[0..4])? != 9994 {
        bail!("unexpected shapefile code");
    }
    let words = usize::try_from(be_u32(&bytes[24..28])?)?;
    if words.checked_mul(2) != Some(bytes.len()) {
        bail!("shapefile header length disagrees with file");
    }
    let version = le_u32(&bytes[28..32])?;
    if version != 1000 {
        bail!("unsupported shapefile version {version}");
    }
    let shape_type = le_u32(&bytes[32..36])?;
    if shape_type != 5 {
        bail!("expected polygon shapefile type 5");
    }
    let mut bounding_box = [0_u64; 4];
    for (index, value) in bounding_box.iter_mut().enumerate() {
        *value = le_u64(&bytes[36 + index * 8..44 + index * 8])?;
    }
    let (mut offset, mut records, mut polygons, mut parts, mut points) =
        (100_usize, 0_u64, 0_u64, 0_u64, 0_u64);
    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            bail!("truncated shapefile record header");
        }
        let content_length = usize::try_from(be_u32(&bytes[offset + 4..offset + 8])?)?
            .checked_mul(2)
            .context("shapefile record length overflow")?;
        let start = offset.checked_add(8).context("record offset overflow")?;
        let end = start
            .checked_add(content_length)
            .context("record end overflow")?;
        if end > bytes.len() {
            bail!("truncated shapefile record body");
        }
        records += 1;
        let body = &bytes[start..end];
        if body.len() < 4 {
            bail!("empty shapefile record");
        }
        match le_u32(&body[..4])? {
            0 => {}
            5 => {
                if body.len() < 44 {
                    bail!("truncated polygon record");
                }
                let record_parts = usize::try_from(le_u32(&body[36..40])?)?;
                let record_points = usize::try_from(le_u32(&body[40..44])?)?;
                let expected = 44_usize
                    .checked_add(record_parts.checked_mul(4).context("part overflow")?)
                    .and_then(|value| value.checked_add(record_points.checked_mul(16)?))
                    .context("polygon record length overflow")?;
                if expected != body.len() {
                    bail!("polygon record length disagrees with counts");
                }
                polygons += 1;
                parts += u64::try_from(record_parts)?;
                points += u64::try_from(record_points)?;
            }
            _ => bail!("polygon shapefile contains an unexpected record type"),
        }
        offset = end;
    }
    Ok(PolygonShapefileSummary {
        version,
        shape_type,
        bounding_box,
        records,
        polygons,
        parts,
        points,
    })
}

fn be_u32(bytes: &[u8]) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes.try_into().context("expected four bytes")?,
    ))
}
fn le_u32(bytes: &[u8]) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes.try_into().context("expected four bytes")?,
    ))
}
fn le_u64(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_le_bytes(
        bytes.try_into().context("expected eight bytes")?,
    ))
}

fn validate(bundle_path: PathBuf, configuration_path: Option<&PathBuf>) -> Result<()> {
    let bytes = fs::read(&bundle_path)
        .with_context(|| format!("failed to read bundle {}", bundle_path.display()))?;
    let bundle = WorldDataBundle::from_canonical_slice(&bytes)
        .with_context(|| format!("bundle {} is invalid", bundle_path.display()))?;
    let digest = bundle.content_digest()?;
    let artifact_root = bundle_path
        .parent()
        .context("bundle path has no parent directory")?;
    let stats = verify_release_artifacts(&bundle, artifact_root)?;

    if let Some(path) = configuration_path {
        let config_bytes = fs::read(path)
            .with_context(|| format!("failed to read configuration {}", path.display()))?;
        let configuration: WorldConfiguration = serde_json::from_slice(&config_bytes)
            .with_context(|| format!("failed to decode configuration {}", path.display()))?;
        bundle
            .validate_for_configuration(&configuration)
            .with_context(|| format!("bundle does not match configuration {}", path.display()))?;
        println!("configuration: matched {}", path.display());
    }

    println!("bundle: {}@{}", bundle.bundle_id, bundle.bundle_version);
    println!("schema: {}", bundle.bundle_schema_version);
    println!("sha256: {digest}");
    println!("sources: {}", bundle.sources.len());
    println!("entities: {}", bundle.entities.len());
    println!("parameters: {}", bundle.parameters.len());
    println!("layers: {}", bundle.layers.len());
    println!("tile indexes: {}", stats.tile_indexes);
    println!("tiles: {}", stats.tiles);
    println!(
        "artifacts: {} ({} bytes verified)",
        stats.artifacts, stats.bytes
    );
    Ok(())
}

fn load_source_manifest(path: &Path) -> Result<SourceSnapshotManifest> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read source manifest {}", path.display()))?;
    SourceSnapshotManifest::from_canonical_slice(&bytes)
        .with_context(|| format!("source manifest {} is invalid", path.display()))
}

fn validate_source(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    let digest = snapshot.content_digest()?;
    let stats = verify_source_snapshot_artifacts(&snapshot, artifact_root)?;
    println!("source snapshot: {}", snapshot.snapshot_id);
    println!("upstream release: {}", snapshot.upstream_release);
    println!("dataset version: {}", snapshot.dataset_version);
    println!("sha256: {digest}");
    println!(
        "artifacts: {} ({} bytes verified)",
        stats.artifacts, stats.bytes
    );
    Ok(())
}

async fn fetch_source(manifest_path: &Path, artifact_root: &Path) -> Result<()> {
    let snapshot = load_source_manifest(manifest_path)?;
    fs::create_dir_all(artifact_root).with_context(|| {
        format!(
            "failed to create source artifact root {}",
            artifact_root.display()
        )
    })?;
    let canonical_root = artifact_root.canonicalize().with_context(|| {
        format!(
            "failed to resolve source artifact root {}",
            artifact_root.display()
        )
    })?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::limited(8))
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(60))
        .user_agent("a-tiny-civilization-source-acquisition/0.1")
        .build()
        .context("failed to construct HTTPS source client")?;

    for artifact in &snapshot.artifacts {
        let destination = prepare_destination(&canonical_root, &artifact.artifact_path)?;
        match fs::symlink_metadata(&destination) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    bail!(
                        "source destination {} exists but is not a regular file",
                        destination.display()
                    );
                }
                verify_source_snapshot_artifact(artifact, &canonical_root).with_context(|| {
                    format!(
                        "existing source artifact {} differs; refusing to replace it",
                        destination.display()
                    )
                })?;
                println!("verified existing {}", artifact.artifact_path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                download_artifact(&client, artifact, &destination).await?;
                println!("fetched {}", artifact.artifact_path);
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect source destination {}",
                        destination.display()
                    )
                });
            }
        }
    }

    validate_source(manifest_path, &canonical_root)
}

fn prepare_destination(canonical_root: &Path, relative_path: &str) -> Result<PathBuf> {
    let relative = Path::new(relative_path);
    let parent = relative
        .parent()
        .context("source artifact path has no parent")?;
    let mut current = canonical_root.to_path_buf();
    for component in parent.components() {
        let Component::Normal(part) = component else {
            bail!("source artifact path {relative_path:?} is not portable");
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    bail!(
                        "source artifact parent {} is not a regular directory",
                        current.display()
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).with_context(|| {
                    format!("failed to create source directory {}", current.display())
                })?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect source directory {}", current.display())
                });
            }
        }
    }
    Ok(canonical_root.join(relative))
}

async fn download_artifact(
    client: &reqwest::Client,
    artifact: &SourceSnapshotArtifact,
    destination: &Path,
) -> Result<()> {
    let mut response = client
        .get(&artifact.download_url)
        .send()
        .await
        .with_context(|| format!("failed to fetch {}", artifact.download_url))?
        .error_for_status()
        .with_context(|| format!("source returned an error for {}", artifact.download_url))?;
    if let Some(advertised) = response.content_length()
        && advertised != artifact.byte_length
    {
        bail!(
            "source {} advertised {} bytes, expected {}",
            artifact.download_url,
            advertised,
            artifact.byte_length
        );
    }

    let parent = destination
        .parent()
        .context("source destination has no parent directory")?;
    let file_name = destination
        .file_name()
        .and_then(OsStr::to_str)
        .context("source destination filename is not UTF-8")?;
    let mut partial = PartialDownload::create(parent, file_name)?;
    let mut hasher = Sha256::new();
    let mut actual_length = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("failed while reading {}", artifact.download_url))?
    {
        actual_length = actual_length
            .checked_add(u64::try_from(chunk.len()).context("response chunk length overflow")?)
            .context("downloaded source length overflow")?;
        if actual_length > artifact.byte_length {
            bail!(
                "source {} exceeded expected length {}",
                artifact.download_url,
                artifact.byte_length
            );
        }
        hasher.update(&chunk);
        partial
            .file
            .write_all(&chunk)
            .with_context(|| format!("failed to write {}", partial.path.display()))?;
    }
    let actual_digest = Digest::from_bytes(hasher.finalize().into());
    artifact
        .expected_artifact()
        .verify_observation(actual_length, actual_digest)
        .with_context(|| format!("downloaded source {:?} is invalid", artifact.artifact_path))?;
    partial
        .file
        .sync_all()
        .with_context(|| format!("failed to sync {}", partial.path.display()))?;
    partial.persist_without_replacement(destination)
}

struct PartialDownload {
    path: PathBuf,
    file: File,
    persisted: bool,
}

impl PartialDownload {
    fn create(parent: &Path, file_name: &str) -> Result<Self> {
        for attempt in 0..100_u16 {
            let path = parent.join(format!(
                ".{file_name}.atc-partial-{}-{attempt}",
                std::process::id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file,
                        persisted: false,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to create partial source file {}", path.display())
                    });
                }
            }
        }
        bail!("could not allocate a unique partial source filename")
    }

    fn persist_without_replacement(&mut self, destination: &Path) -> Result<()> {
        fs::hard_link(&self.path, destination).with_context(|| {
            format!(
                "failed to publish {} without replacing {}",
                self.path.display(),
                destination.display()
            )
        })?;
        fs::remove_file(&self.path)
            .with_context(|| format!("failed to remove partial file {}", self.path.display()))?;
        sync_parent_directory(destination)?;
        self.persisted = true;
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent_directory(destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("published source destination has no parent")?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("failed to sync source directory {}", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_destination: &Path) -> Result<()> {
    Ok(())
}

impl Drop for PartialDownload {
    fn drop(&mut self) {
        if !self.persisted {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn era5_member_contract_separates_instantaneous_and_accumulated_fields() {
        let instantaneous = expected_era5_member_variables(ERA5_ARCHIVE_MEMBERS[0])
            .expect("instantaneous ERA5 schema");
        let accumulated = expected_era5_member_variables(ERA5_ARCHIVE_MEMBERS[1])
            .expect("accumulated ERA5 schema");
        assert_eq!(instantaneous.get("t2m"), Some(&vec![12, 721, 1_440]));
        assert_eq!(instantaneous.get("siconc"), Some(&vec![12, 721, 1_440]));
        assert!(!instantaneous.contains_key("tp"));
        assert_eq!(accumulated.get("tp"), Some(&vec![12, 721, 1_440]));
        assert!(!accumulated.contains_key("t2m"));
        assert!(expected_era5_member_variables("unexpected.nc").is_err());
    }

    #[test]
    fn copernicus_land_cover_contract_keeps_classes_and_quality_fields_separate() {
        let variables = expected_copernicus_land_cover_variables();
        let expected_raster = vec![1, 64_800, 129_600];
        assert_eq!(
            variables.get("lccs_class"),
            Some(&(NcType::UByte, expected_raster.clone()))
        );
        assert_eq!(
            variables.get("processed_flag"),
            Some(&(NcType::Byte, expected_raster.clone()))
        );
        assert_eq!(
            variables.get("current_pixel_state"),
            Some(&(NcType::Byte, expected_raster.clone()))
        );
        assert_eq!(
            variables.get("observation_count"),
            Some(&(NcType::UShort, expected_raster.clone()))
        );
        assert_eq!(
            variables.get("change_count"),
            Some(&(NcType::UByte, expected_raster))
        );
        assert_eq!(variables.len(), 12);
        assert_eq!(
            expected_copernicus_land_cover_global_attributes(),
            BTreeMap::from([
                ("id", "C3S-LC-L4-LCCS-Map-300m-P1Y-2022-v2.1.1"),
                ("license", "EC C3S Land cover Data Policy"),
                ("product_version", "2.1.1"),
                ("source", "Sentinel-3 OLCI"),
                ("spatial_resolution", "300m"),
                ("time_coverage_end", "20221231"),
                ("time_coverage_start", "20220101"),
                ("tracking_id", "cbc0983e-a0fd-4277-9023-2e618c0c2067"),
            ])
        );
        assert_eq!(COPERNICUS_LCCS_CLASSES.len(), 38);
        assert_eq!(
            COPERNICUS_LAND_COVER_LATITUDE_CELLS / COPERNICUS_LAND_COVER_CHUNK_CELLS,
            32
        );
        assert_eq!(
            COPERNICUS_LAND_COVER_LONGITUDE_CELLS / COPERNICUS_LAND_COVER_CHUNK_CELLS,
            64
        );
    }

    #[test]
    fn committed_copernicus_census_fingerprints_complete_global_counts() {
        const CENSUS: &[u8] = include_bytes!(
            "../../../data/source-inspections/copernicus-satellite-land-cover-v2-1-1-2022-census.json"
        );
        assert_eq!(
            Digest::sha256(CENSUS).to_string(),
            "118fa2b71c9acdc785c131c0c9e8e19e00c1bad1b96805f4d286c48a8b35efee"
        );
        let census: serde_json::Value =
            serde_json::from_slice(CENSUS).expect("committed Copernicus census is JSON");
        let expected_cells = 8_398_080_000_u64;
        assert_eq!(
            census["source_snapshot_digest"].as_str(),
            Some("6b2acf6608c382c9321de4f69268c6e5caa2e564820094b41be844643bc27894")
        );
        assert_eq!(census["raster_cells"].as_u64(), Some(expected_cells));
        assert_eq!(census["chunks_scanned"].as_u64(), Some(2_048));
        assert_eq!(
            census["lccs_classes"]
                .as_array()
                .expect("LCCS class counts")
                .len(),
            38
        );
        for field in [
            "lccs_classes",
            "processed_flag_counts",
            "current_pixel_state_counts",
            "observation_count_counts",
            "change_count_counts",
        ] {
            let total = census[field]
                .as_array()
                .expect("census count array")
                .iter()
                .map(|entry| entry["cells"].as_u64().expect("integer cell count"))
                .sum::<u64>();
            assert_eq!(total, expected_cells, "incomplete {field} census");
        }
        let water = census["lccs_classes"]
            .as_array()
            .expect("LCCS class counts")
            .iter()
            .find(|entry| entry["value"].as_u64() == Some(210))
            .expect("water class");
        assert_eq!(water["cells"].as_u64(), Some(5_675_161_787));
        assert_eq!(
            census["observation_count_counts"]
                .as_array()
                .expect("observation counts")
                .last()
                .expect("maximum observed count")["value"]
                .as_u64(),
            Some(994)
        );
    }

    #[test]
    fn copernicus_source_area_lookup_has_explicit_global_edge_ownership() {
        assert_eq!(
            copernicus_source_area_cell(
                GeographicCoordinateE7::new(900_000_000, -1_800_000_000).expect("northwest corner")
            )
            .expect("northwest source area"),
            (0, 0)
        );
        assert_eq!(
            copernicus_source_area_cell(
                GeographicCoordinateE7::new(0, 0).expect("prime-meridian origin")
            )
            .expect("central source area"),
            (32_400, 64_800)
        );
        assert_eq!(
            copernicus_source_area_cell(
                GeographicCoordinateE7::new(-900_000_000, 1_799_999_999)
                    .expect("southeast terminal coordinate")
            )
            .expect("southeast source area"),
            (64_799, 129_599)
        );
    }

    #[test]
    fn copernicus_l10_q32_target_support_has_a_stable_full_fingerprint() {
        let target: S2CellId = "1000010000000000".parse().expect("equatorial L10 cell");
        let samples = copernicus_land_cover_target_support_samples(target, 32)
            .expect("Copernicus target support");
        assert_eq!(samples.len(), 1_024);
        assert_eq!(
            copernicus_target_support_fingerprint(&samples).to_string(),
            "213ce73747aa77914e0fb5f41f7382377d4b8da7a872f7c43e79ad493794c834"
        );
        assert_eq!(
            samples.first(),
            Some(&CopernicusLandCoverSourceSample {
                latitude_e7: 11_747,
                longitude_e7: 11_668,
                source_row: 32_399,
                source_column: 64_800,
            })
        );
        assert_eq!(
            samples.last(),
            Some(&CopernicusLandCoverSourceSample {
                latitude_e7: 740_052,
                longitude_e7: 735_099,
                source_row: 32_373,
                source_column: 64_826,
            })
        );
        assert_eq!(
            samples
                .iter()
                .map(|sample| (sample.source_row, sample.source_column))
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            729
        );
        for sample in samples {
            let coordinate = GeographicCoordinateE7::new(sample.latitude_e7, sample.longitude_e7)
                .expect("retained sample coordinate");
            assert_eq!(
                route_geographic_to_s2(coordinate, 10).expect("route retained target sample"),
                target
            );
        }
    }

    fn temporary_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "a-tiny-civilization-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create test root");
        root
    }

    #[test]
    fn partial_download_publishes_once_without_replacement() {
        let root = temporary_root("source-publish");
        let destination = root.join("source.bin");
        let mut first = PartialDownload::create(&root, "source.bin").expect("first partial");
        first.file.write_all(b"first").expect("write first partial");
        first
            .persist_without_replacement(&destination)
            .expect("publish first source");

        let mut second = PartialDownload::create(&root, "source.bin").expect("second partial");
        second
            .file
            .write_all(b"second")
            .expect("write second partial");
        assert!(second.persist_without_replacement(&destination).is_err());
        assert_eq!(
            fs::read(&destination).expect("read published source"),
            b"first"
        );
        drop(second);
        assert_eq!(fs::read_dir(&root).expect("read test root").count(), 1);
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[cfg(unix)]
    #[test]
    fn destination_preparation_rejects_symlinked_parents() {
        use std::os::unix::fs::symlink;

        let root = temporary_root("source-path");
        let outside = temporary_root("source-outside");
        symlink(&outside, root.join("redirect")).expect("create parent symlink");
        let canonical_root = root.canonicalize().expect("canonical test root");
        assert!(prepare_destination(&canonical_root, "redirect/source.bin").is_err());
        fs::remove_dir_all(root).expect("remove test root");
        fs::remove_dir_all(outside).expect("remove outside root");
    }

    #[test]
    fn parses_a_null_record_in_a_polygon_shapefile() {
        let mut bytes = vec![0_u8; 112];
        bytes[0..4].copy_from_slice(&9994_u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&56_u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&1000_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&5_u32.to_le_bytes());
        bytes[104..108].copy_from_slice(&2_u32.to_be_bytes());
        let summary = parse_polygon_shapefile(&bytes).expect("valid minimal shapefile");
        assert_eq!(summary.records, 1);
        assert_eq!(summary.polygons, 0);
    }

    fn minimal_null_polygon_shapefile() -> Vec<u8> {
        let mut bytes = vec![0_u8; 112];
        bytes[0..4].copy_from_slice(&9994_u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&56_u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&1000_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&5_u32.to_le_bytes());
        bytes[104..108].copy_from_slice(&2_u32.to_be_bytes());
        bytes
    }

    #[test]
    fn rejects_a_header_with_a_wrong_declared_length() {
        let mut bytes = vec![0_u8; 100];
        bytes[0..4].copy_from_slice(&9994_u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&49_u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&1000_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&5_u32.to_le_bytes());
        assert!(parse_polygon_shapefile(&bytes).is_err());
    }

    #[test]
    fn etopo_grid_bytes_preserve_provenance_dimensions_and_float_bits() {
        let snapshot = Digest::sha256(b"snapshot");
        let artifact = Digest::sha256(b"artifact");
        let encoded = encode_etopo_grid(5, snapshot, artifact, 1, 2, &[1.5, -42.25])
            .expect("encode a small ETOPO grid");
        assert_eq!(encoded.len(), ETOPO_GRID_HEADER_LENGTH + 8);
        assert_eq!(&encoded[0..8], ETOPO_GRID_MAGIC);
        assert_eq!(
            u16::from_le_bytes(encoded[8..10].try_into().expect("schema bytes")),
            1
        );
        assert_eq!(
            u16::from_le_bytes(encoded[10..12].try_into().expect("stride bytes")),
            5
        );
        assert_eq!(&encoded[12..44], snapshot.as_bytes());
        assert_eq!(&encoded[44..76], artifact.as_bytes());
        assert_eq!(
            u32::from_le_bytes(encoded[76..80].try_into().expect("latitude bytes")),
            1
        );
        assert_eq!(
            u32::from_le_bytes(encoded[80..84].try_into().expect("longitude bytes")),
            2
        );
        assert_eq!(
            u32::from_le_bytes(encoded[84..88].try_into().expect("first value bytes")),
            1.5_f32.to_bits()
        );
        assert_eq!(
            u32::from_le_bytes(encoded[88..92].try_into().expect("second value bytes")),
            (-42.25_f32).to_bits()
        );
    }

    #[test]
    fn etopo_grid_rejects_dimensions_that_disagree_with_values() {
        assert!(
            encode_etopo_grid(
                5,
                Digest::sha256(b"snapshot"),
                Digest::sha256(b"artifact"),
                1,
                2,
                &[1.5],
            )
            .is_err()
        );
    }

    #[test]
    fn etopo_centre_index_binds_sample_bits_to_exact_routed_centres() {
        let snapshot = Digest::sha256(b"snapshot");
        let artifact = Digest::sha256(b"artifact");
        let mut values = vec![0.0_f32; 180 * 360];
        values[0] = 1.5;
        let encoded = encode_etopo_centre_index(60, 10, snapshot, artifact, 180, 360, &values)
            .expect("encode one-degree ETOPO centre index");
        assert_eq!(
            encoded.len(),
            ETOPO_CENTRE_INDEX_HEADER_LENGTH + values.len() * ETOPO_CENTRE_INDEX_RECORD_LENGTH
        );
        assert_eq!(&encoded[0..8], ETOPO_CENTRE_INDEX_MAGIC);
        assert_eq!(
            u16::from_le_bytes(encoded[8..10].try_into().expect("schema bytes")),
            ETOPO_CENTRE_INDEX_SCHEMA_VERSION
        );
        assert_eq!(encoded[12], 10);
        assert_eq!(&encoded[16..48], snapshot.as_bytes());
        assert_eq!(&encoded[48..80], artifact.as_bytes());
        assert_eq!(
            u64::from_be_bytes(encoded[88..96].try_into().expect("S2 cell bytes")),
            0xa555_5500_0000_0000
        );
        assert_eq!(
            u32::from_le_bytes(encoded[96..100].try_into().expect("sample bytes")),
            1.5_f32.to_bits()
        );
    }

    #[test]
    fn etopo_centre_summary_revalidates_every_source_route_and_preserves_fixed_point_stats() {
        let root = temporary_root("etopo-centre-summary");
        let input = root.join("centres.bin");
        let output = root.join("summary.bin");
        let mut values = vec![1.5_f32; 180 * 360];
        values[0] = -2.25;
        fs::write(
            &input,
            encode_etopo_centre_index(
                60,
                10,
                Digest::sha256(b"snapshot"),
                Digest::sha256(b"artifact"),
                180,
                360,
                &values,
            )
            .expect("encode centre index"),
        )
        .expect("write centre index");

        derive_etopo_centre_summary(&input, 0, &output).expect("derive source-centre summary");
        let bytes = fs::read(&output).expect("read summary");
        assert_eq!(&bytes[..8], ETOPO_CENTRE_SUMMARY_MAGIC);
        assert_eq!(
            u16::from_le_bytes(bytes[8..10].try_into().expect("summary schema")),
            ETOPO_CENTRE_SUMMARY_SCHEMA_VERSION
        );
        assert_eq!(bytes[12], 10);
        assert_eq!(bytes[13], 0);
        let cells = u32::from_le_bytes(bytes[112..116].try_into().expect("summary cell count"));
        assert!(cells >= 6);
        let records = &bytes[ETOPO_CENTRE_SUMMARY_HEADER_LENGTH..];
        assert_eq!(
            records.len(),
            usize::try_from(cells).expect("cell count") * 40
        );
        let source_samples = records
            .chunks_exact(ETOPO_CENTRE_SUMMARY_RECORD_LENGTH)
            .map(|record| u64::from_le_bytes(record[8..16].try_into().expect("sample count")))
            .sum::<u64>();
        assert_eq!(source_samples, 64_800);
        assert_eq!(
            u64::from_le_bytes(bytes[116..124].try_into().expect("header sample count")),
            64_800
        );

        let mut tampered = fs::read(&input).expect("read centre index");
        tampered[88] ^= 1;
        let tampered_input = root.join("tampered-centres.bin");
        fs::write(&tampered_input, tampered).expect("write tampered index");
        assert!(
            derive_etopo_centre_summary(&tampered_input, 0, &root.join("rejected.bin")).is_err()
        );
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn etopo_float_bits_round_to_signed_millimetres_without_host_float_math() {
        assert_eq!(
            f32_bits_to_rounded_millimetres(1.5_f32.to_bits()).expect("finite f32"),
            1_500
        );
        assert_eq!(
            f32_bits_to_rounded_millimetres((-2.25_f32).to_bits()).expect("finite f32"),
            -2_250
        );
        assert_eq!(
            f32_bits_to_rounded_millimetres(0.0005_f32.to_bits()).expect("finite f32"),
            1
        );
        assert_eq!(
            f32_bits_to_rounded_millimetres((-0.0005_f32).to_bits()).expect("finite f32"),
            -1
        );
        assert_eq!(
            f32_bits_to_rounded_millimetres(1).expect("subnormal f32"),
            0
        );
        assert!(f32_bits_to_rounded_millimetres(f32::NAN.to_bits()).is_err());
        assert!(f32_bits_to_rounded_millimetres(f32::INFINITY.to_bits()).is_err());
    }

    #[test]
    fn etopo_cell_centres_keep_the_pinned_area_raster_lattice() {
        let first_support = etopo_cell_support(0, 0).expect("first cell support");
        let first = first_support.centre;
        assert_eq!(
            (
                first.latitude_half_arcseconds(),
                first.longitude_half_arcseconds()
            ),
            (-647_940, -1_295_940)
        );
        assert_eq!(
            (
                first_support.south_boundary_half_arcseconds,
                first_support.north_boundary_half_arcseconds,
                first_support.west_boundary_half_arcseconds,
                first_support.east_boundary_half_arcseconds,
            ),
            (-648_000, -647_880, -1_296_000, -1_295_880)
        );
        let last_support = etopo_cell_support(10_799, 21_599).expect("last cell support");
        let last = last_support.centre;
        assert_eq!(
            (
                last.latitude_half_arcseconds(),
                last.longitude_half_arcseconds()
            ),
            (647_940, 1_295_940)
        );
        assert_eq!(
            (
                last_support.south_boundary_half_arcseconds,
                last_support.north_boundary_half_arcseconds,
                last_support.west_boundary_half_arcseconds,
                last_support.east_boundary_half_arcseconds,
            ),
            (647_880, 648_000, 1_295_880, 1_296_000)
        );
        let seam_left = etopo_cell_support(5_400, 0).expect("western seam support");
        let seam_right = etopo_cell_support(5_400, 21_599).expect("eastern seam support");
        assert_eq!(seam_left.west_boundary_half_arcseconds, -1_296_000);
        assert_eq!(seam_right.east_boundary_half_arcseconds, 1_296_000);
        assert!(etopo_cell_support(10_800, 0).is_err());
        assert!(etopo_cell_support(0, 21_600).is_err());
    }

    #[test]
    fn coordinate_lattice_quantization_rejects_off_grid_values() {
        assert_eq!(
            f64_to_half_arcseconds(-647_940.0 / 7_200.0).expect("valid latitude"),
            -647_940
        );
        assert_eq!(
            f64_to_half_arcseconds(1_295_940.0 / 7_200.0).expect("valid longitude"),
            1_295_940
        );
        assert!(f64_to_half_arcseconds(0.000_000_1).is_err());
        assert!(f64_to_half_arcseconds(f64::NAN).is_err());
    }

    #[test]
    fn chelsa_cell_addresses_are_zero_based_and_bounded_by_the_pinned_grid() {
        validate_chelsa_cell_address(0, 0).expect("first CHELSA cell");
        validate_chelsa_cell_address(CHELSA_LATITUDE_CELLS - 1, CHELSA_LONGITUDE_CELLS - 1)
            .expect("last CHELSA cell");
        assert!(validate_chelsa_cell_address(CHELSA_LATITUDE_CELLS, 0).is_err());
        assert!(validate_chelsa_cell_address(0, CHELSA_LONGITUDE_CELLS).is_err());
    }

    #[test]
    fn nearest_chelsa_axis_cell_uses_exact_lattice_and_lower_index_ties() {
        let axis = [-100, 0, 100];
        assert_eq!(nearest_sorted_e7_index(&axis, -100).expect("first"), 0);
        assert_eq!(nearest_sorted_e7_index(&axis, 100).expect("last"), 2);
        assert_eq!(nearest_sorted_e7_index(&axis, 49).expect("near lower"), 1);
        assert_eq!(nearest_sorted_e7_index(&axis, 50).expect("tie"), 1);
        assert!(nearest_sorted_e7_index(&axis, -101).is_err());
        assert!(nearest_sorted_e7_index(&axis, 101).is_err());
    }

    #[test]
    fn chelsa_documented_tas_transform_uses_integer_millicelsius() {
        assert_eq!(
            chelsa_raw_tas_to_millicelsius(2992.0).expect("documented conversion"),
            26_050
        );
        assert!(chelsa_raw_tas_to_millicelsius(2992.5).is_err());
    }

    #[test]
    fn shapefile_ring_uses_even_odd_membership() {
        let mut points = Vec::new();
        for (x, y) in [
            (0.0_f64, 0.0_f64),
            (2.0_f64, 0.0_f64),
            (2.0_f64, 2.0_f64),
            (0.0_f64, 2.0_f64),
            (0.0_f64, 0.0_f64),
        ] {
            points.extend_from_slice(&x.to_le_bytes());
            points.extend_from_slice(&y.to_le_bytes());
        }
        assert!(point_in_shapefile_ring(&points, 0, 5, 10_000_000, 10_000_000).expect("inside"));
        assert!(!point_in_shapefile_ring(&points, 0, 5, 30_000_000, 10_000_000).expect("outside"));
    }

    #[test]
    fn prepared_land_index_preserves_even_odd_membership_across_buckets() {
        // The two vertical sides of a 0°..2° square cross both 0° and 1° buckets;
        // the horizontal sides intentionally do not enter a horizontal-ray index.
        let mut buckets = vec![Vec::new(); NATURAL_EARTH_LATITUDE_BUCKETS];
        for bucket_edges in buckets.iter_mut().take(93).skip(90) {
            bucket_edges.extend([
                NaturalEarthEdge {
                    current_x: 20_000_000,
                    current_y: 20_000_000,
                    previous_x: 20_000_000,
                    previous_y: 0,
                },
                NaturalEarthEdge {
                    current_x: 0,
                    current_y: 0,
                    previous_x: 0,
                    previous_y: 20_000_000,
                },
            ]);
        }
        let prepared = PreparedNaturalEarthLand {
            edges_by_latitude_bucket: buckets,
        };
        assert!(prepared.contains_point(10_000_000, 10_000_000));
        assert!(!prepared.contains_point(30_000_000, 10_000_000));
        assert!(prepared.contains_point(10_000_000, 19_000_000));
        assert!(!prepared.contains_point(10_000_000, 30_000_000));
    }

    #[test]
    fn shapefile_ieee754_coordinates_round_to_the_e7_lattice_without_host_float_math() {
        assert_eq!(
            ieee754_degrees_bits_to_e7((-93.125_f64).to_bits()).expect("finite coordinate"),
            -931_250_000
        );
        assert_eq!(
            ieee754_degrees_bits_to_e7(0.5_f64.to_bits()).expect("finite coordinate"),
            5_000_000
        );
        assert!(ieee754_degrees_bits_to_e7(f64::NAN.to_bits()).is_err());
    }

    #[test]
    fn etopo_one_point_quadrature_is_the_exact_source_centre_route() {
        let support = etopo_cell_support(5_400, 10_800).expect("ETOP0 source cell");
        let expected = route_half_arcsecond_to_s2(support.centre, 10).expect("route centre");
        let quadrature =
            etopo_cell_quadrature(5_400, 10_800, 10, 1).expect("one-point source quadrature");
        assert_eq!(
            quadrature,
            std::collections::BTreeMap::from([(expected, 1)])
        );
    }

    #[test]
    fn etopo_quadrature_uses_only_interior_exact_lattice_points() {
        let quadrature =
            etopo_cell_quadrature(5_400, 10_800, 10, 4).expect("four-by-four source quadrature");
        assert_eq!(quadrature.values().copied().sum::<u32>(), 16);
        assert!(quadrature.values().all(|count| *count > 0));
        assert!(etopo_cell_quadrature(5_400, 10_800, 10, 7).is_err());
    }

    #[test]
    fn unordered_full_batch_accumulation_has_the_same_sorted_summary() {
        let samples = [
            (0, 0, (-10.25_f32).to_bits()),
            (1, 1, 0.5_f32.to_bits()),
            (5_400, 10_800, 123.75_f32.to_bits()),
            (
                ETOPO_LATITUDE_CELLS as u32 - 1,
                ETOPO_LONGITUDE_CELLS as u32 - 1,
                4.0_f32.to_bits(),
            ),
        ];
        let mut ordered = BTreeMap::new();
        let mut unordered = HashMap::new();
        for (row, column, value_bits) in samples {
            accumulate_etopo_cell_quadrature(&mut ordered, row, column, value_bits, 10, 4)
                .expect("ordered accumulation");
            accumulate_etopo_cell_quadrature_unordered(
                &mut unordered,
                row,
                column,
                value_bits,
                10,
                4,
            )
            .expect("unordered accumulation");
        }
        assert_eq!(unordered.into_iter().collect::<BTreeMap<_, _>>(), ordered);
    }

    #[test]
    fn packs_complete_quadrature_summaries_into_a_canonical_terrain_tile() {
        let container: S2CellId = "1000010000000000".parse().expect("valid L10 container");
        let mut summaries = std::collections::BTreeMap::new();
        for (index, child) in container
            .children()
            .expect("children")
            .into_iter()
            .enumerate()
        {
            let mut stats = EtopoCentreSummaryStats::default();
            stats
                .add_weighted(i64::try_from(index).expect("index fits") * 1_000, 16)
                .expect("weighted support");
            summaries.insert(child, stats);
        }
        let tile = pack_etopo_terrain_tile(
            "bedrock-relief",
            Digest::sha256(b"snapshot"),
            Digest::sha256(b"artifact"),
            4,
            container,
            11,
            &summaries,
        )
        .expect("complete terrain tile");
        assert_eq!(tile.cells.len(), 4);
        assert!(tile.cells.iter().all(|cell| cell.support_samples == 16));
        assert_eq!(tile.cells[3].mean_millimetres, 3_000);
        summaries.pop_first();
        assert!(
            pack_etopo_terrain_tile(
                "bedrock-relief",
                Digest::sha256(b"snapshot"),
                Digest::sha256(b"artifact"),
                4,
                container,
                11,
                &summaries,
            )
            .is_err()
        );
    }

    #[test]
    fn global_s2_container_enumeration_is_complete_and_canonical() {
        let cells = global_s2_cells_at_level(1).expect("global L1 cells");
        assert_eq!(cells.len(), 24);
        assert!(cells.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(cells.iter().filter(|cell| cell.face() == 0).count(), 4);
    }

    #[test]
    fn miniature_terrain_layer_writes_and_reloads_every_packed_tile() {
        let root = temporary_root("terrain-layer");
        let mut summaries = std::collections::BTreeMap::new();
        for cell in global_s2_cells_at_level(1).expect("global target cells") {
            let mut stats = EtopoCentreSummaryStats::default();
            stats.add_weighted(42, 4).expect("support");
            summaries.insert(cell, stats);
        }
        let (root_path, root_bytes) = write_packed_etopo_terrain_layer(
            &root,
            EtopoTerrainPackingProfile {
                layer_id: "bedrock-relief",
                source_snapshot_digest: Digest::sha256(b"snapshot"),
                source_artifact_digest: Digest::sha256(b"artifact"),
                points_per_axis: 4,
                container_s2_level: 0,
                target_s2_level: 1,
            },
            &summaries,
        )
        .expect("write miniature terrain layer");
        let index = TileTreeIndex::from_canonical_slice(&root_bytes).expect("root index");
        assert_eq!(index.entries.len(), 6);
        assert_eq!(
            fs::read(root.join(&root_path)).expect("root bytes"),
            root_bytes
        );
        inspect_etopo_terrain_layer(&root, "bedrock-relief", 0, 1, 4)
            .expect("independently inspect miniature terrain layer");
        for entry in index.entries {
            let tile = PackedScalarTerrainTile::from_canonical_slice(
                &fs::read(root.join(entry.artifact.path)).expect("tile bytes"),
            )
            .expect("packed terrain tile");
            assert_eq!(tile.cells.len(), 4);
            assert!(tile.cells.iter().all(|cell| cell.mean_millimetres == 42));
        }
        fs::remove_dir_all(root).expect("remove miniature terrain layer");
    }

    #[test]
    fn miniature_land_reference_layer_writes_and_reloads_every_packed_tile() {
        let root = temporary_root("land-reference-layer");
        let source = minimal_null_polygon_shapefile();
        let prepared =
            PreparedNaturalEarthLand::from_shapefile(&source).expect("validated empty source");
        let (root_path, root_bytes) = write_packed_natural_earth_land_reference_layer(
            &root,
            "land-reference",
            Digest::sha256(b"snapshot"),
            Digest::sha256(b"artifact"),
            &prepared,
            0,
            1,
        )
        .expect("write miniature land-reference layer");
        let index = TileTreeIndex::from_canonical_slice(&root_bytes).expect("root index");
        assert_eq!(index.entries.len(), 6);
        assert_eq!(
            fs::read(root.join(&root_path)).expect("root bytes"),
            root_bytes
        );
        inspect_natural_earth_land_reference_layer(&root, "land-reference", 0, 1)
            .expect("independently inspect miniature land-reference layer");
        // Replaying an interrupted writer against an already-complete hidden tree
        // must verify and reuse its exact canonical artifacts rather than overwrite.
        let (resumed_root_path, resumed_root_bytes) =
            write_packed_natural_earth_land_reference_layer(
                &root,
                "land-reference",
                Digest::sha256(b"snapshot"),
                Digest::sha256(b"artifact"),
                &prepared,
                0,
                1,
            )
            .expect("resume miniature land-reference layer");
        assert_eq!(resumed_root_path, root_path);
        assert_eq!(resumed_root_bytes, root_bytes);
        for entry in index.entries {
            let tile = PackedBooleanFieldTile::from_canonical_slice(
                &fs::read(root.join(entry.artifact.path)).expect("tile bytes"),
            )
            .expect("packed Boolean tile");
            assert_eq!(tile.cells.len(), 4);
            assert!(tile.cells.iter().all(|cell| cell.true_samples == 0));
        }
        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn terrain_release_staging_is_hidden_until_atomic_rename() {
        let root = temporary_root("terrain-staging");
        let output = root.join("release");
        let staging = prepare_terrain_layer_staging_directory(&output).expect("stage release");
        assert_eq!(staging, root.join(".release.staging"));
        assert!(!output.exists());
        assert!(staging.is_dir());
        fs::write(staging.join("complete"), b"complete").expect("write staged marker");
        fs::rename(&staging, &output).expect("atomically publish staged release");
        assert!(output.join("complete").is_file());
        assert!(prepare_terrain_layer_staging_directory(&output).is_err());
        fs::remove_dir_all(root).expect("remove terrain staging root");
    }
}
