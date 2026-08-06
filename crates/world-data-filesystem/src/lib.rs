//! Safe, exhaustive filesystem verification for scientific world-data releases.

use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use world_data::{
    BundleArtifact, BundleArtifactKind, DataLayerStorage, TileTreeEntry, TileTreeEntryKind,
    TileTreeIndex, TileTreeReference, WorldDataBundle,
};

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

    fn read(&self, relative_path: &str) -> Result<Vec<u8>> {
        let unresolved = self.canonical_root.join(relative_path);
        let metadata = fs::symlink_metadata(&unresolved).with_context(|| {
            format!("failed to inspect bundle artifact {}", unresolved.display())
        })?;
        if metadata.file_type().is_symlink() {
            bail!(
                "bundle artifact {} is a symbolic link",
                unresolved.display()
            );
        }
        if !metadata.is_file() {
            bail!("bundle artifact {} is not a file", unresolved.display());
        }
        let resolved = unresolved.canonicalize().with_context(|| {
            format!("failed to resolve bundle artifact {}", unresolved.display())
        })?;
        if !resolved.starts_with(&self.canonical_root) {
            bail!(
                "bundle artifact {} resolves outside {}",
                unresolved.display(),
                self.canonical_root.display()
            );
        }
        fs::read(&resolved)
            .with_context(|| format!("failed to read bundle artifact {}", resolved.display()))
    }
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

fn entry_contains(parent: &TileTreeEntry, descendant: &TileTreeEntry) -> Result<bool> {
    let parent_id = u64::from_str_radix(&parent.s2_cell_id, 16)
        .with_context(|| format!("invalid parent S2 CellId {}", parent.s2_cell_id))?;
    let descendant_id = u64::from_str_radix(&descendant.s2_cell_id, 16)
        .with_context(|| format!("invalid descendant S2 CellId {}", descendant.s2_cell_id))?;
    if descendant.s2_level < parent.s2_level
        || (descendant.kind == TileTreeEntryKind::Index && descendant.s2_level == parent.s2_level)
    {
        return Ok(false);
    }
    let shift = 2_u32
        .checked_mul(30_u32.saturating_sub(u32::from(parent.s2_level)))
        .context("S2 parent level overflow")?;
    let sentinel = 1_u64
        .checked_shl(shift)
        .context("S2 parent sentinel overflow")?;
    let normalized_parent = (descendant_id & !(sentinel - 1)) | sentinel;
    Ok(normalized_parent == parent_id)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use world_data::{TILE_TREE_INDEX_SCHEMA_VERSION, TileArtifactReference, TileTreeEntryKind};
    use world_domain::Digest;

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
