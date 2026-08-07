//! Canonical composition manifest for an end-to-end provisional world.
//!
//! This type deliberately cannot decode as [`crate::WorldDataBundle`]. It proves that
//! every breadth-first input is present and content-addressed without claiming that the
//! coupled system is scientifically admitted or eligible for canonical genesis.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, FullEarthGrid, ProvisionalWorldCompositionReference};

use crate::DataLayerKind;

pub const PROVISIONAL_WORLD_COMPOSITION_SCHEMA_VERSION: u16 = 1;
pub const PROVISIONAL_WORLD_COMPOSITION_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.provisional-world-composition+json";

const REQUIRED_EARTH_LAYERS: [DataLayerKind; 7] = [
    DataLayerKind::Bathymetry,
    DataLayerKind::Climate,
    DataLayerKind::Coastline,
    DataLayerKind::Elevation,
    DataLayerKind::Habitat,
    DataLayerKind::Hydrography,
    DataLayerKind::Soil,
];

const REQUIRED_WORLD_COMPONENTS: [ProvisionalWorldComponentKind; 4] = [
    ProvisionalWorldComponentKind::CelestialEphemeris,
    ProvisionalWorldComponentKind::FaunaCatalog,
    ProvisionalWorldComponentKind::FaunaTraitEvidence,
    ProvisionalWorldComponentKind::FaunaPhysiologyEvidence,
];

/// The only state representable by this schema.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProvisionalWorldCompositionStatus {
    #[serde(rename = "provisional-not-scientifically-admitted")]
    ProvisionalNotScientificallyAdmitted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvisionalWorldComponentKind {
    CelestialEphemeris,
    FaunaCatalog,
    FaunaTraitEvidence,
    FaunaPhysiologyEvidence,
}

/// One exact local release artifact and its honest current scope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalArtifactReference {
    pub artifact_id: String,
    pub artifact_path: String,
    pub media_type: String,
    pub content_hash: Digest,
    #[serde(with = "crate::u64_decimal")]
    pub byte_length: u64,
    pub license_expression: String,
    pub scientific_scope: String,
    pub limitations: Vec<String>,
}

impl ProvisionalArtifactReference {
    fn validate(&self) -> Result<(), ProvisionalWorldCompositionError> {
        if !slug(&self.artifact_id) {
            return Err(ProvisionalWorldCompositionError::InvalidIdentifier);
        }
        validate_relative_path(&self.artifact_path)?;
        if !media_type(&self.media_type) {
            return Err(ProvisionalWorldCompositionError::InvalidMediaType);
        }
        if self.content_hash == Digest::ZERO {
            return Err(ProvisionalWorldCompositionError::ZeroDigest);
        }
        if self.byte_length == 0 {
            return Err(ProvisionalWorldCompositionError::ZeroByteLength);
        }
        if self.license_expression.trim().is_empty()
            || is_noncommercial_license(&self.license_expression)
        {
            return Err(ProvisionalWorldCompositionError::UnusableLicense);
        }
        if self.scientific_scope.trim().is_empty() {
            return Err(ProvisionalWorldCompositionError::MissingScientificScope);
        }
        validate_canonical_text_set(&self.limitations, "limitations")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalEarthLayerReference {
    pub kind: DataLayerKind,
    pub release: ProvisionalArtifactReference,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalWorldComponentReference {
    pub kind: ProvisionalWorldComponentKind,
    pub release: ProvisionalArtifactReference,
}

/// Complete breadth-first composition, structurally barred from scientific admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalWorldComposition {
    pub composition_schema_version: u16,
    pub composition_id: String,
    pub composition_version: String,
    pub status: ProvisionalWorldCompositionStatus,
    pub full_earth_grid: FullEarthGrid,
    pub earth_layers: Vec<ProvisionalEarthLayerReference>,
    pub world_components: Vec<ProvisionalWorldComponentReference>,
    pub coupled_validation_gaps: Vec<String>,
}

impl ProvisionalWorldComposition {
    pub fn validate(&self) -> Result<(), ProvisionalWorldCompositionError> {
        if self.composition_schema_version != PROVISIONAL_WORLD_COMPOSITION_SCHEMA_VERSION {
            return Err(ProvisionalWorldCompositionError::UnsupportedSchema(
                self.composition_schema_version,
            ));
        }
        if !slug(&self.composition_id) || !semver(&self.composition_version) {
            return Err(ProvisionalWorldCompositionError::InvalidIdentifier);
        }
        self.full_earth_grid
            .validate()
            .map_err(|error| ProvisionalWorldCompositionError::InvalidGrid(error.to_string()))?;

        if self.earth_layers.len() != REQUIRED_EARTH_LAYERS.len() {
            return Err(ProvisionalWorldCompositionError::IncompleteEarthLayers);
        }
        for (layer, expected) in self.earth_layers.iter().zip(REQUIRED_EARTH_LAYERS) {
            if layer.kind != expected {
                return Err(ProvisionalWorldCompositionError::NonCanonicalEarthLayerOrder);
            }
            layer.release.validate()?;
        }

        if self.world_components.len() != REQUIRED_WORLD_COMPONENTS.len() {
            return Err(ProvisionalWorldCompositionError::IncompleteWorldComponents);
        }
        for (component, expected) in self.world_components.iter().zip(REQUIRED_WORLD_COMPONENTS) {
            if component.kind != expected {
                return Err(ProvisionalWorldCompositionError::NonCanonicalComponentOrder);
            }
            component.release.validate()?;
        }

        validate_canonical_text_set(&self.coupled_validation_gaps, "coupled_validation_gaps")?;

        let mut artifact_ids = self
            .earth_layers
            .iter()
            .map(|layer| layer.release.artifact_id.as_str())
            .chain(
                self.world_components
                    .iter()
                    .map(|component| component.release.artifact_id.as_str()),
            )
            .collect::<Vec<_>>();
        artifact_ids.sort_unstable();
        if artifact_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ProvisionalWorldCompositionError::DuplicateArtifactIdentifier);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProvisionalWorldCompositionError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self)
            .map_err(|error| ProvisionalWorldCompositionError::Encoding(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn content_digest(&self) -> Result<Digest, ProvisionalWorldCompositionError> {
        Ok(Digest::sha256(&self.canonical_bytes()?))
    }

    /// Produce the only domain reference that provisional execution is allowed to
    /// commit. The distinct type prevents this composition from being mistaken for
    /// a scientifically admitted world-data bundle.
    pub fn execution_reference(
        &self,
    ) -> Result<ProvisionalWorldCompositionReference, ProvisionalWorldCompositionError> {
        self.validate()?;
        ProvisionalWorldCompositionReference::new(
            self.composition_schema_version,
            self.composition_id.clone(),
            self.composition_version.clone(),
            self.content_digest()?,
        )
        .map_err(|error| {
            ProvisionalWorldCompositionError::InvalidExecutionReference(error.to_string())
        })
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, ProvisionalWorldCompositionError> {
        let composition: Self = serde_json::from_slice(bytes)
            .map_err(|error| ProvisionalWorldCompositionError::Decode(error.to_string()))?;
        composition.validate()?;
        if composition.canonical_bytes()? != bytes {
            return Err(ProvisionalWorldCompositionError::NonCanonicalEncoding);
        }
        Ok(composition)
    }
}

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn semver(value: &str) -> bool {
    let mut parts = value.split('.');
    let valid = (0..3).all(|_| {
        parts
            .next()
            .is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    });
    valid && parts.next().is_none()
}

fn media_type(value: &str) -> bool {
    value.len() <= 128
        && value.contains('/')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'+' | b'-'))
}

fn validate_relative_path(value: &str) -> Result<(), ProvisionalWorldCompositionError> {
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
        return Err(ProvisionalWorldCompositionError::InvalidArtifactPath);
    }
    Ok(())
}

fn is_noncommercial_license(value: &str) -> bool {
    let normalized = value.to_ascii_uppercase().replace([' ', '_'], "-");
    normalized.contains("NONCOMMERCIAL")
        || normalized.contains("NON-COMMERCIAL")
        || normalized.contains("-NC")
        || normalized.contains("CC-BY-NC")
        || normalized.contains("CC-BY-NC-")
}

fn validate_canonical_text_set(
    values: &[String],
    field: &'static str,
) -> Result<(), ProvisionalWorldCompositionError> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        return Err(ProvisionalWorldCompositionError::EmptyTextSet(field));
    }
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(ProvisionalWorldCompositionError::NonCanonicalTextSet(field));
    }
    Ok(())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProvisionalWorldCompositionError {
    #[error("unsupported provisional-world composition schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid provisional-world identifier or semantic version")]
    InvalidIdentifier,
    #[error("invalid full-Earth grid: {0}")]
    InvalidGrid(String),
    #[error("the provisional composition must contain all seven Earth layer roles")]
    IncompleteEarthLayers,
    #[error("provisional Earth layers are not in canonical role order")]
    NonCanonicalEarthLayerOrder,
    #[error("the provisional composition must contain sky, fauna catalog, and fauna traits")]
    IncompleteWorldComponents,
    #[error("provisional non-layer components are not in canonical order")]
    NonCanonicalComponentOrder,
    #[error("provisional artifact identifiers must be unique")]
    DuplicateArtifactIdentifier,
    #[error("provisional artifact path must be a normalized relative path")]
    InvalidArtifactPath,
    #[error("invalid provisional artifact media type")]
    InvalidMediaType,
    #[error("provisional artifact digest must not be zero")]
    ZeroDigest,
    #[error("provisional artifact byte length must be positive")]
    ZeroByteLength,
    #[error("provisional artifact license must permit project use")]
    UnusableLicense,
    #[error("provisional artifact scientific scope is required")]
    MissingScientificScope,
    #[error("{0} must contain at least one nonempty statement")]
    EmptyTextSet(&'static str),
    #[error("{0} must be strictly sorted without duplicates")]
    NonCanonicalTextSet(&'static str),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("encoding error: {0}")]
    Encoding(String),
    #[error("noncanonical encoding")]
    NonCanonicalEncoding,
    #[error("invalid provisional execution reference: {0}")]
    InvalidExecutionReference(String),
}

#[cfg(test)]
mod tests {
    use world_domain::{EarthResolutionLevels, S2Projection};

    use super::*;

    fn grid() -> FullEarthGrid {
        FullEarthGrid {
            physics_crs_epsg: 4_978,
            catalog_crs_epsg: 4_979,
            vertical_crs_epsg: 3_855,
            s2_definition_url: "https://s2geometry.io/devguide/s2cell_hierarchy".to_owned(),
            s2_library_revision: "0123456789abcdef".to_owned(),
            s2_definition_hash: Digest::sha256(b"provisional composition S2 fixture"),
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

    fn artifact(id: &str, salt: u8) -> ProvisionalArtifactReference {
        ProvisionalArtifactReference {
            artifact_id: id.to_owned(),
            artifact_path: format!("releases/{id}/root.index"),
            media_type: "application/vnd.atinycivilization.provisional+json".to_owned(),
            content_hash: Digest::from_bytes([salt; 32]),
            byte_length: 100,
            license_expression: "CC-BY-4.0".to_owned(),
            scientific_scope: "Breadth-first fixture only.".to_owned(),
            limitations: vec!["Not scientifically admitted.".to_owned()],
        }
    }

    fn composition() -> ProvisionalWorldComposition {
        ProvisionalWorldComposition {
            composition_schema_version: PROVISIONAL_WORLD_COMPOSITION_SCHEMA_VERSION,
            composition_id: "full-earth-breadth-first".to_owned(),
            composition_version: "0.1.0".to_owned(),
            status: ProvisionalWorldCompositionStatus::ProvisionalNotScientificallyAdmitted,
            full_earth_grid: grid(),
            earth_layers: REQUIRED_EARTH_LAYERS
                .into_iter()
                .enumerate()
                .map(|(index, kind)| ProvisionalEarthLayerReference {
                    kind,
                    release: artifact(
                        &format!("earth-layer-{index}"),
                        u8::try_from(index + 1).expect("fixture salt fits"),
                    ),
                })
                .collect(),
            world_components: REQUIRED_WORLD_COMPONENTS
                .into_iter()
                .enumerate()
                .map(|(index, kind)| ProvisionalWorldComponentReference {
                    kind,
                    release: artifact(
                        &format!("world-component-{index}"),
                        u8::try_from(index + 20).expect("fixture salt fits"),
                    ),
                })
                .collect(),
            coupled_validation_gaps: vec![
                "Cross-layer coupling has not been independently validated.".to_owned(),
                "Uncertainty propagation remains incomplete.".to_owned(),
            ],
        }
    }

    #[test]
    fn complete_provisional_composition_round_trips_but_is_not_a_world_bundle() {
        let composition = composition();
        let reference = composition
            .execution_reference()
            .expect("valid execution reference");
        assert_eq!(reference.composition_id, composition.composition_id);
        assert_eq!(
            reference.content_hash,
            composition.content_digest().expect("composition digest")
        );
        let bytes = composition
            .canonical_bytes()
            .expect("canonical composition");
        assert_eq!(
            ProvisionalWorldComposition::from_canonical_slice(&bytes),
            Ok(composition)
        );
        assert!(crate::WorldDataBundle::from_canonical_slice(&bytes).is_err());
    }

    #[test]
    fn committed_full_earth_composition_includes_normalized_fauna_evidence() {
        let bytes = include_bytes!("../../../data/provisional/full-earth-breadth-first-0.1.0.json");
        let composition = ProvisionalWorldComposition::from_canonical_slice(bytes)
            .expect("committed provisional composition stays canonical");
        assert_eq!(
            composition
                .world_components
                .last()
                .map(|component| component.kind),
            Some(ProvisionalWorldComponentKind::FaunaPhysiologyEvidence)
        );
    }

    #[test]
    fn missing_reordered_or_noncommercial_inputs_fail_closed() {
        let mut missing = composition();
        missing.earth_layers.pop();
        assert_eq!(
            missing.validate(),
            Err(ProvisionalWorldCompositionError::IncompleteEarthLayers)
        );

        let mut reordered = composition();
        reordered.earth_layers.swap(0, 1);
        assert_eq!(
            reordered.validate(),
            Err(ProvisionalWorldCompositionError::NonCanonicalEarthLayerOrder)
        );

        let mut noncommercial = composition();
        noncommercial.world_components[0].release.license_expression = "CC-BY-NC-4.0".to_owned();
        assert_eq!(
            noncommercial.validate(),
            Err(ProvisionalWorldCompositionError::UnusableLicense)
        );
    }

    #[test]
    fn gaps_and_artifact_references_are_canonical_and_tamper_evident() {
        let mut duplicate_gap = composition();
        duplicate_gap
            .coupled_validation_gaps
            .push("Uncertainty propagation remains incomplete.".to_owned());
        assert_eq!(
            duplicate_gap.validate(),
            Err(ProvisionalWorldCompositionError::NonCanonicalTextSet(
                "coupled_validation_gaps"
            ))
        );

        let mut escaped_path = composition();
        escaped_path.earth_layers[0].release.artifact_path = "../outside".to_owned();
        assert_eq!(
            escaped_path.validate(),
            Err(ProvisionalWorldCompositionError::InvalidArtifactPath)
        );

        let bytes = composition()
            .canonical_bytes()
            .expect("canonical composition");
        let pretty = [bytes.as_slice(), b"\n"].concat();
        assert_eq!(
            ProvisionalWorldComposition::from_canonical_slice(&pretty),
            Err(ProvisionalWorldCompositionError::NonCanonicalEncoding)
        );
    }
}
