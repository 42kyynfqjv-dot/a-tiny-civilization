//! Deterministic, provenance-complete scientific input bundles.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use world_domain::{
    Digest, FullEarthGrid, S2CellId, SpatialGrid, WorldConfiguration, WorldGeometry,
};

pub const LEGACY_WORLD_DATA_BUNDLE_SCHEMA_VERSION: u16 = 1;
pub const WORLD_DATA_BUNDLE_SCHEMA_VERSION: u16 = 2;
pub const TILE_TREE_INDEX_SCHEMA_VERSION: u16 = 1;
pub const SOURCE_SNAPSHOT_SCHEMA_VERSION: u16 = 1;
const MAX_DECIMAL_PLACES: u8 = 9;
const FORBIDDEN_AFFORDANCE_CODES: &[&str] = &[
    "building",
    "edible",
    "food",
    "invention",
    "medicine",
    "prey",
    "shelter",
    "technology",
    "tool",
    "weapon",
];

/// One complete, normalized and genesis-eligible scientific input release.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldDataBundle {
    pub bundle_schema_version: u16,
    pub bundle_id: String,
    pub bundle_version: String,
    pub title: String,
    pub license_expression: String,
    pub reference_domain: ReferenceDomain,
    #[serde(flatten)]
    pub coverage: WorldDataCoverage,
    pub normalization: NormalizationRecord,
    pub sources: Vec<SourceRecord>,
    pub assumptions: Vec<AssumptionRecord>,
    pub entities: Vec<CatalogEntity>,
    pub parameters: Vec<ParameterRecord>,
    pub layers: Vec<DataLayer>,
}

/// Spatial coverage encoded without changing legacy schema-v1 JSON field names.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WorldDataCoverage {
    BoundedRaster {
        spatial_grid: SpatialGrid,
    },
    FullEarth {
        full_earth_grid: FullEarthGrid,
        earth_baseline: EarthBaseline,
    },
}

/// The public world uses present physical geography plus a documented counterfactual
/// biosphere. Source epochs remain explicit; this is not a synchronized historical Earth.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EarthBaseline {
    pub manifest_cutoff_date: NaiveDate,
    pub source_epoch_policy: SourceEpochPolicy,
    pub human_feature_policy: HumanFeaturePolicy,
    pub sensitive_location_policy: SensitiveLocationPolicy,
    pub sea_level_definition: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceEpochPolicy {
    PerSourceEpochComposite,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanFeaturePolicy {
    ExcludeDirectFeaturesAndFlagInferences,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveLocationPolicy {
    OmitOrGeneralize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReferenceDomain {
    pub name: String,
    pub description: String,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NormalizationRecord {
    pub pipeline_id: String,
    pub pipeline_version: String,
    pub source_revision: String,
    pub executable_hash: Digest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceRecord {
    pub source_id: String,
    pub title: String,
    pub publisher: String,
    pub canonical_url: String,
    pub version: String,
    pub retrieved_on: NaiveDate,
    pub license_expression: String,
    pub artifact_path: String,
    pub artifact_media_type: String,
    pub artifact_hash: Digest,
    #[serde(with = "u64_decimal")]
    pub artifact_byte_length: u64,
}

/// Exact upstream evidence acquired before normalization into a world-data bundle.
///
/// A source snapshot is never itself a canonical world input. It records immutable
/// acquisition facts so a later normalization pipeline can cite verified source bytes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSnapshotManifest {
    pub source_snapshot_schema_version: u16,
    pub snapshot_id: String,
    pub title: String,
    pub publisher: String,
    pub documentation_url: String,
    pub upstream_release: String,
    pub upstream_revision: String,
    pub artifact_locator_policy: SourceSnapshotLocatorPolicy,
    pub dataset_version: String,
    pub retrieved_on: NaiveDate,
    pub license_expression: String,
    pub license_url: String,
    pub scope: String,
    pub limitations: Vec<String>,
    pub artifacts: Vec<SourceSnapshotArtifact>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSnapshotArtifactRole {
    Data,
    Documentation,
    LicenseEvidence,
    VersionEvidence,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSnapshotLocatorPolicy {
    RevisionInEveryArtifactUrl,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSnapshotArtifact {
    pub role: SourceSnapshotArtifactRole,
    pub artifact_path: String,
    pub download_url: String,
    pub media_type: String,
    pub content_hash: Digest,
    #[serde(with = "u64_decimal")]
    pub byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AssumptionRecord {
    pub assumption_id: String,
    pub statement: String,
    pub rationale: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogEntityKind {
    Taxon,
    Material,
    ChemicalSubstance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CatalogEntity {
    pub entity_id: String,
    pub kind: CatalogEntityKind,
    pub canonical_name: String,
    pub scientific_name: Option<String>,
    pub external_identities: Vec<ExternalIdentity>,
    pub source_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExternalIdentity {
    pub authority: String,
    pub identifier: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    DirectMeasurement,
    DocumentedTransformation,
    LiteratureApproximation,
    EngineeringAssumption,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceRecord {
    pub classification: EvidenceClass,
    pub source_ids: Vec<String>,
    pub assumption_ids: Vec<String>,
    pub method: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParameterRecord {
    pub parameter_id: String,
    pub subject_id: String,
    pub property: String,
    pub value: ParameterValue,
    pub evidence: EvidenceRecord,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ParameterValue {
    Quantity(ScaledRange),
    Category { code: String },
    Boolean { value: bool },
}

/// Exact decimal range: stored integers are divided by 10^decimal_places.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScaledRange {
    #[serde(with = "i64_decimal")]
    pub minimum: i64,
    #[serde(with = "i64_decimal")]
    pub typical: i64,
    #[serde(with = "i64_decimal")]
    pub maximum: i64,
    pub decimal_places: u8,
    pub unit: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataLayerKind {
    Bathymetry,
    Climate,
    Coastline,
    Elevation,
    Habitat,
    Hydrography,
    Soil,
}

impl DataLayerKind {
    const REQUIRED_BOUNDED: [Self; 5] = [
        Self::Climate,
        Self::Elevation,
        Self::Habitat,
        Self::Hydrography,
        Self::Soil,
    ];

    const REQUIRED_FULL_EARTH: [Self; 7] = [
        Self::Bathymetry,
        Self::Climate,
        Self::Coastline,
        Self::Elevation,
        Self::Habitat,
        Self::Hydrography,
        Self::Soil,
    ];
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldUnit {
    pub field: String,
    pub unit: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DataLayer {
    pub layer_id: String,
    pub kind: DataLayerKind,
    #[serde(flatten)]
    pub storage: DataLayerStorage,
    pub units: Vec<FieldUnit>,
    pub source_ids: Vec<String>,
    pub transformation: String,
}

/// A bounded release stores one raster. A full-Earth release stores a content-addressed
/// tile-tree root, whose child indexes and leaves are verified when traversed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DataLayerStorage {
    Raster {
        artifact_path: String,
        media_type: String,
        width_cells: u32,
        height_cells: u32,
        content_hash: Digest,
        #[serde(with = "u64_decimal")]
        byte_length: u64,
    },
    FullEarthTileTree {
        tile_tree: TileTreeReference,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TileTreeReference {
    pub index_schema_version: u16,
    pub root_index_path: String,
    pub root_index_media_type: String,
    pub root_index_hash: Digest,
    #[serde(with = "u64_decimal")]
    pub root_index_byte_length: u64,
    #[serde(with = "u64_decimal")]
    pub leaf_tile_count: u64,
    pub minimum_s2_level: u8,
    pub maximum_s2_level: u8,
}

/// One canonical content-addressed index node in a normalized layer tile tree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TileTreeIndex {
    pub index_schema_version: u16,
    pub layer_id: String,
    pub entries: Vec<TileTreeEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TileTreeEntry {
    pub kind: TileTreeEntryKind,
    /// Fixed-width lowercase hexadecimal S2 CellId.
    pub s2_cell_id: String,
    pub s2_level: u8,
    pub artifact: TileArtifactReference,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TileTreeEntryKind {
    Index,
    Tile,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TileArtifactReference {
    pub path: String,
    pub media_type: String,
    pub content_hash: Digest,
    #[serde(with = "u64_decimal")]
    pub byte_length: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundleArtifactKind {
    Source,
    NormalizedLayer,
    TileTreeIndex,
    Tile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleArtifact<'a> {
    pub kind: BundleArtifactKind,
    pub relative_path: &'a str,
    pub content_hash: Digest,
    pub byte_length: u64,
}

impl SourceSnapshotManifest {
    pub fn validate(&self) -> Result<(), SourceSnapshotError> {
        if self.source_snapshot_schema_version != SOURCE_SNAPSHOT_SCHEMA_VERSION {
            return Err(SourceSnapshotError::UnsupportedSchema(
                self.source_snapshot_schema_version,
            ));
        }
        validate_slug(&self.snapshot_id, "source_snapshot.snapshot_id")?;
        require_text(&self.title, "source_snapshot.title")?;
        require_text(&self.publisher, "source_snapshot.publisher")?;
        validate_https_url(&self.documentation_url, "source_snapshot.documentation_url")?;
        require_text(&self.upstream_release, "source_snapshot.upstream_release")?;
        if self.upstream_revision.len() < 12
            || !self
                .upstream_revision
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(SourceSnapshotError::InvalidUpstreamRevision(
                self.upstream_revision.clone(),
            ));
        }
        require_text(&self.dataset_version, "source_snapshot.dataset_version")?;
        require_text(
            &self.license_expression,
            "source_snapshot.license_expression",
        )?;
        validate_https_url(&self.license_url, "source_snapshot.license_url")?;
        require_text(&self.scope, "source_snapshot.scope")?;
        require_nonempty(&self.limitations, "source_snapshot.limitations")?;
        require_nonempty(&self.artifacts, "source_snapshot.artifacts")?;

        let mut limitations = BTreeSet::new();
        for limitation in &self.limitations {
            require_text(limitation, "source_snapshot.limitations")?;
            if !limitations.insert(limitation.as_str()) {
                return Err(SourceSnapshotError::DuplicateLimitation(limitation.clone()));
            }
        }

        validate_sorted_unique(
            self.artifacts
                .iter()
                .map(|artifact| artifact.artifact_path.as_str()),
            "source_snapshot.artifacts",
        )?;
        let mut roles = BTreeSet::new();
        for artifact in &self.artifacts {
            artifact.validate()?;
            match self.artifact_locator_policy {
                SourceSnapshotLocatorPolicy::RevisionInEveryArtifactUrl
                    if !artifact.download_url.contains(&self.upstream_revision) =>
                {
                    return Err(SourceSnapshotError::ArtifactUrlMissingRevision {
                        path: artifact.artifact_path.clone(),
                        revision: self.upstream_revision.clone(),
                    });
                }
                SourceSnapshotLocatorPolicy::RevisionInEveryArtifactUrl => {}
            }
            roles.insert(artifact.role);
        }
        for required in [
            SourceSnapshotArtifactRole::Data,
            SourceSnapshotArtifactRole::Documentation,
            SourceSnapshotArtifactRole::LicenseEvidence,
            SourceSnapshotArtifactRole::VersionEvidence,
        ] {
            if !roles.contains(&required) {
                return Err(SourceSnapshotError::MissingArtifactRole(required));
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SourceSnapshotError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self)
            .map_err(|error| SourceSnapshotError::Encoding(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn content_digest(&self) -> Result<Digest, SourceSnapshotError> {
        Ok(Digest::sha256(&self.canonical_bytes()?))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, SourceSnapshotError> {
        let snapshot: Self = serde_json::from_slice(bytes)
            .map_err(|error| SourceSnapshotError::Decode(error.to_string()))?;
        snapshot.validate()?;
        if snapshot.canonical_bytes()? != bytes {
            return Err(SourceSnapshotError::NonCanonicalEncoding);
        }
        Ok(snapshot)
    }
}

impl SourceSnapshotArtifact {
    pub fn validate(&self) -> Result<(), SourceSnapshotError> {
        validate_artifact_path(&self.artifact_path, "source_snapshot.artifact_path")?;
        validate_https_url(&self.download_url, "source_snapshot.artifact.download_url")?;
        validate_media_type(&self.media_type)?;
        if self.content_hash == Digest::ZERO {
            return Err(SourceSnapshotError::ZeroDigest);
        }
        if self.byte_length == 0 {
            return Err(SourceSnapshotError::ZeroByteLength);
        }
        Ok(())
    }

    #[must_use]
    pub fn expected_artifact(&self) -> BundleArtifact<'_> {
        BundleArtifact {
            kind: BundleArtifactKind::Source,
            relative_path: &self.artifact_path,
            content_hash: self.content_hash,
            byte_length: self.byte_length,
        }
    }
}

impl WorldDataBundle {
    pub fn validate(&self) -> Result<(), BundleError> {
        if !matches!(
            self.bundle_schema_version,
            LEGACY_WORLD_DATA_BUNDLE_SCHEMA_VERSION | WORLD_DATA_BUNDLE_SCHEMA_VERSION
        ) {
            return Err(BundleError::UnsupportedSchema(self.bundle_schema_version));
        }
        match (&self.coverage, self.bundle_schema_version) {
            (WorldDataCoverage::BoundedRaster { .. }, LEGACY_WORLD_DATA_BUNDLE_SCHEMA_VERSION)
            | (WorldDataCoverage::FullEarth { .. }, WORLD_DATA_BUNDLE_SCHEMA_VERSION) => {}
            _ => {
                return Err(BundleError::CoverageSchemaMismatch {
                    schema: self.bundle_schema_version,
                });
            }
        }
        validate_slug(&self.bundle_id, "bundle_id")?;
        validate_semver(&self.bundle_version)?;
        require_text(&self.title, "title")?;
        require_text(&self.license_expression, "license_expression")?;
        self.coverage.validate()?;
        self.normalization.validate()?;

        require_nonempty(&self.sources, "sources")?;
        require_nonempty(&self.entities, "entities")?;
        require_nonempty(&self.parameters, "parameters")?;
        require_nonempty(&self.layers, "layers")?;

        validate_sorted_unique(
            self.sources.iter().map(|source| source.source_id.as_str()),
            "sources",
        )?;
        validate_sorted_unique(
            self.assumptions
                .iter()
                .map(|assumption| assumption.assumption_id.as_str()),
            "assumptions",
        )?;
        validate_sorted_unique(
            self.entities.iter().map(|entity| entity.entity_id.as_str()),
            "entities",
        )?;
        validate_sorted_unique(
            self.parameters
                .iter()
                .map(|parameter| parameter.parameter_id.as_str()),
            "parameters",
        )?;
        validate_sorted_unique(
            self.layers.iter().map(|layer| layer.layer_id.as_str()),
            "layers",
        )?;

        let source_ids = self
            .sources
            .iter()
            .map(|source| source.source_id.as_str())
            .collect::<BTreeSet<_>>();
        let assumption_ids = self
            .assumptions
            .iter()
            .map(|assumption| assumption.assumption_id.as_str())
            .collect::<BTreeSet<_>>();
        let entity_ids = self
            .entities
            .iter()
            .map(|entity| entity.entity_id.as_str())
            .collect::<BTreeSet<_>>();

        require_text(&self.reference_domain.name, "reference_domain.name")?;
        require_text(
            &self.reference_domain.description,
            "reference_domain.description",
        )?;
        require_nonempty(
            &self.reference_domain.source_ids,
            "reference_domain.source_ids",
        )?;
        validate_references(
            &self.reference_domain.source_ids,
            &source_ids,
            "reference_domain.source_ids",
        )?;

        for source in &self.sources {
            source.validate()?;
        }
        if let WorldDataCoverage::FullEarth { earth_baseline, .. } = &self.coverage
            && self
                .sources
                .iter()
                .any(|source| source.retrieved_on > earth_baseline.manifest_cutoff_date)
        {
            return Err(BundleError::SourceAfterManifestCutoff);
        }
        for assumption in &self.assumptions {
            assumption.validate()?;
        }
        for entity in &self.entities {
            entity.validate(&source_ids)?;
        }

        let mut subject_properties = BTreeSet::new();
        let mut referenced_assumptions = BTreeSet::new();
        for parameter in &self.parameters {
            parameter.validate(&source_ids, &assumption_ids, &entity_ids)?;
            if !subject_properties
                .insert((parameter.subject_id.as_str(), parameter.property.as_str()))
            {
                return Err(BundleError::DuplicateSubjectProperty {
                    subject_id: parameter.subject_id.clone(),
                    property: parameter.property.clone(),
                });
            }
            referenced_assumptions
                .extend(parameter.evidence.assumption_ids.iter().map(String::as_str));
        }

        for entity_id in &entity_ids {
            if !self
                .parameters
                .iter()
                .any(|parameter| parameter.subject_id == *entity_id)
            {
                return Err(BundleError::EntityWithoutParameters(
                    (*entity_id).to_owned(),
                ));
            }
        }
        for assumption_id in &assumption_ids {
            if !referenced_assumptions.contains(assumption_id) {
                return Err(BundleError::UnusedAssumption((*assumption_id).to_owned()));
            }
        }

        let mut layer_kinds = BTreeSet::new();
        for layer in &self.layers {
            layer.validate(&source_ids, &self.coverage)?;
            layer_kinds.insert(layer.kind);
        }
        let required_layers: &[DataLayerKind] = match &self.coverage {
            WorldDataCoverage::BoundedRaster { .. } => &DataLayerKind::REQUIRED_BOUNDED,
            WorldDataCoverage::FullEarth { .. } => &DataLayerKind::REQUIRED_FULL_EARTH,
        };
        for required in required_layers {
            if !layer_kinds.contains(required) {
                return Err(BundleError::MissingRequiredLayer(*required));
            }
        }

        let mut artifact_paths = BTreeSet::new();
        for artifact in self.unchecked_artifacts() {
            if !artifact_paths.insert(artifact.relative_path) {
                return Err(BundleError::DuplicateIdentifier {
                    field: "artifact_paths",
                    value: artifact.relative_path.to_owned(),
                });
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, BundleError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| BundleError::Encoding(error.to_string()))
    }

    pub fn content_digest(&self) -> Result<Digest, BundleError> {
        Ok(Digest::sha256(&self.canonical_bytes()?))
    }

    pub fn artifacts(&self) -> Result<Vec<BundleArtifact<'_>>, BundleError> {
        self.validate()?;
        let mut artifacts = self.unchecked_artifacts();
        artifacts.sort_by_key(|artifact| artifact.relative_path);
        Ok(artifacts)
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, BundleError> {
        let bundle: Self = serde_json::from_slice(bytes)
            .map_err(|error| BundleError::Decode(error.to_string()))?;
        bundle.validate()?;
        if bundle.canonical_bytes()? != bytes {
            return Err(BundleError::NonCanonicalEncoding);
        }
        Ok(bundle)
    }

    pub fn validate_for_configuration(
        &self,
        configuration: &WorldConfiguration,
    ) -> Result<(), BundleError> {
        self.validate()?;
        configuration
            .validate()
            .map_err(|error| BundleError::InvalidConfiguration(error.to_string()))?;
        let reference = &configuration.world_data;
        if reference.bundle_schema_version != self.bundle_schema_version {
            return Err(BundleError::ConfigurationMismatch("bundle_schema_version"));
        }
        if reference.bundle_id != self.bundle_id {
            return Err(BundleError::ConfigurationMismatch("bundle_id"));
        }
        if reference.bundle_version != self.bundle_version {
            return Err(BundleError::ConfigurationMismatch("bundle_version"));
        }
        if reference.license_expression != self.license_expression {
            return Err(BundleError::ConfigurationMismatch("license_expression"));
        }
        let coverage_matches = match (&self.coverage, &configuration.geometry) {
            (
                WorldDataCoverage::BoundedRaster {
                    spatial_grid: bundle,
                },
                WorldGeometry::BoundedRaster {
                    spatial_grid: configured,
                },
            ) => bundle == configured,
            (
                WorldDataCoverage::FullEarth {
                    full_earth_grid: bundle,
                    ..
                },
                WorldGeometry::FullEarth {
                    full_earth_grid: configured,
                },
            ) => bundle == configured,
            _ => false,
        };
        if !coverage_matches {
            return Err(BundleError::ConfigurationMismatch("coverage"));
        }
        if reference.content_hash != self.content_digest()? {
            return Err(BundleError::ConfigurationMismatch("content_hash"));
        }
        Ok(())
    }

    fn unchecked_artifacts(&self) -> Vec<BundleArtifact<'_>> {
        self.sources
            .iter()
            .map(|source| BundleArtifact {
                kind: BundleArtifactKind::Source,
                relative_path: &source.artifact_path,
                content_hash: source.artifact_hash,
                byte_length: source.artifact_byte_length,
            })
            .chain(self.layers.iter().map(DataLayer::artifact))
            .collect()
    }
}

impl WorldDataCoverage {
    fn validate(&self) -> Result<(), BundleError> {
        match self {
            Self::BoundedRaster { spatial_grid } => spatial_grid
                .validate()
                .map_err(|error| BundleError::InvalidSpatialGrid(error.to_string())),
            Self::FullEarth {
                full_earth_grid,
                earth_baseline,
            } => {
                full_earth_grid
                    .validate()
                    .map_err(|error| BundleError::InvalidFullEarthGrid(error.to_string()))?;
                earth_baseline.validate()
            }
        }
    }
}

impl EarthBaseline {
    fn validate(&self) -> Result<(), BundleError> {
        require_text(
            &self.sea_level_definition,
            "earth_baseline.sea_level_definition",
        )
    }
}

impl BundleArtifact<'_> {
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<(), BundleError> {
        let actual_length =
            u64::try_from(bytes.len()).map_err(|_| BundleError::HostLengthOverflow)?;
        self.verify_observation(actual_length, Digest::sha256(bytes))
    }

    pub fn verify_observation(
        &self,
        actual_length: u64,
        actual_digest: Digest,
    ) -> Result<(), BundleError> {
        if actual_length != self.byte_length {
            return Err(BundleError::ArtifactLengthMismatch {
                path: self.relative_path.to_owned(),
                expected: self.byte_length,
                actual: actual_length,
            });
        }
        if actual_digest != self.content_hash {
            return Err(BundleError::ArtifactDigestMismatch {
                path: self.relative_path.to_owned(),
                expected: self.content_hash,
                actual: actual_digest,
            });
        }
        Ok(())
    }
}

impl NormalizationRecord {
    fn validate(&self) -> Result<(), BundleError> {
        validate_slug(&self.pipeline_id, "normalization.pipeline_id")?;
        validate_semver(&self.pipeline_version)?;
        if self.source_revision.len() < 12
            || !self
                .source_revision
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(BundleError::InvalidSourceRevision(
                self.source_revision.clone(),
            ));
        }
        if self.executable_hash == Digest::ZERO {
            return Err(BundleError::ZeroDigest("normalization.executable_hash"));
        }
        Ok(())
    }
}

impl SourceRecord {
    fn validate(&self) -> Result<(), BundleError> {
        validate_slug(&self.source_id, "source_id")?;
        require_text(&self.title, "source.title")?;
        require_text(&self.publisher, "source.publisher")?;
        validate_https_url(&self.canonical_url, "source.canonical_url")?;
        require_text(&self.version, "source.version")?;
        require_text(&self.license_expression, "source.license_expression")?;
        validate_artifact_path(&self.artifact_path, "source.artifact_path")?;
        validate_media_type(&self.artifact_media_type)?;
        if self.artifact_hash == Digest::ZERO {
            return Err(BundleError::ZeroDigest("source.artifact_hash"));
        }
        if self.artifact_byte_length == 0 {
            return Err(BundleError::ZeroByteLength("source.artifact_byte_length"));
        }
        Ok(())
    }
}

impl AssumptionRecord {
    fn validate(&self) -> Result<(), BundleError> {
        validate_slug(&self.assumption_id, "assumption_id")?;
        require_text(&self.statement, "assumption.statement")?;
        require_text(&self.rationale, "assumption.rationale")
    }
}

impl CatalogEntity {
    fn validate(&self, source_ids: &BTreeSet<&str>) -> Result<(), BundleError> {
        validate_slug(&self.entity_id, "entity_id")?;
        require_text(&self.canonical_name, "entity.canonical_name")?;
        if self.kind == CatalogEntityKind::Taxon {
            require_text(
                self.scientific_name.as_deref().unwrap_or_default(),
                "entity.scientific_name",
            )?;
        }
        require_nonempty(&self.external_identities, "entity.external_identities")?;
        require_nonempty(&self.source_ids, "entity.source_ids")?;
        validate_references(&self.source_ids, source_ids, "entity.source_ids")?;

        let mut previous: Option<(&str, &str)> = None;
        for identity in &self.external_identities {
            identity.validate()?;
            let current = (identity.authority.as_str(), identity.identifier.as_str());
            if let Some(last) = previous {
                if current == last {
                    return Err(BundleError::DuplicateIdentifier {
                        field: "entity.external_identities",
                        value: format!("{}:{}", current.0, current.1),
                    });
                }
                if current < last {
                    return Err(BundleError::UnsortedIdentifiers(
                        "entity.external_identities",
                    ));
                }
            }
            previous = Some(current);
        }
        Ok(())
    }
}

impl ExternalIdentity {
    fn validate(&self) -> Result<(), BundleError> {
        validate_slug(&self.authority, "external_identity.authority")?;
        require_text(&self.identifier, "external_identity.identifier")?;
        validate_https_url(&self.url, "external_identity.url")
    }
}

impl ParameterRecord {
    fn validate(
        &self,
        source_ids: &BTreeSet<&str>,
        assumption_ids: &BTreeSet<&str>,
        entity_ids: &BTreeSet<&str>,
    ) -> Result<(), BundleError> {
        validate_slug(&self.parameter_id, "parameter_id")?;
        if !entity_ids.contains(self.subject_id.as_str()) {
            return Err(BundleError::MissingReference {
                field: "parameter.subject_id",
                value: self.subject_id.clone(),
            });
        }
        validate_slug(&self.property, "parameter.property")?;
        reject_privileged_affordance(&self.property)?;
        self.value.validate()?;
        self.evidence.validate(source_ids, assumption_ids)
    }
}

impl ParameterValue {
    fn validate(&self) -> Result<(), BundleError> {
        match self {
            Self::Quantity(range) => range.validate(),
            Self::Category { code } => {
                validate_slug(code, "parameter.category")?;
                reject_privileged_affordance(code)
            }
            Self::Boolean { .. } => Ok(()),
        }
    }
}

impl ScaledRange {
    fn validate(&self) -> Result<(), BundleError> {
        if self.minimum > self.typical || self.typical > self.maximum {
            return Err(BundleError::InvalidQuantityRange);
        }
        if self.decimal_places > MAX_DECIMAL_PLACES {
            return Err(BundleError::ExcessiveDecimalScale(self.decimal_places));
        }
        validate_unit(&self.unit, "parameter.unit")
    }
}

impl EvidenceRecord {
    fn validate(
        &self,
        source_ids: &BTreeSet<&str>,
        assumption_ids: &BTreeSet<&str>,
    ) -> Result<(), BundleError> {
        validate_references(&self.source_ids, source_ids, "evidence.source_ids")?;
        validate_references(
            &self.assumption_ids,
            assumption_ids,
            "evidence.assumption_ids",
        )?;
        require_text(&self.method, "evidence.method")?;
        match self.classification {
            EvidenceClass::DirectMeasurement | EvidenceClass::DocumentedTransformation => {
                if self.source_ids.is_empty() || !self.assumption_ids.is_empty() {
                    return Err(BundleError::InvalidEvidenceLinks(self.classification));
                }
            }
            EvidenceClass::LiteratureApproximation => {
                if self.source_ids.is_empty() {
                    return Err(BundleError::InvalidEvidenceLinks(self.classification));
                }
            }
            EvidenceClass::EngineeringAssumption => {
                if self.assumption_ids.is_empty() {
                    return Err(BundleError::InvalidEvidenceLinks(self.classification));
                }
            }
        }
        Ok(())
    }
}

impl DataLayer {
    fn validate(
        &self,
        source_ids: &BTreeSet<&str>,
        coverage: &WorldDataCoverage,
    ) -> Result<(), BundleError> {
        validate_slug(&self.layer_id, "layer_id")?;
        self.storage.validate(&self.layer_id, coverage)?;
        require_nonempty(&self.units, "layer.units")?;
        validate_sorted_unique(
            self.units.iter().map(|unit| unit.field.as_str()),
            "layer.units",
        )?;
        for unit in &self.units {
            validate_slug(&unit.field, "layer.unit.field")?;
            validate_unit(&unit.unit, "layer.unit.unit")?;
        }
        require_nonempty(&self.source_ids, "layer.source_ids")?;
        validate_references(&self.source_ids, source_ids, "layer.source_ids")?;
        require_text(&self.transformation, "layer.transformation")
    }

    fn artifact(&self) -> BundleArtifact<'_> {
        match &self.storage {
            DataLayerStorage::Raster {
                artifact_path,
                content_hash,
                byte_length,
                ..
            } => BundleArtifact {
                kind: BundleArtifactKind::NormalizedLayer,
                relative_path: artifact_path,
                content_hash: *content_hash,
                byte_length: *byte_length,
            },
            DataLayerStorage::FullEarthTileTree { tile_tree } => BundleArtifact {
                kind: BundleArtifactKind::TileTreeIndex,
                relative_path: &tile_tree.root_index_path,
                content_hash: tile_tree.root_index_hash,
                byte_length: tile_tree.root_index_byte_length,
            },
        }
    }
}

impl DataLayerStorage {
    fn validate(&self, layer_id: &str, coverage: &WorldDataCoverage) -> Result<(), BundleError> {
        match (self, coverage) {
            (
                Self::Raster {
                    artifact_path,
                    media_type,
                    width_cells,
                    height_cells,
                    content_hash,
                    byte_length,
                },
                WorldDataCoverage::BoundedRaster { spatial_grid },
            ) => {
                validate_artifact_path(artifact_path, "layer.artifact_path")?;
                validate_media_type(media_type)?;
                if *width_cells != spatial_grid.width_cells
                    || *height_cells != spatial_grid.height_cells
                {
                    return Err(BundleError::LayerShapeMismatch(layer_id.to_owned()));
                }
                if *content_hash == Digest::ZERO {
                    return Err(BundleError::ZeroDigest("layer.content_hash"));
                }
                if *byte_length == 0 {
                    return Err(BundleError::ZeroByteLength("layer.byte_length"));
                }
                Ok(())
            }
            (
                Self::FullEarthTileTree { tile_tree },
                WorldDataCoverage::FullEarth {
                    full_earth_grid, ..
                },
            ) => tile_tree.validate(full_earth_grid),
            _ => Err(BundleError::LayerStorageCoverageMismatch(
                layer_id.to_owned(),
            )),
        }
    }
}

impl TileTreeReference {
    fn validate(&self, grid: &FullEarthGrid) -> Result<(), BundleError> {
        if self.index_schema_version != TILE_TREE_INDEX_SCHEMA_VERSION {
            return Err(BundleError::UnsupportedTileIndexSchema(
                self.index_schema_version,
            ));
        }
        validate_artifact_path(&self.root_index_path, "tile_tree.root_index_path")?;
        validate_media_type(&self.root_index_media_type)?;
        if self.root_index_hash == Digest::ZERO {
            return Err(BundleError::ZeroDigest("tile_tree.root_index_hash"));
        }
        if self.root_index_byte_length == 0 {
            return Err(BundleError::ZeroByteLength(
                "tile_tree.root_index_byte_length",
            ));
        }
        if self.leaf_tile_count == 0 {
            return Err(BundleError::ZeroTileCount);
        }
        if self.minimum_s2_level > self.maximum_s2_level
            || self.minimum_s2_level > grid.levels.planetary_aggregate
            || self.maximum_s2_level < grid.levels.planetary_aggregate
            || self.maximum_s2_level > grid.levels.embodied_patch
        {
            return Err(BundleError::InvalidTileLevelRange);
        }
        Ok(())
    }
}

impl TileTreeIndex {
    pub fn validate(&self) -> Result<(), BundleError> {
        if self.index_schema_version != TILE_TREE_INDEX_SCHEMA_VERSION {
            return Err(BundleError::UnsupportedTileIndexSchema(
                self.index_schema_version,
            ));
        }
        validate_slug(&self.layer_id, "tile_index.layer_id")?;
        require_nonempty(&self.entries, "tile_index.entries")?;

        let mut previous: Option<(u8, &str, TileTreeEntryKind)> = None;
        let mut artifact_paths = BTreeSet::new();
        for entry in &self.entries {
            entry.validate()?;
            let key = (entry.s2_level, entry.s2_cell_id.as_str(), entry.kind);
            if let Some(last) = previous {
                if key == last {
                    return Err(BundleError::DuplicateTileTreeEntry {
                        s2_cell_id: entry.s2_cell_id.clone(),
                        s2_level: entry.s2_level,
                        kind: entry.kind,
                    });
                }
                if key < last {
                    return Err(BundleError::UnsortedTileTreeEntries);
                }
            }
            previous = Some(key);

            if !artifact_paths.insert(entry.artifact.path.as_str()) {
                return Err(BundleError::DuplicateIdentifier {
                    field: "tile_index.artifact_paths",
                    value: entry.artifact.path.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn validate_for_tree(
        &self,
        expected_layer_id: &str,
        tree: &TileTreeReference,
    ) -> Result<(), BundleError> {
        self.validate()?;
        if self.index_schema_version != tree.index_schema_version {
            return Err(BundleError::TileIndexSchemaMismatch {
                expected: tree.index_schema_version,
                actual: self.index_schema_version,
            });
        }
        if self.layer_id != expected_layer_id {
            return Err(BundleError::TileIndexLayerMismatch {
                expected: expected_layer_id.to_owned(),
                actual: self.layer_id.clone(),
            });
        }
        for entry in &self.entries {
            entry.validate_for_tree(tree)?;
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, BundleError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| BundleError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, BundleError> {
        let index: Self = serde_json::from_slice(bytes)
            .map_err(|error| BundleError::TileIndexDecode(error.to_string()))?;
        index.validate()?;
        if index.canonical_bytes()? != bytes {
            return Err(BundleError::NonCanonicalTileIndexEncoding);
        }
        Ok(index)
    }
}

impl TileTreeEntry {
    fn validate(&self) -> Result<(), BundleError> {
        let actual_level = s2_level_from_cell_id(&self.s2_cell_id)?;
        if actual_level != self.s2_level {
            return Err(BundleError::S2CellLevelMismatch {
                cell_id: self.s2_cell_id.clone(),
                declared: self.s2_level,
                actual: actual_level,
            });
        }
        self.artifact.validate()
    }

    fn validate_for_tree(&self, tree: &TileTreeReference) -> Result<(), BundleError> {
        let level_is_valid = match self.kind {
            TileTreeEntryKind::Index => self.s2_level <= tree.maximum_s2_level,
            TileTreeEntryKind::Tile => {
                self.s2_level >= tree.minimum_s2_level && self.s2_level <= tree.maximum_s2_level
            }
        };
        if !level_is_valid {
            return Err(BundleError::TileEntryOutsideTreeLevels {
                cell_id: self.s2_cell_id.clone(),
                level: self.s2_level,
                minimum: tree.minimum_s2_level,
                maximum: tree.maximum_s2_level,
            });
        }
        if self.kind == TileTreeEntryKind::Index
            && self.artifact.media_type != tree.root_index_media_type
        {
            return Err(BundleError::TileIndexMediaTypeMismatch {
                expected: tree.root_index_media_type.clone(),
                actual: self.artifact.media_type.clone(),
            });
        }
        Ok(())
    }

    #[must_use]
    pub fn artifact(&self) -> BundleArtifact<'_> {
        BundleArtifact {
            kind: match self.kind {
                TileTreeEntryKind::Index => BundleArtifactKind::TileTreeIndex,
                TileTreeEntryKind::Tile => BundleArtifactKind::Tile,
            },
            relative_path: &self.artifact.path,
            content_hash: self.artifact.content_hash,
            byte_length: self.artifact.byte_length,
        }
    }
}

impl TileArtifactReference {
    fn validate(&self) -> Result<(), BundleError> {
        validate_artifact_path(&self.path, "tile_entry.artifact.path")?;
        validate_media_type(&self.media_type)?;
        if self.content_hash == Digest::ZERO {
            return Err(BundleError::ZeroDigest("tile_entry.artifact.content_hash"));
        }
        if self.byte_length == 0 {
            return Err(BundleError::ZeroByteLength(
                "tile_entry.artifact.byte_length",
            ));
        }
        Ok(())
    }
}

fn s2_level_from_cell_id(value: &str) -> Result<u8, BundleError> {
    value
        .parse::<S2CellId>()
        .map(S2CellId::level)
        .map_err(|_| BundleError::InvalidS2CellId(value.to_owned()))
}

fn validate_references(
    values: &[String],
    available: &BTreeSet<&str>,
    field: &'static str,
) -> Result<(), BundleError> {
    validate_sorted_unique(values.iter().map(String::as_str), field)?;
    for value in values {
        if !available.contains(value.as_str()) {
            return Err(BundleError::MissingReference {
                field,
                value: value.clone(),
            });
        }
    }
    Ok(())
}

fn validate_sorted_unique<'a>(
    values: impl Iterator<Item = &'a str>,
    field: &'static str,
) -> Result<(), BundleError> {
    let mut previous = None;
    for value in values {
        if let Some(last) = previous {
            if value == last {
                return Err(BundleError::DuplicateIdentifier {
                    field,
                    value: value.to_owned(),
                });
            }
            if value < last {
                return Err(BundleError::UnsortedIdentifiers(field));
            }
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_slug(value: &str, field: &'static str) -> Result<(), BundleError> {
    if value.is_empty()
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err(BundleError::InvalidSlug {
            field,
            value: value.to_owned(),
        });
    }
    Ok(())
}

fn validate_semver(value: &str) -> Result<(), BundleError> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(BundleError::InvalidBundleVersion(value.to_owned()));
    }
    Ok(())
}

fn validate_https_url(value: &str, field: &'static str) -> Result<(), BundleError> {
    let parsed = Url::parse(value).ok();
    if parsed.as_ref().is_none_or(|url| {
        url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
    }) {
        return Err(BundleError::InvalidHttpsUrl {
            field,
            value: value.to_owned(),
        });
    }
    Ok(())
}

fn validate_media_type(value: &str) -> Result<(), BundleError> {
    let Some((media_type, media_subtype)) = value.split_once('/') else {
        return Err(BundleError::InvalidMediaType(value.to_owned()));
    };
    if media_type.is_empty()
        || media_subtype.is_empty()
        || media_subtype.contains('/')
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || !byte.is_ascii_graphic())
    {
        return Err(BundleError::InvalidMediaType(value.to_owned()));
    }
    Ok(())
}

fn validate_artifact_path(value: &str, field: &'static str) -> Result<(), BundleError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || !part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_' | b'.')
                })
        })
    {
        return Err(BundleError::InvalidArtifactPath {
            field,
            value: value.to_owned(),
        });
    }
    Ok(())
}

fn validate_unit(value: &str, field: &'static str) -> Result<(), BundleError> {
    if value.is_empty()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || !byte.is_ascii_graphic())
    {
        return Err(BundleError::InvalidUnit {
            field,
            value: value.to_owned(),
        });
    }
    Ok(())
}

fn reject_privileged_affordance(value: &str) -> Result<(), BundleError> {
    if value
        .split('_')
        .any(|part| FORBIDDEN_AFFORDANCE_CODES.contains(&part))
    {
        return Err(BundleError::PrivilegedAffordance(value.to_owned()));
    }
    Ok(())
}

fn require_text(value: &str, field: &'static str) -> Result<(), BundleError> {
    if value.trim().is_empty() {
        return Err(BundleError::MissingText(field));
    }
    Ok(())
}

fn require_nonempty<T>(values: &[T], field: &'static str) -> Result<(), BundleError> {
    if values.is_empty() {
        return Err(BundleError::EmptyCollection(field));
    }
    Ok(())
}

mod u64_decimal {
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

mod i64_decimal {
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

#[derive(Debug, Error)]
pub enum BundleError {
    #[error("world-data bundle schema version {0} is unsupported")]
    UnsupportedSchema(u16),
    #[error("world-data bundle schema {schema} does not match its coverage shape")]
    CoverageSchemaMismatch { schema: u16 },
    #[error("{field} must be a lowercase ASCII slug, found {value:?}")]
    InvalidSlug { field: &'static str, value: String },
    #[error("bundle version must be numeric major.minor.patch, found {0:?}")]
    InvalidBundleVersion(String),
    #[error(
        "normalization source revision must be at least 12 lowercase hexadecimal characters, found {0:?}"
    )]
    InvalidSourceRevision(String),
    #[error("{0} must contain non-whitespace text")]
    MissingText(&'static str),
    #[error("{0} must not be empty")]
    EmptyCollection(&'static str),
    #[error("{field} contains duplicate identifier {value:?}")]
    DuplicateIdentifier { field: &'static str, value: String },
    #[error("{0} identifiers must use ascending byte order")]
    UnsortedIdentifiers(&'static str),
    #[error("{field} references missing identifier {value:?}")]
    MissingReference { field: &'static str, value: String },
    #[error("{field} must be an HTTPS URL without whitespace, found {value:?}")]
    InvalidHttpsUrl { field: &'static str, value: String },
    #[error("{0} must not use the all-zero digest")]
    ZeroDigest(&'static str),
    #[error("{0} must have a positive byte length")]
    ZeroByteLength(&'static str),
    #[error("invalid spatial grid: {0}")]
    InvalidSpatialGrid(String),
    #[error("invalid full-Earth grid: {0}")]
    InvalidFullEarthGrid(String),
    #[error("a source retrieval date falls after the full-Earth manifest cutoff")]
    SourceAfterManifestCutoff,
    #[error("entity {0:?} has no normalized parameters")]
    EntityWithoutParameters(String),
    #[error("assumption {0:?} is never cited by parameter evidence")]
    UnusedAssumption(String),
    #[error("subject {subject_id:?} defines property {property:?} more than once")]
    DuplicateSubjectProperty {
        subject_id: String,
        property: String,
    },
    #[error("property {0:?} encodes a privileged use or invention label")]
    PrivilegedAffordance(String),
    #[error("quantity range must satisfy minimum <= typical <= maximum")]
    InvalidQuantityRange,
    #[error("quantity decimal scale {0} exceeds the schema-v1 maximum")]
    ExcessiveDecimalScale(u8),
    #[error("evidence class {0:?} has incompatible source or assumption links")]
    InvalidEvidenceLinks(EvidenceClass),
    #[error("required normalized layer {0:?} is missing")]
    MissingRequiredLayer(DataLayerKind),
    #[error("layer {0:?} dimensions do not match the bundle grid")]
    LayerShapeMismatch(String),
    #[error("layer {0:?} storage does not match the bundle coverage")]
    LayerStorageCoverageMismatch(String),
    #[error("tile-tree index schema version {0} is unsupported")]
    UnsupportedTileIndexSchema(u16),
    #[error("tile-tree leaf count must be greater than zero")]
    ZeroTileCount,
    #[error("tile-tree S2 levels must include the planetary tier and not exceed the patch tier")]
    InvalidTileLevelRange,
    #[error("tile index entries must use ascending (level, cell, kind) order")]
    UnsortedTileTreeEntries,
    #[error(
        "tile index contains duplicate {kind:?} entry for S2 cell {s2_cell_id} at level {s2_level}"
    )]
    DuplicateTileTreeEntry {
        s2_cell_id: String,
        s2_level: u8,
        kind: TileTreeEntryKind,
    },
    #[error("invalid fixed-width S2 CellId {0:?}")]
    InvalidS2CellId(String),
    #[error(
        "S2 cell {cell_id} declares level {declared}, but its CellId sentinel encodes level {actual}"
    )]
    S2CellLevelMismatch {
        cell_id: String,
        declared: u8,
        actual: u8,
    },
    #[error(
        "tile entry {cell_id} level {level} is outside tree tile range {minimum} through {maximum}"
    )]
    TileEntryOutsideTreeLevels {
        cell_id: String,
        level: u8,
        minimum: u8,
        maximum: u8,
    },
    #[error("tile index schema mismatch: expected {expected}, found {actual}")]
    TileIndexSchemaMismatch { expected: u16, actual: u16 },
    #[error("tile index layer mismatch: expected {expected:?}, found {actual:?}")]
    TileIndexLayerMismatch { expected: String, actual: String },
    #[error("child tile-index media type mismatch: expected {expected:?}, found {actual:?}")]
    TileIndexMediaTypeMismatch { expected: String, actual: String },
    #[error("tile index JSON could not be decoded: {0}")]
    TileIndexDecode(String),
    #[error("tile index bytes are valid JSON but not the canonical schema encoding")]
    NonCanonicalTileIndexEncoding,
    #[error("invalid media type {0:?}")]
    InvalidMediaType(String),
    #[error("{field} must be a safe portable relative path, found {value:?}")]
    InvalidArtifactPath { field: &'static str, value: String },
    #[error("{field} must be a non-empty whitespace-free unit code, found {value:?}")]
    InvalidUnit { field: &'static str, value: String },
    #[error("bundle JSON could not be decoded: {0}")]
    Decode(String),
    #[error("bundle JSON could not be encoded: {0}")]
    Encoding(String),
    #[error("bundle bytes are valid JSON but not the canonical schema encoding")]
    NonCanonicalEncoding,
    #[error("world configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("world configuration does not match bundle field {0}")]
    ConfigurationMismatch(&'static str),
    #[error("artifact byte length cannot be represented on this host")]
    HostLengthOverflow,
    #[error("artifact {path:?} length mismatch: expected {expected}, found {actual}")]
    ArtifactLengthMismatch {
        path: String,
        expected: u64,
        actual: u64,
    },
    #[error("artifact {path:?} digest mismatch: expected {expected}, found {actual}")]
    ArtifactDigestMismatch {
        path: String,
        expected: Digest,
        actual: Digest,
    },
}

#[derive(Debug, Error)]
pub enum SourceSnapshotError {
    #[error("source-snapshot schema version {0} is unsupported")]
    UnsupportedSchema(u16),
    #[error(
        "source-snapshot upstream revision must be at least 12 lowercase hexadecimal characters, found {0:?}"
    )]
    InvalidUpstreamRevision(String),
    #[error("source-snapshot limitation occurs more than once: {0:?}")]
    DuplicateLimitation(String),
    #[error("source snapshot is missing required artifact role {0:?}")]
    MissingArtifactRole(SourceSnapshotArtifactRole),
    #[error("source artifact {path:?} URL does not contain declared revision {revision:?}")]
    ArtifactUrlMissingRevision { path: String, revision: String },
    #[error("source-snapshot artifact digest must not be all zero")]
    ZeroDigest,
    #[error("source-snapshot artifact byte length must be positive")]
    ZeroByteLength,
    #[error("source-snapshot JSON could not be decoded: {0}")]
    Decode(String),
    #[error("source-snapshot JSON could not be encoded: {0}")]
    Encoding(String),
    #[error("source-snapshot bytes are valid JSON but not the canonical schema encoding")]
    NonCanonicalEncoding,
    #[error(transparent)]
    SharedValidation(#[from] BundleError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{
        CapacityExhaustionPolicy, EarthResolutionLevels, FullEarthGrid, PartitionedExecution,
        PersonRepresentation, S2Projection, SchedulerKind, WorldConfiguration,
        WorldDataBundleReference, WorldGeometry,
    };

    const SOURCE_ARTIFACT: &[u8] = b"source artifact fixture";
    const NATURAL_EARTH_10M_LAND_SNAPSHOT: &[u8] =
        include_bytes!("../../../data/source-snapshots/natural-earth-10m-land-v5.1.2.json");

    fn source_snapshot() -> SourceSnapshotManifest {
        let artifact = |role, path: &str, bytes: &[u8]| SourceSnapshotArtifact {
            role,
            artifact_path: path.to_owned(),
            download_url: format!(
                "https://example.test/0123456789abcdef0123456789abcdef01234567/{path}"
            ),
            media_type: "application/octet-stream".to_owned(),
            content_hash: Digest::sha256(bytes),
            byte_length: u64::try_from(bytes.len()).expect("fixture length fits u64"),
        };
        SourceSnapshotManifest {
            source_snapshot_schema_version: SOURCE_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: "global-land-v1".to_owned(),
            title: "Global land evidence".to_owned(),
            publisher: "Example publisher".to_owned(),
            documentation_url: "https://example.test/global-land".to_owned(),
            upstream_release: "v1.2.3".to_owned(),
            upstream_revision: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            artifact_locator_policy: SourceSnapshotLocatorPolicy::RevisionInEveryArtifactUrl,
            dataset_version: "1.2.2".to_owned(),
            retrieved_on: NaiveDate::from_ymd_opt(2026, 8, 6).expect("valid fixture date"),
            license_expression: "LicenseRef-Example-Public-Domain".to_owned(),
            license_url: "https://example.test/terms".to_owned(),
            scope: "Generalized global land polygons.".to_owned(),
            limitations: vec!["Not a simulation-ready coastline.".to_owned()],
            artifacts: vec![
                artifact(SourceSnapshotArtifactRole::Data, "data/land.shp", b"shape"),
                artifact(
                    SourceSnapshotArtifactRole::Documentation,
                    "docs/readme.html",
                    b"documentation",
                ),
                artifact(
                    SourceSnapshotArtifactRole::LicenseEvidence,
                    "docs/terms.txt",
                    b"public domain",
                ),
                artifact(
                    SourceSnapshotArtifactRole::VersionEvidence,
                    "docs/version.txt",
                    b"1.2.2",
                ),
            ],
        }
    }

    #[test]
    fn source_snapshot_round_trips_only_as_canonical_complete_evidence() {
        let snapshot = source_snapshot();
        let bytes = snapshot.canonical_bytes().expect("valid source snapshot");
        let decoded = SourceSnapshotManifest::from_canonical_slice(&bytes)
            .expect("canonical snapshot round trip");
        assert_eq!(decoded, snapshot);
        assert_eq!(
            decoded.content_digest().expect("valid content digest"),
            Digest::sha256(&bytes)
        );

        let mut pretty = serde_json::to_vec_pretty(&snapshot).expect("pretty snapshot JSON");
        pretty.push(b'\n');
        assert!(matches!(
            SourceSnapshotManifest::from_canonical_slice(&pretty),
            Err(SourceSnapshotError::NonCanonicalEncoding)
        ));
    }

    #[test]
    fn source_snapshot_rejects_ambiguous_provenance_and_artifacts() {
        let mut missing_role = source_snapshot();
        missing_role
            .artifacts
            .retain(|artifact| artifact.role != SourceSnapshotArtifactRole::LicenseEvidence);
        assert!(matches!(
            missing_role.validate(),
            Err(SourceSnapshotError::MissingArtifactRole(
                SourceSnapshotArtifactRole::LicenseEvidence
            ))
        ));

        let mut unsorted = source_snapshot();
        unsorted.artifacts.swap(0, 1);
        assert!(unsorted.validate().is_err());

        let mut unsafe_path = source_snapshot();
        unsafe_path.artifacts[0].artifact_path = "../land.shp".to_owned();
        assert!(unsafe_path.validate().is_err());

        let mut mutable_revision = source_snapshot();
        mutable_revision.upstream_revision = "main".to_owned();
        assert!(matches!(
            mutable_revision.validate(),
            Err(SourceSnapshotError::InvalidUpstreamRevision(_))
        ));

        let mut unbound_url = source_snapshot();
        unbound_url.artifacts[0].download_url = "https://example.test/source.bin".to_owned();
        assert!(matches!(
            unbound_url.validate(),
            Err(SourceSnapshotError::ArtifactUrlMissingRevision { .. })
        ));

        let mut duplicate_limitation = source_snapshot();
        duplicate_limitation
            .limitations
            .push(duplicate_limitation.limitations[0].clone());
        assert!(matches!(
            duplicate_limitation.validate(),
            Err(SourceSnapshotError::DuplicateLimitation(_))
        ));
    }

    #[test]
    fn source_snapshot_artifact_observation_checks_length_and_digest() {
        let artifact = &source_snapshot().artifacts[0];
        let expected = artifact.expected_artifact();
        expected.verify_bytes(b"shape").expect("valid bytes");
        assert!(matches!(
            expected.verify_bytes(b"shap"),
            Err(BundleError::ArtifactLengthMismatch { .. })
        ));
        assert!(matches!(
            expected.verify_bytes(b"other"),
            Err(BundleError::ArtifactDigestMismatch { .. })
        ));
    }

    #[test]
    fn committed_natural_earth_snapshot_is_canonical_and_fingerprinted() {
        let snapshot =
            SourceSnapshotManifest::from_canonical_slice(NATURAL_EARTH_10M_LAND_SNAPSHOT)
                .expect("committed Natural Earth snapshot is canonical");
        assert_eq!(snapshot.upstream_release, "v5.1.2");
        assert_eq!(snapshot.dataset_version, "5.1.1");
        assert_eq!(snapshot.artifacts.len(), 9);
        assert!(
            snapshot
                .artifacts
                .iter()
                .all(|artifact| artifact.download_url.contains(&snapshot.upstream_revision))
        );
        assert_eq!(
            snapshot
                .artifacts
                .iter()
                .map(|artifact| artifact.byte_length)
                .sum::<u64>(),
            7_209_312
        );
        assert_eq!(
            snapshot
                .content_digest()
                .expect("valid snapshot content digest")
                .to_string(),
            "21382550977608ef2f8e3f4f787a987d7c06848560fcd8902b6a44e7857b427a"
        );
    }

    fn grid() -> SpatialGrid {
        SpatialGrid {
            epsg: 26_915,
            origin_easting_mm: 500_000_000,
            origin_northing_mm: 3_980_000_000,
            cell_size_mm: 32_000,
            width_cells: 256,
            height_cells: 256,
        }
    }

    fn source() -> SourceRecord {
        SourceRecord {
            source_id: "usgs-water-reference".to_owned(),
            title: "Water reference fixture".to_owned(),
            publisher: "U.S. Geological Survey".to_owned(),
            canonical_url: "https://www.usgs.gov/".to_owned(),
            version: "retrieved-2026-08-06".to_owned(),
            retrieved_on: NaiveDate::from_ymd_opt(2026, 8, 6).expect("valid fixed date"),
            license_expression: "LicenseRef-US-Public-Domain".to_owned(),
            artifact_path: "sources/usgs-water-reference.html".to_owned(),
            artifact_media_type: "text/html".to_owned(),
            artifact_hash: Digest::sha256(SOURCE_ARTIFACT),
            artifact_byte_length: u64::try_from(SOURCE_ARTIFACT.len())
                .expect("fixture length fits u64"),
        }
    }

    fn layer(kind: DataLayerKind, id: &str, salt: u8) -> DataLayer {
        DataLayer {
            layer_id: id.to_owned(),
            kind,
            storage: DataLayerStorage::Raster {
                artifact_path: format!("layers/{id}.grid"),
                media_type: "application/vnd.atinycivilization.grid+i32".to_owned(),
                width_cells: 256,
                height_cells: 256,
                content_hash: Digest::from_bytes([salt; 32]),
                byte_length: 262_144,
            },
            units: vec![FieldUnit {
                field: "value".to_owned(),
                unit: "1".to_owned(),
            }],
            source_ids: vec!["usgs-water-reference".to_owned()],
            transformation: "Schema test normalization; not a scientific release.".to_owned(),
        }
    }

    fn bundle() -> WorldDataBundle {
        WorldDataBundle {
            bundle_schema_version: LEGACY_WORLD_DATA_BUNDLE_SCHEMA_VERSION,
            bundle_id: "bundle-schema-test".to_owned(),
            bundle_version: "0.1.0".to_owned(),
            title: "World-data schema test fixture".to_owned(),
            license_expression: "LicenseRef-US-Public-Domain".to_owned(),
            reference_domain: ReferenceDomain {
                name: "Schema test domain".to_owned(),
                description: "A code-only fixture that is never eligible as public-world data."
                    .to_owned(),
                source_ids: vec!["usgs-water-reference".to_owned()],
            },
            coverage: WorldDataCoverage::BoundedRaster {
                spatial_grid: grid(),
            },
            normalization: NormalizationRecord {
                pipeline_id: "bundle-schema-test".to_owned(),
                pipeline_version: "0.1.0".to_owned(),
                source_revision: "037b2b73b523".to_owned(),
                executable_hash: Digest::sha256(b"normalizer fixture"),
            },
            sources: vec![source()],
            assumptions: Vec::new(),
            entities: vec![CatalogEntity {
                entity_id: "water".to_owned(),
                kind: CatalogEntityKind::ChemicalSubstance,
                canonical_name: "water".to_owned(),
                scientific_name: None,
                external_identities: vec![ExternalIdentity {
                    authority: "pubchem".to_owned(),
                    identifier: "962".to_owned(),
                    url: "https://pubchem.ncbi.nlm.nih.gov/compound/962".to_owned(),
                }],
                source_ids: vec!["usgs-water-reference".to_owned()],
            }],
            parameters: vec![ParameterRecord {
                parameter_id: "water-density".to_owned(),
                subject_id: "water".to_owned(),
                property: "density".to_owned(),
                value: ParameterValue::Quantity(ScaledRange {
                    minimum: 997,
                    typical: 998,
                    maximum: 1_000,
                    decimal_places: 0,
                    unit: "kg/m3".to_owned(),
                }),
                evidence: EvidenceRecord {
                    classification: EvidenceClass::LiteratureApproximation,
                    source_ids: vec!["usgs-water-reference".to_owned()],
                    assumption_ids: Vec::new(),
                    method: "Schema fixture range; not a released world parameter.".to_owned(),
                },
            }],
            layers: vec![
                layer(DataLayerKind::Climate, "climate", 1),
                layer(DataLayerKind::Elevation, "elevation", 2),
                layer(DataLayerKind::Habitat, "habitat", 3),
                layer(DataLayerKind::Hydrography, "hydrography", 4),
                layer(DataLayerKind::Soil, "soil", 5),
            ],
        }
    }

    fn configuration(bundle: &WorldDataBundle) -> WorldConfiguration {
        let digest = bundle.content_digest().expect("valid bundle digest");
        WorldConfiguration::new(
            300,
            grid(),
            WorldDataBundleReference::new(
                bundle.bundle_schema_version,
                bundle.bundle_id.clone(),
                bundle.bundle_version.clone(),
                digest,
                "https://data.atinycivilization.com/tests/bundle.json",
                bundle.license_expression.clone(),
            )
            .expect("valid bundle reference"),
            10_000,
        )
        .expect("valid configuration")
    }

    fn full_earth_grid() -> FullEarthGrid {
        FullEarthGrid {
            physics_crs_epsg: 4_978,
            catalog_crs_epsg: 4_979,
            vertical_crs_epsg: 3_855,
            s2_definition_url: "https://s2geometry.io/devguide/s2cell_hierarchy".to_owned(),
            s2_library_revision: "0123456789abcdef".to_owned(),
            s2_definition_hash: Digest::sha256(b"world-data S2 fixture"),
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

    fn tiled_layer(kind: DataLayerKind, id: &str, salt: u8) -> DataLayer {
        DataLayer {
            layer_id: id.to_owned(),
            kind,
            storage: DataLayerStorage::FullEarthTileTree {
                tile_tree: TileTreeReference {
                    index_schema_version: 1,
                    root_index_path: format!("layers/{id}/root.index"),
                    root_index_media_type: "application/vnd.atinycivilization.tile-index+json"
                        .to_owned(),
                    root_index_hash: Digest::from_bytes([salt; 32]),
                    root_index_byte_length: 4_096,
                    leaf_tile_count: 6_291_456,
                    minimum_s2_level: 10,
                    maximum_s2_level: 23,
                },
            },
            units: vec![FieldUnit {
                field: "value".to_owned(),
                unit: "1".to_owned(),
            }],
            source_ids: vec!["usgs-water-reference".to_owned()],
            transformation: "Global tile-tree schema fixture; not a scientific release.".to_owned(),
        }
    }

    fn tile_tree_reference() -> TileTreeReference {
        TileTreeReference {
            index_schema_version: TILE_TREE_INDEX_SCHEMA_VERSION,
            root_index_path: "layers/elevation/root.index".to_owned(),
            root_index_media_type: "application/vnd.atinycivilization.tile-index+json".to_owned(),
            root_index_hash: Digest::sha256(b"root index fixture"),
            root_index_byte_length: 512,
            leaf_tile_count: 1,
            minimum_s2_level: 10,
            maximum_s2_level: 23,
        }
    }

    fn tile_entry(
        kind: TileTreeEntryKind,
        s2_cell_id: &str,
        s2_level: u8,
        path: &str,
        bytes: &[u8],
    ) -> TileTreeEntry {
        TileTreeEntry {
            kind,
            s2_cell_id: s2_cell_id.to_owned(),
            s2_level,
            artifact: TileArtifactReference {
                path: path.to_owned(),
                media_type: match kind {
                    TileTreeEntryKind::Index => "application/vnd.atinycivilization.tile-index+json",
                    TileTreeEntryKind::Tile => "application/vnd.atinycivilization.tile+i32",
                }
                .to_owned(),
                content_hash: Digest::sha256(bytes),
                byte_length: u64::try_from(bytes.len()).expect("fixture length fits u64"),
            },
        }
    }

    fn tile_index() -> TileTreeIndex {
        TileTreeIndex {
            index_schema_version: TILE_TREE_INDEX_SCHEMA_VERSION,
            layer_id: "elevation".to_owned(),
            entries: vec![
                tile_entry(
                    TileTreeEntryKind::Index,
                    "1000000000000000",
                    0,
                    "layers/elevation/face-0.index",
                    b"child index fixture",
                ),
                tile_entry(
                    TileTreeEntryKind::Tile,
                    "0000010000000000",
                    10,
                    "layers/elevation/l10/0000010000000000.tile",
                    b"tile fixture",
                ),
            ],
        }
    }

    fn full_earth_bundle() -> WorldDataBundle {
        let mut bundle = bundle();
        bundle.bundle_schema_version = WORLD_DATA_BUNDLE_SCHEMA_VERSION;
        bundle.bundle_id = "full-earth-schema-test".to_owned();
        bundle.reference_domain = ReferenceDomain {
            name: "Full Earth schema fixture".to_owned(),
            description: "Code-only full-Earth coverage fixture; not scientific data.".to_owned(),
            source_ids: vec!["usgs-water-reference".to_owned()],
        };
        bundle.coverage = WorldDataCoverage::FullEarth {
            full_earth_grid: full_earth_grid(),
            earth_baseline: EarthBaseline {
                manifest_cutoff_date: NaiveDate::from_ymd_opt(2026, 8, 6)
                    .expect("valid fixture date"),
                source_epoch_policy: SourceEpochPolicy::PerSourceEpochComposite,
                human_feature_policy: HumanFeaturePolicy::ExcludeDirectFeaturesAndFlagInferences,
                sensitive_location_policy: SensitiveLocationPolicy::OmitOrGeneralize,
                sea_level_definition: "Pinned mean-sea-level surface fixture.".to_owned(),
            },
        };
        bundle.layers = vec![
            tiled_layer(DataLayerKind::Bathymetry, "bathymetry", 1),
            tiled_layer(DataLayerKind::Climate, "climate", 2),
            tiled_layer(DataLayerKind::Coastline, "coastline", 3),
            tiled_layer(DataLayerKind::Elevation, "elevation", 4),
            tiled_layer(DataLayerKind::Habitat, "habitat", 5),
            tiled_layer(DataLayerKind::Hydrography, "hydrography", 6),
            tiled_layer(DataLayerKind::Soil, "soil", 7),
        ];
        bundle
    }

    fn full_earth_configuration(bundle: &WorldDataBundle) -> WorldConfiguration {
        let digest = bundle.content_digest().expect("valid full-Earth digest");
        WorldConfiguration::new_full_earth(
            300,
            full_earth_grid(),
            WorldDataBundleReference::new(
                bundle.bundle_schema_version,
                bundle.bundle_id.clone(),
                bundle.bundle_version.clone(),
                digest,
                "https://data.atinycivilization.com/tests/full-earth-bundle.json",
                bundle.license_expression.clone(),
            )
            .expect("valid bundle reference"),
            PartitionedExecution {
                scheduler_schema_version: 1,
                scheduler: SchedulerKind::DeterministicEventQueue,
                partition_s2_level: 10,
                person_representation: PersonRepresentation::DurableIndividuals,
                capacity_exhaustion: CapacityExhaustionPolicy::PauseAtCommittedBoundary,
                max_events_per_partition_transition: 10_000,
            },
        )
        .expect("valid full-Earth configuration")
    }

    #[test]
    fn canonical_bundle_matches_pinned_world_configuration() {
        let bundle = bundle();
        let bytes = bundle.canonical_bytes().expect("valid canonical bytes");
        let encoded = std::str::from_utf8(&bytes).expect("canonical JSON is UTF-8");
        assert!(encoded.contains("\"minimum\":\"997\""));
        assert!(encoded.contains("\"artifact_byte_length\":\"23\""));
        let decoded = WorldDataBundle::from_canonical_slice(&bytes).expect("valid bundle bytes");
        assert_eq!(decoded, bundle);
        assert!(
            bundle
                .validate_for_configuration(&configuration(&bundle))
                .is_ok()
        );
    }

    #[test]
    fn pretty_json_is_not_canonical_input() {
        let bytes = serde_json::to_vec_pretty(&bundle()).expect("serializable fixture");
        assert!(matches!(
            WorldDataBundle::from_canonical_slice(&bytes),
            Err(BundleError::NonCanonicalEncoding)
        ));
    }

    #[test]
    fn referenced_artifact_bytes_are_verified() {
        let bundle = bundle();
        let artifacts = bundle.artifacts().expect("valid artifact descriptors");
        let source = artifacts
            .iter()
            .find(|artifact| artifact.kind == BundleArtifactKind::Source)
            .expect("source artifact descriptor");
        assert!(source.verify_bytes(SOURCE_ARTIFACT).is_ok());
        assert!(matches!(
            source.verify_bytes(b"changed source artifact"),
            Err(BundleError::ArtifactLengthMismatch { .. })
                | Err(BundleError::ArtifactDigestMismatch { .. })
        ));
    }

    #[test]
    fn dangling_provenance_and_privileged_properties_are_rejected() {
        let mut dangling = bundle();
        dangling.parameters[0].evidence.source_ids = vec!["unknown-source".to_owned()];
        assert!(matches!(
            dangling.validate(),
            Err(BundleError::MissingReference {
                field: "evidence.source_ids",
                ..
            })
        ));

        let mut privileged = bundle();
        privileged.parameters[0].property = "preferred_tool_material".to_owned();
        assert!(matches!(
            privileged.validate(),
            Err(BundleError::PrivilegedAffordance(_))
        ));
    }

    #[test]
    fn assumptions_and_required_layers_are_enforced() {
        let mut unused = bundle();
        unused.assumptions.push(AssumptionRecord {
            assumption_id: "test-assumption".to_owned(),
            statement: "Fixture statement".to_owned(),
            rationale: "Fixture rationale".to_owned(),
        });
        assert!(matches!(
            unused.validate(),
            Err(BundleError::UnusedAssumption(_))
        ));

        let mut missing = bundle();
        missing.layers.remove(0);
        assert!(matches!(
            missing.validate(),
            Err(BundleError::MissingRequiredLayer(DataLayerKind::Climate))
        ));
    }

    #[test]
    fn configuration_mismatch_cannot_be_hidden_by_valid_json() {
        let bundle = bundle();
        let mut configuration = configuration(&bundle);
        let WorldGeometry::BoundedRaster { spatial_grid } = &mut configuration.geometry else {
            panic!("legacy fixture must use a raster");
        };
        spatial_grid.cell_size_mm = 64_000;
        assert!(matches!(
            bundle.validate_for_configuration(&configuration),
            Err(BundleError::ConfigurationMismatch("coverage"))
        ));
    }

    #[test]
    fn full_earth_bundle_requires_global_layers_and_matches_partitioned_configuration() {
        let bundle = full_earth_bundle();
        assert!(bundle.validate().is_ok());
        assert!(
            bundle
                .validate_for_configuration(&full_earth_configuration(&bundle))
                .is_ok()
        );

        let bytes = bundle
            .canonical_bytes()
            .expect("canonical full-Earth bytes");
        let encoded = std::str::from_utf8(&bytes).expect("UTF-8 bundle");
        assert!(encoded.contains("\"full_earth_grid\""));
        assert!(encoded.contains("\"tile_tree\""));
        assert!(encoded.contains("\"exclude_direct_features_and_flag_inferences\""));

        let mut missing_coastline = bundle;
        missing_coastline
            .layers
            .retain(|layer| layer.kind != DataLayerKind::Coastline);
        assert!(matches!(
            missing_coastline.validate(),
            Err(BundleError::MissingRequiredLayer(DataLayerKind::Coastline))
        ));
    }

    #[test]
    fn tile_index_has_canonical_bytes_and_structural_s2_validation() {
        let index = tile_index();
        index
            .validate_for_tree("elevation", &tile_tree_reference())
            .expect("valid index fixture");
        let bytes = index.canonical_bytes().expect("canonical index bytes");
        assert_eq!(
            TileTreeIndex::from_canonical_slice(&bytes).expect("canonical index decodes"),
            index
        );

        let tile = index
            .entries
            .iter()
            .find(|entry| entry.kind == TileTreeEntryKind::Tile)
            .expect("tile entry");
        tile.artifact()
            .verify_bytes(b"tile fixture")
            .expect("tile digest matches");

        let pretty = serde_json::to_vec_pretty(&index).expect("pretty index JSON");
        assert!(matches!(
            TileTreeIndex::from_canonical_slice(&pretty),
            Err(BundleError::NonCanonicalTileIndexEncoding)
        ));

        assert!(matches!(
            index.validate_for_tree("soil", &tile_tree_reference()),
            Err(BundleError::TileIndexLayerMismatch { .. })
        ));

        let mut wrong_index_media_type = index;
        wrong_index_media_type.entries[0].artifact.media_type =
            "application/vnd.atinycivilization.tile+i32".to_owned();
        assert!(matches!(
            wrong_index_media_type.validate_for_tree("elevation", &tile_tree_reference()),
            Err(BundleError::TileIndexMediaTypeMismatch { .. })
        ));
    }

    #[test]
    fn malformed_duplicate_and_out_of_range_tile_entries_are_rejected() {
        let mut bad_cell = tile_index();
        bad_cell.entries[1].s2_cell_id = "D000000000000000".to_owned();
        assert!(matches!(
            bad_cell.validate(),
            Err(BundleError::InvalidS2CellId(_))
        ));

        let mut wrong_level = tile_index();
        wrong_level.entries[1].s2_level = 11;
        assert!(matches!(
            wrong_level.validate(),
            Err(BundleError::S2CellLevelMismatch { .. })
        ));

        let mut duplicate = tile_index();
        duplicate.entries.push(duplicate.entries[1].clone());
        assert!(matches!(
            duplicate.validate(),
            Err(BundleError::DuplicateTileTreeEntry { .. })
        ));

        let mut unsorted = tile_index();
        unsorted.entries.reverse();
        assert!(matches!(
            unsorted.validate(),
            Err(BundleError::UnsortedTileTreeEntries)
        ));

        let mut outside = tile_index();
        outside.entries[1] = tile_entry(
            TileTreeEntryKind::Tile,
            "0000000100000000",
            14,
            "layers/elevation/l14/0000000100000000.tile",
            b"tile fixture",
        );
        let mut tree = tile_tree_reference();
        tree.maximum_s2_level = 10;
        assert!(matches!(
            outside.validate_for_tree("elevation", &tree),
            Err(BundleError::TileEntryOutsideTreeLevels { .. })
        ));
    }
}
