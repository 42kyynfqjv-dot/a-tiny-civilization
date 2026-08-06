use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use netcdf_reader::{NcFile, NcSliceInfo, NcSliceInfoElem};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use world_data::{SourceSnapshotArtifact, SourceSnapshotManifest, WorldDataBundle};
use world_data_filesystem::{
    verify_release_artifacts, verify_source_snapshot_artifact, verify_source_snapshot_artifacts,
};
use world_domain::{
    Digest, GeographicCoordinateE7, GeographicCoordinateHalfArcsecond, MAX_S2_LEVEL, S2CellId,
    WorldConfiguration, route_geographic_to_s2, route_half_arcsecond_to_s2,
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
    /// Inspect the pinned ETOPO NetCDF schema through the portable Rust reader.
    Etopo {
        #[arg(long)]
        source_snapshot: PathBuf,
        #[arg(long)]
        artifact_root: PathBuf,
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
            InspectCommand::Etopo {
                source_snapshot,
                artifact_root,
            } => inspect_etopo(&source_snapshot, &artifact_root),
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

#[derive(Clone, Copy, Debug, Default)]
struct EtopoCentreSummaryStats {
    samples: u64,
    total_millimetres: i64,
    minimum_millimetres: i64,
    maximum_millimetres: i64,
}

impl EtopoCentreSummaryStats {
    fn add(&mut self, millimetres: i64) -> Result<()> {
        if self.samples == 0 {
            self.minimum_millimetres = millimetres;
            self.maximum_millimetres = millimetres;
        } else {
            self.minimum_millimetres = self.minimum_millimetres.min(millimetres);
            self.maximum_millimetres = self.maximum_millimetres.max(millimetres);
        }
        self.samples = self
            .samples
            .checked_add(1)
            .context("ETOPO summary sample overflow")?;
        self.total_millimetres = self
            .total_millimetres
            .checked_add(millimetres)
            .context("ETOPO summary total overflow")?;
        Ok(())
    }

    fn mean_millimetres(self) -> i64 {
        debug_assert!(self.samples > 0);
        round_divide_i64(self.total_millimetres, self.samples as i64)
    }
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
struct EtopoVariableInspection {
    name: String,
    shape: Vec<u64>,
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
}
