//! Deterministic, provenance-complete scientific input bundles.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use world_domain::{Digest, SpatialGrid, WorldConfiguration};

pub const WORLD_DATA_BUNDLE_SCHEMA_VERSION: u16 = 1;
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
    pub spatial_grid: SpatialGrid,
    pub normalization: NormalizationRecord,
    pub sources: Vec<SourceRecord>,
    pub assumptions: Vec<AssumptionRecord>,
    pub entities: Vec<CatalogEntity>,
    pub parameters: Vec<ParameterRecord>,
    pub layers: Vec<DataLayer>,
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
    Climate,
    Elevation,
    Habitat,
    Hydrography,
    Soil,
}

impl DataLayerKind {
    const REQUIRED: [Self; 5] = [
        Self::Climate,
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
    pub artifact_path: String,
    pub media_type: String,
    pub width_cells: u32,
    pub height_cells: u32,
    pub content_hash: Digest,
    #[serde(with = "u64_decimal")]
    pub byte_length: u64,
    pub units: Vec<FieldUnit>,
    pub source_ids: Vec<String>,
    pub transformation: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundleArtifactKind {
    Source,
    NormalizedLayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleArtifact<'a> {
    pub kind: BundleArtifactKind,
    pub relative_path: &'a str,
    pub content_hash: Digest,
    pub byte_length: u64,
}

impl WorldDataBundle {
    pub fn validate(&self) -> Result<(), BundleError> {
        if self.bundle_schema_version != WORLD_DATA_BUNDLE_SCHEMA_VERSION {
            return Err(BundleError::UnsupportedSchema(self.bundle_schema_version));
        }
        validate_slug(&self.bundle_id, "bundle_id")?;
        validate_semver(&self.bundle_version)?;
        require_text(&self.title, "title")?;
        require_text(&self.license_expression, "license_expression")?;
        self.spatial_grid
            .validate()
            .map_err(|error| BundleError::InvalidSpatialGrid(error.to_string()))?;
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
            layer.validate(&source_ids, &self.spatial_grid)?;
            layer_kinds.insert(layer.kind);
        }
        for required in DataLayerKind::REQUIRED {
            if !layer_kinds.contains(&required) {
                return Err(BundleError::MissingRequiredLayer(required));
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
        if configuration.spatial_grid != self.spatial_grid {
            return Err(BundleError::ConfigurationMismatch("spatial_grid"));
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
            .chain(self.layers.iter().map(|layer| BundleArtifact {
                kind: BundleArtifactKind::NormalizedLayer,
                relative_path: &layer.artifact_path,
                content_hash: layer.content_hash,
                byte_length: layer.byte_length,
            }))
            .collect()
    }
}

impl BundleArtifact<'_> {
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<(), BundleError> {
        let actual_length =
            u64::try_from(bytes.len()).map_err(|_| BundleError::HostLengthOverflow)?;
        if actual_length != self.byte_length {
            return Err(BundleError::ArtifactLengthMismatch {
                path: self.relative_path.to_owned(),
                expected: self.byte_length,
                actual: actual_length,
            });
        }
        let actual = Digest::sha256(bytes);
        if actual != self.content_hash {
            return Err(BundleError::ArtifactDigestMismatch {
                path: self.relative_path.to_owned(),
                expected: self.content_hash,
                actual,
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
    fn validate(&self, source_ids: &BTreeSet<&str>, grid: &SpatialGrid) -> Result<(), BundleError> {
        validate_slug(&self.layer_id, "layer_id")?;
        validate_artifact_path(&self.artifact_path, "layer.artifact_path")?;
        validate_media_type(&self.media_type)?;
        if self.width_cells != grid.width_cells || self.height_cells != grid.height_cells {
            return Err(BundleError::LayerShapeMismatch(self.layer_id.clone()));
        }
        if self.content_hash == Digest::ZERO {
            return Err(BundleError::ZeroDigest("layer.content_hash"));
        }
        if self.byte_length == 0 {
            return Err(BundleError::ZeroByteLength("layer.byte_length"));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{WorldConfiguration, WorldDataBundleReference};

    const SOURCE_ARTIFACT: &[u8] = b"source artifact fixture";

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
            artifact_path: format!("layers/{id}.grid"),
            media_type: "application/vnd.atinycivilization.grid+i32".to_owned(),
            width_cells: 256,
            height_cells: 256,
            content_hash: Digest::from_bytes([salt; 32]),
            byte_length: 262_144,
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
            bundle_schema_version: WORLD_DATA_BUNDLE_SCHEMA_VERSION,
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
            spatial_grid: grid(),
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
                WORLD_DATA_BUNDLE_SCHEMA_VERSION,
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
        configuration.spatial_grid.cell_size_mm = 64_000;
        assert!(matches!(
            bundle.validate_for_configuration(&configuration),
            Err(BundleError::ConfigurationMismatch("spatial_grid"))
        ));
    }
}
