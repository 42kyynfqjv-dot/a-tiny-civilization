//! Safe, exhaustive filesystem verification for scientific world-data releases.

use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use sha2::{Digest as _, Sha256};
use world_data::{
    BundleArtifact, BundleArtifactKind, DataLayerStorage, PACKED_BOOLEAN_FIELD_TILE_MEDIA_TYPE,
    PACKED_SCALAR_FIELD_TILE_MEDIA_TYPE, PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE,
    PackedBooleanFieldTile, PackedScalarFieldTile, PackedScalarTerrainTile,
    ProvisionalArtifactReference, ProvisionalWorldComposition, SourceSnapshotArtifact,
    SourceSnapshotManifest, TileTreeEntry, TileTreeEntryKind, TileTreeIndex, TileTreeReference,
    WorldDataBundle,
};
use world_domain::{Digest, S2CellId};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VerificationStats {
    pub artifacts: u64,
    pub bytes: u64,
    pub tile_indexes: u64,
    pub tiles: u64,
}

impl VerificationStats {
    fn add_artifact(&mut self, byte_length: u64) -> Result<()> {
        self.artifacts = self
            .artifacts
            .checked_add(1)
            .context("verified artifact count overflow")?;
        self.bytes = self
            .bytes
            .checked_add(byte_length)
            .context("verified artifact byte total overflow")?;
        Ok(())
    }

    fn add_tree(&mut self, tree: Self) -> Result<()> {
        self.artifacts = self
            .artifacts
            .checked_add(tree.artifacts)
            .context("verified artifact count overflow")?;
        self.bytes = self
            .bytes
            .checked_add(tree.bytes)
            .context("verified artifact byte total overflow")?;
        self.tile_indexes = self
            .tile_indexes
            .checked_add(tree.tile_indexes)
            .context("verified tile-index count overflow")?;
        self.tiles = self
            .tiles
            .checked_add(tree.tiles)
            .context("verified tile count overflow")?;
        Ok(())
    }
}

struct SafeArtifactRoot {
    canonical_root: PathBuf,
}

impl SafeArtifactRoot {
    fn new(root: &Path) -> Result<Self> {
        let canonical_root = root
            .canonicalize()
            .with_context(|| format!("failed to resolve artifact root {}", root.display()))?;
        Ok(Self { canonical_root })
    }

    fn resolve_file(&self, relative_path: &str) -> Result<PathBuf> {
        let components = Path::new(relative_path).components().collect::<Vec<_>>();
        let mut unresolved = self.canonical_root.clone();
        for (index, component) in components.iter().enumerate() {
            let Component::Normal(part) = component else {
                bail!("scientific artifact path {relative_path:?} is not portable");
            };
            unresolved.push(part);
            let metadata = fs::symlink_metadata(&unresolved).with_context(|| {
                format!(
                    "failed to inspect scientific artifact {}",
                    unresolved.display()
                )
            })?;
            if metadata.file_type().is_symlink() {
                bail!(
                    "scientific artifact path component {} is a symbolic link",
                    unresolved.display()
                );
            }
            let is_leaf = index + 1 == components.len();
            if is_leaf && !metadata.is_file() {
                bail!("scientific artifact {} is not a file", unresolved.display());
            }
            if !is_leaf && !metadata.is_dir() {
                bail!(
                    "scientific artifact parent {} is not a directory",
                    unresolved.display()
                );
            }
        }
        let resolved = unresolved.canonicalize().with_context(|| {
            format!(
                "failed to resolve scientific artifact {}",
                unresolved.display()
            )
        })?;
        if !resolved.starts_with(&self.canonical_root) {
            bail!(
                "scientific artifact {} resolves outside {}",
                unresolved.display(),
                self.canonical_root.display()
            );
        }
        Ok(resolved)
    }

    fn read(&self, relative_path: &str) -> Result<Vec<u8>> {
        let resolved = self.resolve_file(relative_path)?;
        fs::read(&resolved)
            .with_context(|| format!("failed to read scientific artifact {}", resolved.display()))
    }
}

/// Verify every exact upstream artifact in a source-snapshot manifest without loading
/// complete files into memory.
pub fn verify_source_snapshot_artifacts(
    snapshot: &SourceSnapshotManifest,
    artifact_root: &Path,
) -> Result<VerificationStats> {
    snapshot.validate().context("source snapshot is invalid")?;
    let artifact_root = SafeArtifactRoot::new(artifact_root)?;
    let mut stats = VerificationStats::default();
    for artifact in &snapshot.artifacts {
        verify_source_artifact(&artifact_root, artifact)?;
        stats.add_artifact(artifact.byte_length)?;
    }
    Ok(stats)
}

/// Verify one already-validated source artifact beneath a safe root.
pub fn verify_source_snapshot_artifact(
    artifact: &SourceSnapshotArtifact,
    artifact_root: &Path,
) -> Result<()> {
    artifact.validate().context("source artifact is invalid")?;
    verify_source_artifact(&SafeArtifactRoot::new(artifact_root)?, artifact)
}

fn verify_source_artifact(
    artifact_root: &SafeArtifactRoot,
    artifact: &SourceSnapshotArtifact,
) -> Result<()> {
    let path = artifact_root.resolve_file(&artifact.artifact_path)?;
    let mut file = fs::File::open(&path)
        .with_context(|| format!("failed to open source artifact {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut actual_length = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read source artifact {}", path.display()))?;
        if count == 0 {
            break;
        }
        actual_length = actual_length
            .checked_add(u64::try_from(count).context("source read length overflow")?)
            .context("source artifact byte count overflow")?;
        if actual_length > artifact.byte_length {
            bail!(
                "source artifact {:?} exceeds expected length {}",
                artifact.artifact_path,
                artifact.byte_length
            );
        }
        hasher.update(&buffer[..count]);
    }
    let actual_digest = Digest::from_bytes(hasher.finalize().into());
    artifact
        .expected_artifact()
        .verify_observation(actual_length, actual_digest)
        .with_context(|| format!("source artifact {:?} is invalid", artifact.artifact_path))
}

/// Load and validate an exact canonical provisional-world composition.
pub fn load_provisional_world_composition(path: &Path) -> Result<ProvisionalWorldComposition> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read provisional composition {}", path.display()))?;
    ProvisionalWorldComposition::from_canonical_slice(&bytes)
        .with_context(|| format!("provisional composition {} is invalid", path.display()))
}

/// Verify every artifact referenced by an already-loaded provisional-world composition.
///
/// Verification is local and streaming. Every path component beneath `artifact_root` must be a
/// real directory or file rather than a symbolic link, and every file must exactly match its
/// declared byte length and SHA-256 digest.
pub fn verify_provisional_world_artifacts(
    composition: &ProvisionalWorldComposition,
    artifact_root: &Path,
) -> Result<VerificationStats> {
    composition
        .validate()
        .context("provisional composition is invalid")?;
    let artifact_root = SafeArtifactRoot::new(artifact_root).with_context(|| {
        format!(
            "failed to resolve provisional artifact root {}",
            artifact_root.display()
        )
    })?;
    let releases = composition
        .earth_layers
        .iter()
        .map(|layer| &layer.release)
        .chain(
            composition
                .world_components
                .iter()
                .map(|component| &component.release),
        );
    let mut stats = VerificationStats::default();
    for release in releases {
        verify_provisional_artifact(&artifact_root, release)?;
        stats.add_artifact(release.byte_length)?;
    }
    Ok(stats)
}

fn verify_provisional_artifact(
    artifact_root: &SafeArtifactRoot,
    release: &ProvisionalArtifactReference,
) -> Result<()> {
    let path = artifact_root
        .resolve_file(&release.artifact_path)
        .with_context(|| {
            format!(
                "resolve provisional artifact {}",
                artifact_root
                    .canonical_root
                    .join(&release.artifact_path)
                    .display()
            )
        })?;
    let mut file = fs::File::open(&path)
        .with_context(|| format!("verify provisional artifact {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut actual_length = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("verify provisional artifact {}", path.display()))?;
        if count == 0 {
            break;
        }
        actual_length = actual_length
            .checked_add(u64::try_from(count).context("provisional read length overflow")?)
            .context("provisional artifact byte count overflow")?;
        if actual_length > release.byte_length {
            bail!(
                "provisional artifact differs from its composition reference: {}",
                path.display()
            );
        }
        hasher.update(&buffer[..count]);
    }
    let actual_hash = Digest::from_bytes(hasher.finalize().into());
    if actual_length != release.byte_length || actual_hash != release.content_hash {
        bail!(
            "provisional artifact differs from its composition reference: {}",
            path.display()
        );
    }
    Ok(())
}

/// Verify every retained source, bounded raster, tile index, and tile under `artifact_root`.
///
/// This performs no network access. A successful result means the reachable release is
/// complete, canonical, content-addressed, and spatially well-formed at verification time.
pub fn verify_release_artifacts(
    bundle: &WorldDataBundle,
    artifact_root: &Path,
) -> Result<VerificationStats> {
    bundle.validate().context("world-data bundle is invalid")?;
    let artifact_root = SafeArtifactRoot::new(artifact_root)?;
    let mut seen_paths = BTreeSet::new();
    let mut stats = VerificationStats::default();

    for artifact in bundle.artifacts()? {
        if artifact.kind == BundleArtifactKind::TileTreeIndex {
            continue;
        }
        verify_artifact(&artifact, &mut |path| artifact_root.read(path))?;
        if !seen_paths.insert(artifact.relative_path.to_owned()) {
            bail!(
                "artifact path {:?} is referenced twice",
                artifact.relative_path
            );
        }
        stats.add_artifact(artifact.byte_length)?;
    }

    for layer in &bundle.layers {
        if let DataLayerStorage::FullEarthTileTree { tile_tree } = &layer.storage {
            let tree_stats =
                verify_tile_tree(&layer.layer_id, tile_tree, &mut seen_paths, |path| {
                    artifact_root.read(path)
                })?;
            stats.add_tree(tree_stats)?;
        }
    }
    Ok(stats)
}

fn verify_artifact<F>(artifact: &BundleArtifact<'_>, read: &mut F) -> Result<Vec<u8>>
where
    F: FnMut(&str) -> Result<Vec<u8>>,
{
    let bytes = read(artifact.relative_path)?;
    artifact
        .verify_bytes(&bytes)
        .with_context(|| format!("bundle artifact {:?} is invalid", artifact.relative_path))?;
    Ok(bytes)
}

fn verify_tile_tree<F>(
    layer_id: &str,
    tree: &TileTreeReference,
    seen_paths: &mut BTreeSet<String>,
    mut read: F,
) -> Result<VerificationStats>
where
    F: FnMut(&str) -> Result<Vec<u8>>,
{
    let root = BundleArtifact {
        kind: BundleArtifactKind::TileTreeIndex,
        relative_path: &tree.root_index_path,
        content_hash: tree.root_index_hash,
        byte_length: tree.root_index_byte_length,
    };
    if !seen_paths.insert(tree.root_index_path.clone()) {
        bail!(
            "tile-tree root path {:?} is referenced twice",
            tree.root_index_path
        );
    }
    let root_bytes = verify_artifact(&root, &mut read)?;
    let root_index = TileTreeIndex::from_canonical_slice(&root_bytes)
        .with_context(|| format!("tile-tree root {:?} is invalid", tree.root_index_path))?;
    root_index.validate_for_tree(layer_id, tree)?;

    let maximum_indexes = tree
        .leaf_tile_count
        .checked_mul(u64::from(tree.maximum_s2_level) + 2)
        .context("tile-tree traversal bound overflow")?;
    let mut stats = VerificationStats {
        artifacts: 1,
        bytes: tree.root_index_byte_length,
        tile_indexes: 1,
        tiles: 0,
    };
    let mut pending = root_index.entries.into_iter().collect::<VecDeque<_>>();
    let mut index_scopes = BTreeSet::new();
    let mut tile_cells = BTreeSet::new();

    while let Some(entry) = pending.pop_front() {
        let key = (entry.s2_level, entry.s2_cell_id.clone());
        match entry.kind {
            TileTreeEntryKind::Index if !index_scopes.insert(key.clone()) => {
                bail!(
                    "tile tree repeats index scope {} at level {}",
                    entry.s2_cell_id,
                    entry.s2_level
                );
            }
            TileTreeEntryKind::Tile if !tile_cells.insert(key) => {
                bail!(
                    "tile tree repeats tile {} at level {}",
                    entry.s2_cell_id,
                    entry.s2_level
                );
            }
            _ => {}
        }
        if !seen_paths.insert(entry.artifact.path.clone()) {
            bail!(
                "tile tree repeats or cycles through artifact path {:?}",
                entry.artifact.path
            );
        }

        let artifact = entry.artifact();
        let artifact_bytes = verify_artifact(&artifact, &mut read)?;
        stats.add_artifact(artifact.byte_length)?;

        match entry.kind {
            TileTreeEntryKind::Tile => {
                validate_known_tile_payload(layer_id, &entry, &artifact_bytes)?;
                stats.tiles = stats
                    .tiles
                    .checked_add(1)
                    .context("verified tile count overflow")?;
                if stats.tiles > tree.leaf_tile_count {
                    bail!(
                        "tile tree contains more than its declared {} leaves",
                        tree.leaf_tile_count
                    );
                }
            }
            TileTreeEntryKind::Index => {
                stats.tile_indexes = stats
                    .tile_indexes
                    .checked_add(1)
                    .context("verified tile-index count overflow")?;
                if stats.tile_indexes > maximum_indexes {
                    bail!(
                        "tile tree exceeds its structural bound of {maximum_indexes} index nodes"
                    );
                }
                let child = TileTreeIndex::from_canonical_slice(&artifact_bytes)
                    .with_context(|| format!("tile index {:?} is invalid", entry.artifact.path))?;
                child.validate_for_tree(layer_id, tree)?;
                for descendant in &child.entries {
                    if !entry_contains(&entry, descendant)? {
                        bail!(
                            "tile index {:?} contains S2 cell {} level {} outside parent {} level {}",
                            entry.artifact.path,
                            descendant.s2_cell_id,
                            descendant.s2_level,
                            entry.s2_cell_id,
                            entry.s2_level
                        );
                    }
                }
                pending.extend(child.entries);
            }
        }
    }

    if stats.tiles != tree.leaf_tile_count {
        bail!(
            "tile tree declares {} leaves but contains {}",
            tree.leaf_tile_count,
            stats.tiles
        );
    }
    Ok(stats)
}

fn validate_known_tile_payload(layer_id: &str, entry: &TileTreeEntry, bytes: &[u8]) -> Result<()> {
    if entry.artifact.media_type == PACKED_BOOLEAN_FIELD_TILE_MEDIA_TYPE {
        let tile = PackedBooleanFieldTile::from_canonical_slice(bytes).with_context(|| {
            format!(
                "packed Boolean field tile {:?} is invalid",
                entry.artifact.path
            )
        })?;
        if tile.layer_id != layer_id {
            bail!(
                "packed Boolean field tile {:?} declares layer {:?}, expected {:?}",
                entry.artifact.path,
                tile.layer_id,
                layer_id
            );
        }
        let declared_container = entry
            .s2_cell_id
            .parse::<S2CellId>()
            .with_context(|| format!("invalid tile S2 CellId {}", entry.s2_cell_id))?;
        if tile.container_s2_cell_id != declared_container {
            bail!(
                "packed Boolean field tile {:?} declares container {}, expected {}",
                entry.artifact.path,
                tile.container_s2_cell_id,
                declared_container
            );
        }
        return Ok(());
    }
    if entry.artifact.media_type == PACKED_SCALAR_FIELD_TILE_MEDIA_TYPE {
        let tile = PackedScalarFieldTile::from_canonical_slice(bytes).with_context(|| {
            format!(
                "packed scalar field tile {:?} is invalid",
                entry.artifact.path
            )
        })?;
        if tile.layer_id != layer_id {
            bail!(
                "packed scalar field tile {:?} declares layer {:?}, expected {:?}",
                entry.artifact.path,
                tile.layer_id,
                layer_id
            );
        }
        let declared_container = entry
            .s2_cell_id
            .parse::<S2CellId>()
            .with_context(|| format!("invalid tile S2 CellId {}", entry.s2_cell_id))?;
        if tile.container_s2_cell_id != declared_container {
            bail!(
                "packed scalar field tile {:?} declares container {}, expected {}",
                entry.artifact.path,
                tile.container_s2_cell_id,
                declared_container
            );
        }
        return Ok(());
    }
    if entry.artifact.media_type != PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE {
        return Ok(());
    }
    let tile = PackedScalarTerrainTile::from_canonical_slice(bytes).with_context(|| {
        format!(
            "packed scalar terrain tile {:?} is invalid",
            entry.artifact.path
        )
    })?;
    if tile.layer_id != layer_id {
        bail!(
            "packed scalar terrain tile {:?} declares layer {:?}, expected {:?}",
            entry.artifact.path,
            tile.layer_id,
            layer_id
        );
    }
    let declared_container = entry
        .s2_cell_id
        .parse::<S2CellId>()
        .with_context(|| format!("invalid tile S2 CellId {}", entry.s2_cell_id))?;
    if tile.container_s2_cell_id != declared_container {
        bail!(
            "packed scalar terrain tile {:?} declares container {}, expected {}",
            entry.artifact.path,
            tile.container_s2_cell_id,
            declared_container
        );
    }
    Ok(())
}

fn entry_contains(parent: &TileTreeEntry, descendant: &TileTreeEntry) -> Result<bool> {
    let parent_id = parent
        .s2_cell_id
        .parse::<S2CellId>()
        .with_context(|| format!("invalid parent S2 CellId {}", parent.s2_cell_id))?;
    let descendant_id = descendant
        .s2_cell_id
        .parse::<S2CellId>()
        .with_context(|| format!("invalid descendant S2 CellId {}", descendant.s2_cell_id))?;
    if descendant.s2_level < parent.s2_level
        || (descendant.kind == TileTreeEntryKind::Index && descendant.s2_level == parent.s2_level)
    {
        return Ok(false);
    }
    Ok(parent_id.contains(descendant_id))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use world_data::{
        DataLayerKind, PROVISIONAL_WORLD_COMPOSITION_SCHEMA_VERSION,
        ProvisionalEarthLayerReference, ProvisionalWorldComponentKind,
        ProvisionalWorldComponentReference, ProvisionalWorldCompositionStatus,
        SourceSnapshotArtifact, SourceSnapshotArtifactRole, TILE_TREE_INDEX_SCHEMA_VERSION,
        TileArtifactReference, TileTreeEntryKind,
    };
    use world_domain::{Digest, EarthResolutionLevels, FullEarthGrid, S2Projection};

    fn temporary_root(label: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};

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

    fn provisional_composition(root: &Path) -> ProvisionalWorldComposition {
        let artifact_directory = root.join("provisional");
        fs::create_dir(&artifact_directory).expect("create provisional fixture directory");
        let release = |artifact_id: String, index: u8| {
            let bytes = vec![index; usize::from(index) + 3];
            let artifact_path = format!("provisional/{artifact_id}.bin");
            fs::write(root.join(&artifact_path), &bytes).expect("write provisional fixture");
            ProvisionalArtifactReference {
                artifact_id,
                artifact_path,
                media_type: "application/octet-stream".to_owned(),
                content_hash: Digest::sha256(&bytes),
                byte_length: u64::try_from(bytes.len()).expect("fixture length fits u64"),
                license_expression: "CC-BY-4.0".to_owned(),
                scientific_scope: "Filesystem verification fixture.".to_owned(),
                limitations: vec!["Not scientifically admitted.".to_owned()],
            }
        };
        let earth_layers = [
            DataLayerKind::Bathymetry,
            DataLayerKind::Climate,
            DataLayerKind::Coastline,
            DataLayerKind::Elevation,
            DataLayerKind::Habitat,
            DataLayerKind::Hydrography,
            DataLayerKind::Soil,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| ProvisionalEarthLayerReference {
            kind,
            release: release(
                format!("earth-layer-{index}"),
                u8::try_from(index + 1).expect("earth fixture index fits u8"),
            ),
        })
        .collect();
        let world_components = [
            ProvisionalWorldComponentKind::CelestialEphemeris,
            ProvisionalWorldComponentKind::FaunaCatalog,
            ProvisionalWorldComponentKind::FaunaTraitEvidence,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| ProvisionalWorldComponentReference {
            kind,
            release: release(
                format!("world-component-{index}"),
                u8::try_from(index + 20).expect("world fixture index fits u8"),
            ),
        })
        .collect();
        ProvisionalWorldComposition {
            composition_schema_version: PROVISIONAL_WORLD_COMPOSITION_SCHEMA_VERSION,
            composition_id: "filesystem-verification-fixture".to_owned(),
            composition_version: "0.1.0".to_owned(),
            status: ProvisionalWorldCompositionStatus::ProvisionalNotScientificallyAdmitted,
            full_earth_grid: FullEarthGrid {
                physics_crs_epsg: 4_978,
                catalog_crs_epsg: 4_979,
                vertical_crs_epsg: 3_855,
                s2_definition_url: "https://s2geometry.io/devguide/s2cell_hierarchy".to_owned(),
                s2_library_revision: "0123456789abcdef".to_owned(),
                s2_definition_hash: Digest::sha256(b"filesystem provisional S2 fixture"),
                s2_projection: S2Projection::Quadratic,
                levels: EarthResolutionLevels {
                    planetary_aggregate: 10,
                    regional_ecology: 14,
                    active_landscape: 18,
                    embodied_patch: 23,
                },
                refinement_policy_version: 1,
            },
            earth_layers,
            world_components,
            coupled_validation_gaps: vec!["Fixture validation is incomplete.".to_owned()],
        }
    }

    fn artifact(path: &str, media_type: &str, bytes: &[u8]) -> TileArtifactReference {
        TileArtifactReference {
            path: path.to_owned(),
            media_type: media_type.to_owned(),
            content_hash: Digest::sha256(bytes),
            byte_length: u64::try_from(bytes.len()).expect("fixture length fits u64"),
        }
    }

    fn entry(
        kind: TileTreeEntryKind,
        cell: &str,
        level: u8,
        path: &str,
        bytes: &[u8],
    ) -> TileTreeEntry {
        TileTreeEntry {
            kind,
            s2_cell_id: cell.to_owned(),
            s2_level: level,
            artifact: artifact(
                path,
                match kind {
                    TileTreeEntryKind::Index => "application/vnd.atinycivilization.tile-index+json",
                    TileTreeEntryKind::Tile => "application/vnd.atinycivilization.tile+i32",
                },
                bytes,
            ),
        }
    }

    fn valid_tree() -> (TileTreeReference, BTreeMap<String, Vec<u8>>) {
        let tile_a = b"tile-a".to_vec();
        let tile_b = b"tile-b".to_vec();
        let child = TileTreeIndex {
            index_schema_version: TILE_TREE_INDEX_SCHEMA_VERSION,
            layer_id: "elevation".to_owned(),
            entries: vec![
                entry(
                    TileTreeEntryKind::Tile,
                    "0000010000000000",
                    10,
                    "layers/elevation/l10/a.tile",
                    &tile_a,
                ),
                entry(
                    TileTreeEntryKind::Tile,
                    "0000000100000000",
                    14,
                    "layers/elevation/l14/b.tile",
                    &tile_b,
                ),
            ],
        };
        let child_bytes = child.canonical_bytes().expect("canonical child index");
        let root = TileTreeIndex {
            index_schema_version: TILE_TREE_INDEX_SCHEMA_VERSION,
            layer_id: "elevation".to_owned(),
            entries: vec![entry(
                TileTreeEntryKind::Index,
                "1000000000000000",
                0,
                "layers/elevation/face-0.index",
                &child_bytes,
            )],
        };
        let root_bytes = root.canonical_bytes().expect("canonical root index");
        let tree = TileTreeReference {
            index_schema_version: TILE_TREE_INDEX_SCHEMA_VERSION,
            root_index_path: "layers/elevation/root.index".to_owned(),
            root_index_media_type: "application/vnd.atinycivilization.tile-index+json".to_owned(),
            root_index_hash: Digest::sha256(&root_bytes),
            root_index_byte_length: u64::try_from(root_bytes.len()).expect("root length fits"),
            leaf_tile_count: 2,
            minimum_s2_level: 10,
            maximum_s2_level: 23,
        };
        let files = BTreeMap::from([
            (tree.root_index_path.clone(), root_bytes),
            ("layers/elevation/face-0.index".to_owned(), child_bytes),
            ("layers/elevation/l10/a.tile".to_owned(), tile_a),
            ("layers/elevation/l14/b.tile".to_owned(), tile_b),
        ]);
        (tree, files)
    }

    fn verify_from_map(
        tree: &TileTreeReference,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<VerificationStats> {
        verify_tile_tree("elevation", tree, &mut BTreeSet::new(), |path| {
            files
                .get(path)
                .cloned()
                .with_context(|| format!("missing fixture {path}"))
        })
    }

    #[test]
    fn source_snapshot_verification_streams_and_rejects_tampering() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "a-tiny-civilization-source-snapshot-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create source snapshot fixture root");
        let bytes = vec![0x5a; 2 * 64 * 1024 + 17];
        fs::write(root.join("source.bin"), &bytes).expect("write source fixture");
        let artifact = SourceSnapshotArtifact {
            role: SourceSnapshotArtifactRole::Data,
            artifact_path: "source.bin".to_owned(),
            download_url: "https://example.test/source.bin".to_owned(),
            media_type: "application/octet-stream".to_owned(),
            content_hash: Digest::sha256(&bytes),
            byte_length: u64::try_from(bytes.len()).expect("fixture length fits u64"),
        };
        verify_source_snapshot_artifact(&artifact, &root).expect("valid streamed source");

        let mut tampered = bytes.clone();
        tampered[64 * 1024] ^= 1;
        fs::write(root.join("source.bin"), tampered).expect("write same-length tamper");
        assert!(verify_source_snapshot_artifact(&artifact, &root).is_err());

        fs::write(root.join("source.bin"), &bytes[..bytes.len() - 1])
            .expect("write truncated tamper");
        assert!(verify_source_snapshot_artifact(&artifact, &root).is_err());

        let mut oversized = bytes.clone();
        oversized.push(0);
        fs::write(root.join("source.bin"), oversized).expect("write oversized tamper");
        assert!(verify_source_snapshot_artifact(&artifact, &root).is_err());
        fs::remove_dir_all(root).expect("remove source snapshot fixture root");
    }

    #[test]
    fn provisional_composition_loads_canonical_bytes_and_verifies_every_artifact() {
        let root = temporary_root("provisional-load");
        let composition = provisional_composition(&root);
        let composition_path = root.join("composition.json");
        fs::write(
            &composition_path,
            composition
                .canonical_bytes()
                .expect("canonical provisional composition"),
        )
        .expect("write provisional composition");

        let loaded = load_provisional_world_composition(&composition_path)
            .expect("load canonical provisional composition");
        assert_eq!(loaded, composition);
        let stats = verify_provisional_world_artifacts(&loaded, &root)
            .expect("verify provisional artifacts");
        assert_eq!(stats.artifacts, 10);
        assert_eq!(
            stats.bytes,
            loaded
                .earth_layers
                .iter()
                .map(|layer| layer.release.byte_length)
                .chain(
                    loaded
                        .world_components
                        .iter()
                        .map(|component| component.release.byte_length)
                )
                .sum::<u64>()
        );

        let mut noncanonical = composition
            .canonical_bytes()
            .expect("canonical provisional composition");
        noncanonical.push(b'\n');
        fs::write(&composition_path, noncanonical).expect("write noncanonical composition");
        assert!(load_provisional_world_composition(&composition_path).is_err());

        fs::remove_dir_all(root).expect("remove provisional fixture root");
    }

    #[test]
    fn provisional_verification_rejects_hash_and_length_tampering() {
        let root = temporary_root("provisional-tamper");
        let composition = provisional_composition(&root);
        let artifact = &composition.earth_layers[0].release;
        let path = root.join(&artifact.artifact_path);
        let original = fs::read(&path).expect("read provisional fixture");

        let mut same_length_tamper = original.clone();
        same_length_tamper[0] ^= 1;
        fs::write(&path, same_length_tamper).expect("write same-length tamper");
        assert!(verify_provisional_world_artifacts(&composition, &root).is_err());

        let mut length_tamper = original;
        length_tamper.push(0);
        fs::write(&path, length_tamper).expect("write length tamper");
        assert!(verify_provisional_world_artifacts(&composition, &root).is_err());

        fs::remove_dir_all(root).expect("remove provisional tamper root");
    }

    #[test]
    fn provisional_verification_rejects_nonportable_paths_before_filesystem_access() {
        let root = temporary_root("provisional-path");
        let mut composition = provisional_composition(&root);
        composition.earth_layers[0].release.artifact_path = "../outside.bin".to_owned();
        let error = verify_provisional_world_artifacts(&composition, &root)
            .expect_err("parent path must fail closed");
        assert!(format!("{error:#}").contains("provisional composition is invalid"));
        fs::remove_dir_all(root).expect("remove provisional path root");
    }

    #[cfg(unix)]
    #[test]
    fn provisional_verification_rejects_symlink_leaves_and_parents() {
        use std::os::unix::fs::symlink;

        let root = temporary_root("provisional-symlink");
        let outside = temporary_root("provisional-symlink-outside");
        let composition = provisional_composition(&root);
        let leaf = &composition.earth_layers[0].release;
        let leaf_path = root.join(&leaf.artifact_path);
        let outside_leaf = outside.join("outside.bin");
        fs::write(
            &outside_leaf,
            fs::read(&leaf_path).expect("read leaf fixture"),
        )
        .expect("write outside leaf fixture");
        fs::remove_file(&leaf_path).expect("remove original leaf fixture");
        symlink(&outside_leaf, &leaf_path).expect("create leaf symlink");
        assert!(verify_provisional_world_artifacts(&composition, &root).is_err());

        fs::remove_file(&leaf_path).expect("remove leaf symlink");
        let parent = root.join("provisional");
        let moved_parent = outside.join("provisional");
        fs::rename(&parent, &moved_parent).expect("move fixture directory outside root");
        symlink(&moved_parent, &parent).expect("create parent symlink");
        assert!(verify_provisional_world_artifacts(&composition, &root).is_err());

        fs::remove_dir_all(root).expect("remove provisional symlink root");
        fs::remove_dir_all(outside).expect("remove outside provisional root");
    }

    #[test]
    fn traverses_every_index_and_leaf() {
        let (tree, files) = valid_tree();
        let stats = verify_from_map(&tree, &files).expect("valid tree verifies");
        assert_eq!(stats.artifacts, 4);
        assert_eq!(stats.tile_indexes, 2);
        assert_eq!(stats.tiles, 2);
        assert_eq!(
            stats.bytes,
            files
                .values()
                .map(|bytes| u64::try_from(bytes.len()).expect("fixture length fits"))
                .sum::<u64>()
        );
    }

    #[test]
    fn s2_containment_uses_the_declared_non_root_parent_scope() {
        let parent = entry(
            TileTreeEntryKind::Index,
            "0000010000000000",
            10,
            "layers/elevation/l10/parent.index",
            b"parent",
        );
        let descendant = entry(
            TileTreeEntryKind::Tile,
            "0000000100000000",
            14,
            "layers/elevation/l14/inside.tile",
            b"inside",
        );
        let sibling = entry(
            TileTreeEntryKind::Tile,
            "0000020100000000",
            14,
            "layers/elevation/l14/outside.tile",
            b"outside",
        );

        assert!(entry_contains(&parent, &descendant).expect("valid S2 fixture"));
        assert!(!entry_contains(&parent, &sibling).expect("valid S2 fixture"));
    }

    #[test]
    fn packed_scalar_terrain_tile_binds_its_index_scope_and_layer() {
        use world_data::{PackedScalarTerrainTile, ScalarTerrainCell};

        let container: S2CellId = "1000010000000000".parse().expect("valid container");
        let tile = PackedScalarTerrainTile {
            tile_schema_version: 1,
            layer_id: "elevation".to_owned(),
            source_snapshot_digest: Digest::sha256(b"snapshot"),
            source_artifact_digest: Digest::sha256(b"artifact"),
            quadrature_points_per_axis: 4,
            container_s2_cell_id: container,
            target_s2_level: 11,
            cells: container
                .children()
                .expect("children")
                .into_iter()
                .map(|s2_cell_id| ScalarTerrainCell {
                    s2_cell_id,
                    support_samples: 1,
                    minimum_millimetres: 0,
                    mean_millimetres: 0,
                    maximum_millimetres: 0,
                })
                .collect(),
        };
        let bytes = tile.canonical_bytes().expect("canonical terrain tile");
        let entry = TileTreeEntry {
            kind: TileTreeEntryKind::Tile,
            s2_cell_id: container.to_string(),
            s2_level: 10,
            artifact: artifact(
                "layers/elevation/l10/terrain.tile",
                PACKED_SCALAR_TERRAIN_TILE_MEDIA_TYPE,
                &bytes,
            ),
        };
        validate_known_tile_payload("elevation", &entry, &bytes).expect("matching payload");
        assert!(validate_known_tile_payload("bathymetry", &entry, &bytes).is_err());
    }

    #[test]
    fn rejects_tampering_false_counts_cycles_and_wrong_parentage() {
        let (tree, mut files) = valid_tree();
        files.insert(
            "layers/elevation/l10/a.tile".to_owned(),
            b"tampered".to_vec(),
        );
        assert!(verify_from_map(&tree, &files).is_err());

        let (mut wrong_count, files) = valid_tree();
        wrong_count.leaf_tile_count = 3;
        assert!(verify_from_map(&wrong_count, &files).is_err());

        let (tree, mut files) = valid_tree();
        let root = TileTreeIndex {
            index_schema_version: TILE_TREE_INDEX_SCHEMA_VERSION,
            layer_id: "elevation".to_owned(),
            entries: vec![entry(
                TileTreeEntryKind::Index,
                "1000000000000000",
                0,
                &tree.root_index_path,
                files
                    .get(&tree.root_index_path)
                    .expect("root fixture exists"),
            )],
        };
        let root_bytes = root.canonical_bytes().expect("canonical cyclic root");
        let mut cyclic = tree.clone();
        cyclic.root_index_hash = Digest::sha256(&root_bytes);
        cyclic.root_index_byte_length = u64::try_from(root_bytes.len()).expect("root length fits");
        files.insert(cyclic.root_index_path.clone(), root_bytes);
        assert!(verify_from_map(&cyclic, &files).is_err());

        let (mut tree, mut files) = valid_tree();
        let child = TileTreeIndex {
            index_schema_version: TILE_TREE_INDEX_SCHEMA_VERSION,
            layer_id: "elevation".to_owned(),
            entries: vec![entry(
                TileTreeEntryKind::Tile,
                "2000010000000000",
                10,
                "layers/elevation/l10/wrong-face.tile",
                b"wrong-face",
            )],
        };
        let child_bytes = child.canonical_bytes().expect("canonical wrong child");
        let root = TileTreeIndex {
            index_schema_version: TILE_TREE_INDEX_SCHEMA_VERSION,
            layer_id: "elevation".to_owned(),
            entries: vec![entry(
                TileTreeEntryKind::Index,
                "1000000000000000",
                0,
                "layers/elevation/face-0.index",
                &child_bytes,
            )],
        };
        let root_bytes = root.canonical_bytes().expect("canonical updated root");
        tree.root_index_hash = Digest::sha256(&root_bytes);
        tree.root_index_byte_length = u64::try_from(root_bytes.len()).expect("root length fits");
        tree.leaf_tile_count = 1;
        files.insert(tree.root_index_path.clone(), root_bytes);
        files.insert("layers/elevation/face-0.index".to_owned(), child_bytes);
        files.insert(
            "layers/elevation/l10/wrong-face.tile".to_owned(),
            b"wrong-face".to_vec(),
        );
        let error = verify_from_map(&tree, &files).expect_err("wrong S2 face must fail");
        assert!(format!("{error:#}").contains("outside parent"));
    }

    #[cfg(unix)]
    #[test]
    fn safe_artifact_root_rejects_symlink_leaves_and_parent_escape() {
        use std::{
            os::unix::fs::symlink,
            time::{SystemTime, UNIX_EPOCH},
        };

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let temporary = std::env::temp_dir();
        let root = temporary.join(format!(
            "a-tiny-civilization-artifacts-{}-{nonce}",
            std::process::id()
        ));
        let outside = temporary.join(format!(
            "a-tiny-civilization-outside-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create safe root fixture");
        fs::create_dir_all(&outside).expect("create outside fixture");
        fs::write(root.join("plain.tile"), b"plain").expect("write in-root fixture");
        fs::write(outside.join("outside.tile"), b"outside").expect("write outside fixture");
        symlink(outside.join("outside.tile"), root.join("leaf-link.tile"))
            .expect("create leaf symlink");
        symlink(&outside, root.join("parent-link")).expect("create parent symlink");

        let safe = SafeArtifactRoot::new(&root).expect("valid safe root");
        assert_eq!(safe.read("plain.tile").expect("plain file reads"), b"plain");
        assert!(safe.read("leaf-link.tile").is_err());
        assert!(safe.read("parent-link/outside.tile").is_err());

        fs::remove_dir_all(&root).expect("remove safe root fixture");
        fs::remove_dir_all(&outside).expect("remove outside fixture");
    }
}
