use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use netcdf_reader::NcFile;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use world_data::{SourceSnapshotArtifact, SourceSnapshotManifest, WorldDataBundle};
use world_data_filesystem::{
    verify_release_artifacts, verify_source_snapshot_artifact, verify_source_snapshot_artifacts,
};
use world_domain::{Digest, WorldConfiguration};

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
        },
    }
}

#[derive(Serialize)]
struct EtopoInspection {
    inspection_schema_version: u16,
    source_snapshot_id: String,
    source_snapshot_digest: Digest,
    artifact_path: String,
    artifact_hash: Digest,
    artifact_byte_length: u64,
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
            inspection_schema_version: 1,
            source_snapshot_id: snapshot.snapshot_id,
            source_snapshot_digest,
            artifact_path: artifact.artifact_path.clone(),
            artifact_hash: artifact.content_hash,
            artifact_byte_length: artifact.byte_length,
            variables,
        })?
    );
    Ok(())
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
}
