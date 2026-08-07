//! Provisional source manifests and trait evidence for real-world fauna.
//!
//! This schema keeps retained observations and source-compiled aggregates separate
//! from assumptions. It does not make a fauna catalog scientifically admissible and
//! cannot represent an imputed value as observed evidence.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, SpeciesIdentity};

pub const FAUNA_EVIDENCE_MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_EVIDENCE_MANIFEST_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-evidence-manifest+json";
const MAX_DECIMAL_PLACES: u8 = 9;

/// The deliberately small initial trait-source composition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FaunaEvidenceSource {
    #[serde(rename = "amniote-life-history-2015-08")]
    AmnioteLifeHistoryAugust2015,
    #[serde(rename = "animal-traits-1.0.7")]
    AnimalTraitsV1_0_7,
    #[serde(rename = "elton-traits-1.0")]
    EltonTraitsV1_0,
}

impl FaunaEvidenceSource {
    const ALL: [Self; 3] = [
        Self::AmnioteLifeHistoryAugust2015,
        Self::AnimalTraitsV1_0_7,
        Self::EltonTraitsV1_0,
    ];

    #[must_use]
    pub const fn source_id(self) -> &'static str {
        match self {
            Self::AmnioteLifeHistoryAugust2015 => "amniote-life-history-2015-08",
            Self::AnimalTraitsV1_0_7 => "animal-traits-1.0.7",
            Self::EltonTraitsV1_0 => "elton-traits-1.0",
        }
    }

    const fn title(self) -> &'static str {
        match self {
            Self::AmnioteLifeHistoryAugust2015 => "Amniote Life-History Database",
            Self::AnimalTraitsV1_0_7 => "AnimalTraits",
            Self::EltonTraitsV1_0 => "EltonTraits 1.0",
        }
    }

    const fn version(self) -> &'static str {
        match self {
            Self::AmnioteLifeHistoryAugust2015 => "2015-08",
            Self::AnimalTraitsV1_0_7 => "1.0.7",
            Self::EltonTraitsV1_0 => "1.0",
        }
    }

    const fn canonical_url(self) -> &'static str {
        match self {
            Self::AmnioteLifeHistoryAugust2015 => "https://www.esapubs.org/archive/ecol/E096/269/",
            Self::AnimalTraitsV1_0_7 => "https://doi.org/10.5281/zenodo.6468938",
            Self::EltonTraitsV1_0 => "https://doi.org/10.6084/m9.figshare.c.3306933.v1",
        }
    }

    const fn license(self) -> FaunaEvidenceLicense {
        match self {
            Self::AmnioteLifeHistoryAugust2015 => {
                FaunaEvidenceLicense::EsaE096269NoCopyrightRestrictions
            }
            Self::AnimalTraitsV1_0_7 => FaunaEvidenceLicense::Cc0_1_0,
            Self::EltonTraitsV1_0 => FaunaEvidenceLicense::CcBy4_0,
        }
    }

    const fn license_evidence_url(self) -> &'static str {
        match self {
            Self::AmnioteLifeHistoryAugust2015 => {
                "https://www.esapubs.org/archive/ecol/E096/269/metadata.php"
            }
            Self::AnimalTraitsV1_0_7 => "https://doi.org/10.1038/s41597-022-01364-9",
            Self::EltonTraitsV1_0 => "https://doi.org/10.6084/m9.figshare.c.3306933.v1",
        }
    }

    const fn scope(self) -> &'static str {
        match self {
            Self::AmnioteLifeHistoryAugust2015 => {
                "compiled life-history parameters for birds, mammals, and reptiles; source medians are not raw observations"
            }
            Self::AnimalTraitsV1_0_7 => {
                "body-mass, metabolic-rate, and brain-size observations with incomplete terrestrial-animal coverage"
            }
            Self::EltonTraitsV1_0 => {
                "species-level diet, foraging-stratum, nocturnality, and body-mass aggregates for birds and mammals"
            }
        }
    }
}

/// License expressions verified for the three initial sources.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FaunaEvidenceLicense {
    #[serde(rename = "CC0-1.0")]
    Cc0_1_0,
    #[serde(rename = "CC-BY-4.0")]
    CcBy4_0,
    #[serde(rename = "LicenseRef-ESA-E096-269-No-Copyright-Restrictions")]
    EsaE096269NoCopyrightRestrictions,
}

/// Immutable acquisition facts for one initial fauna trait source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaEvidenceSourceManifest {
    pub source: FaunaEvidenceSource,
    pub source_id: String,
    pub title: String,
    pub version: String,
    pub canonical_url: String,
    pub license: FaunaEvidenceLicense,
    pub license_evidence_url: String,
    pub scope: String,
    pub retrieved_on: NaiveDate,
    pub source_snapshot_digest: Digest,
    /// Digest of the canonical ordered aggregate of retained source artifacts.
    pub source_artifact_digest: Digest,
    pub source_artifact_count: u32,
    pub source_artifact_byte_length: u64,
}

impl FaunaEvidenceSourceManifest {
    #[must_use]
    pub fn new(
        source: FaunaEvidenceSource,
        retrieved_on: NaiveDate,
        source_snapshot_digest: Digest,
        source_artifact_digest: Digest,
        source_artifact_count: u32,
        source_artifact_byte_length: u64,
    ) -> Self {
        Self {
            source,
            source_id: source.source_id().to_owned(),
            title: source.title().to_owned(),
            version: source.version().to_owned(),
            canonical_url: source.canonical_url().to_owned(),
            license: source.license(),
            license_evidence_url: source.license_evidence_url().to_owned(),
            scope: source.scope().to_owned(),
            retrieved_on,
            source_snapshot_digest,
            source_artifact_digest,
            source_artifact_count,
            source_artifact_byte_length,
        }
    }

    pub fn validate(&self) -> Result<(), FaunaEvidenceManifestError> {
        if self.source_id != self.source.source_id()
            || self.title != self.source.title()
            || self.version != self.source.version()
            || self.canonical_url != self.source.canonical_url()
            || self.license != self.source.license()
            || self.license_evidence_url != self.source.license_evidence_url()
            || self.scope != self.source.scope()
        {
            return Err(FaunaEvidenceManifestError::SourceMetadataMismatch(
                self.source,
            ));
        }
        if self.source_snapshot_digest == Digest::ZERO
            || self.source_artifact_digest == Digest::ZERO
        {
            return Err(FaunaEvidenceManifestError::ZeroDigest);
        }
        if self.source_artifact_count == 0 || self.source_artifact_byte_length == 0 {
            return Err(FaunaEvidenceManifestError::EmptySourceArtifactSet);
        }
        Ok(())
    }
}

/// Only retained empirical records and source-declared aggregates are evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FaunaEvidenceBasis {
    EmpiricalObservation,
    SourceCompiledSpeciesAggregate,
}

/// Exact fixed-point representation shared by evidence and assumptions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScaledFaunaTraitValue {
    pub value: i64,
    pub decimal_places: u8,
    pub unit: String,
}

/// One retained, source-addressable trait value for a cited real species.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedFaunaTrait {
    pub species: SpeciesIdentity,
    pub trait_id: String,
    pub value: ScaledFaunaTraitValue,
    pub source: FaunaEvidenceSource,
    pub source_field: String,
    pub source_record_id: String,
    pub source_record_digest: Digest,
    pub evidence_basis: FaunaEvidenceBasis,
}

/// Explicit reason a provisional value is not scientific evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FaunaAssumptionBasis {
    ConservativeTaxonomicEstimate,
    DeterministicAllometry,
    Placeholder,
}

/// A provisional value that can never be decoded as observed evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaTraitAssumption {
    pub species: SpeciesIdentity,
    pub trait_id: String,
    pub value: ScaledFaunaTraitValue,
    pub assumption_id: String,
    pub method_id: String,
    pub method_digest: Digest,
    pub basis: FaunaAssumptionBasis,
    pub rationale: String,
}

/// A scaffold status is intentionally not a scientific-admission state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FaunaEvidenceStatus {
    #[serde(rename = "provisional-unvalidated")]
    ProvisionalUnvalidated,
}

/// Complete provisional manifests plus strictly separated evidence and assumptions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaEvidenceManifest {
    pub manifest_schema_version: u16,
    pub manifest_id: String,
    pub status: FaunaEvidenceStatus,
    pub sources: Vec<FaunaEvidenceSourceManifest>,
    pub observed_traits: Vec<ObservedFaunaTrait>,
    pub assumptions: Vec<FaunaTraitAssumption>,
}

impl FaunaEvidenceManifest {
    pub fn validate(&self) -> Result<(), FaunaEvidenceManifestError> {
        if self.manifest_schema_version != FAUNA_EVIDENCE_MANIFEST_SCHEMA_VERSION {
            return Err(FaunaEvidenceManifestError::UnsupportedSchema(
                self.manifest_schema_version,
            ));
        }
        if !slug(&self.manifest_id) {
            return Err(FaunaEvidenceManifestError::InvalidIdentifier);
        }
        if self.sources.len() != FaunaEvidenceSource::ALL.len() {
            return Err(FaunaEvidenceManifestError::IncompleteSourceComposition);
        }
        for (manifest, expected) in self.sources.iter().zip(FaunaEvidenceSource::ALL) {
            if manifest.source != expected {
                return Err(FaunaEvidenceManifestError::NonCanonicalSourceOrder);
            }
            manifest.validate()?;
        }

        if self
            .observed_traits
            .windows(2)
            .any(|pair| observed_order_key(&pair[0]) >= observed_order_key(&pair[1]))
        {
            return Err(FaunaEvidenceManifestError::NonCanonicalEvidenceOrder);
        }
        let mut evidenced_traits = BTreeSet::new();
        for evidence in &self.observed_traits {
            validate_species_trait(&evidence.species, &evidence.trait_id, &evidence.value)?;
            if !technical_identifier(&evidence.source_field)
                || evidence.source_record_id.trim().is_empty()
                || evidence.source_record_id.len() > 256
            {
                return Err(FaunaEvidenceManifestError::InvalidSourceRecord);
            }
            if evidence.source_record_digest == Digest::ZERO {
                return Err(FaunaEvidenceManifestError::ZeroDigest);
            }
            let correct_basis = matches!(
                (evidence.source, evidence.evidence_basis),
                (
                    FaunaEvidenceSource::AnimalTraitsV1_0_7,
                    FaunaEvidenceBasis::EmpiricalObservation
                ) | (
                    FaunaEvidenceSource::AmnioteLifeHistoryAugust2015
                        | FaunaEvidenceSource::EltonTraitsV1_0,
                    FaunaEvidenceBasis::SourceCompiledSpeciesAggregate
                )
            );
            if !correct_basis {
                return Err(FaunaEvidenceManifestError::SourceEvidenceBasisMismatch);
            }
            evidenced_traits.insert(species_trait_key(&evidence.species, &evidence.trait_id));
        }

        if self
            .assumptions
            .windows(2)
            .any(|pair| assumption_order_key(&pair[0]) >= assumption_order_key(&pair[1]))
        {
            return Err(FaunaEvidenceManifestError::NonCanonicalAssumptionOrder);
        }
        for assumption in &self.assumptions {
            validate_species_trait(&assumption.species, &assumption.trait_id, &assumption.value)?;
            if !slug(&assumption.assumption_id)
                || !slug(&assumption.method_id)
                || assumption.method_digest == Digest::ZERO
                || assumption.rationale.trim().is_empty()
            {
                return Err(FaunaEvidenceManifestError::InvalidAssumption);
            }
            if evidenced_traits.contains(&species_trait_key(
                &assumption.species,
                &assumption.trait_id,
            )) {
                return Err(FaunaEvidenceManifestError::AssumptionShadowsEvidence);
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FaunaEvidenceManifestError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaEvidenceManifestError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, FaunaEvidenceManifestError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaEvidenceManifestError::Decode(error.to_string()))?;
        manifest.validate()?;
        if manifest.canonical_bytes()? != bytes {
            return Err(FaunaEvidenceManifestError::NonCanonicalEncoding);
        }
        Ok(manifest)
    }
}

fn validate_species_trait(
    species: &SpeciesIdentity,
    trait_id: &str,
    value: &ScaledFaunaTraitValue,
) -> Result<(), FaunaEvidenceManifestError> {
    species
        .validate()
        .map_err(|error| FaunaEvidenceManifestError::InvalidSpecies(error.to_string()))?;
    if !slug(trait_id) {
        return Err(FaunaEvidenceManifestError::InvalidIdentifier);
    }
    if value.decimal_places > MAX_DECIMAL_PLACES || !unit(&value.unit) {
        return Err(FaunaEvidenceManifestError::InvalidScaledValue);
    }
    Ok(())
}

fn species_trait_key<'a>(
    species: &'a SpeciesIdentity,
    trait_id: &'a str,
) -> (&'a str, &'a str, &'a str) {
    (
        species.catalog.as_str(),
        species.identifier.as_str(),
        trait_id,
    )
}

fn observed_order_key(
    evidence: &ObservedFaunaTrait,
) -> (&str, &str, &str, FaunaEvidenceSource, &str) {
    (
        evidence.species.catalog.as_str(),
        evidence.species.identifier.as_str(),
        evidence.trait_id.as_str(),
        evidence.source,
        evidence.source_record_id.as_str(),
    )
}

fn assumption_order_key(assumption: &FaunaTraitAssumption) -> (&str, &str, &str) {
    species_trait_key(&assumption.species, &assumption.trait_id)
}

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn technical_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn unit(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'^' | b'-' | b'_' | b'.')
        })
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FaunaEvidenceManifestError {
    #[error("unsupported fauna-evidence manifest schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid fauna-evidence identifier")]
    InvalidIdentifier,
    #[error("the provisional fauna source composition must contain all three pinned sources")]
    IncompleteSourceComposition,
    #[error("fauna evidence sources are not in canonical order")]
    NonCanonicalSourceOrder,
    #[error("pinned metadata does not match source {0:?}")]
    SourceMetadataMismatch(FaunaEvidenceSource),
    #[error("fauna evidence digest must not be zero")]
    ZeroDigest,
    #[error("retained source artifact sets must have positive count and byte length")]
    EmptySourceArtifactSet,
    #[error("fauna evidence records are not in canonical order")]
    NonCanonicalEvidenceOrder,
    #[error("fauna assumptions are not in canonical order or contain duplicates")]
    NonCanonicalAssumptionOrder,
    #[error("invalid real species identity: {0}")]
    InvalidSpecies(String),
    #[error("invalid fixed-point fauna trait value")]
    InvalidScaledValue,
    #[error("invalid source field, record identifier, or record provenance")]
    InvalidSourceRecord,
    #[error("fauna evidence basis does not match the pinned source's record semantics")]
    SourceEvidenceBasisMismatch,
    #[error("invalid explicit fauna assumption")]
    InvalidAssumption,
    #[error("an assumption cannot replace a species trait backed by retained evidence")]
    AssumptionShadowsEvidence,
    #[error("decode error: {0}")]
    Decode(String),
    #[error("encoding error: {0}")]
    Encoding(String),
    #[error("noncanonical encoding")]
    NonCanonicalEncoding,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_manifest(source: FaunaEvidenceSource) -> FaunaEvidenceSourceManifest {
        FaunaEvidenceSourceManifest::new(
            source,
            NaiveDate::from_ymd_opt(2026, 8, 7).expect("valid date"),
            Digest::sha256(format!("{}-snapshot", source.source_id()).as_bytes()),
            Digest::sha256(format!("{}-artifacts", source.source_id()).as_bytes()),
            1,
            100,
        )
    }

    fn species() -> SpeciesIdentity {
        SpeciesIdentity::new(
            "gbif",
            "2441176",
            "Bison bison",
            "https://www.gbif.org/species/2441176",
        )
        .expect("valid retained GBIF species")
    }

    fn value(value: i64, unit: &str) -> ScaledFaunaTraitValue {
        ScaledFaunaTraitValue {
            value,
            decimal_places: 2,
            unit: unit.to_owned(),
        }
    }

    fn manifest() -> FaunaEvidenceManifest {
        FaunaEvidenceManifest {
            manifest_schema_version: FAUNA_EVIDENCE_MANIFEST_SCHEMA_VERSION,
            manifest_id: "initial-fauna-trait-evidence".to_owned(),
            status: FaunaEvidenceStatus::ProvisionalUnvalidated,
            sources: FaunaEvidenceSource::ALL
                .into_iter()
                .map(source_manifest)
                .collect(),
            observed_traits: vec![ObservedFaunaTrait {
                species: species(),
                trait_id: "adult-body-mass".to_owned(),
                value: value(80_000_000, "g"),
                source: FaunaEvidenceSource::AnimalTraitsV1_0_7,
                source_field: "body_mass_g".to_owned(),
                source_record_id: "fixture-animaltraits-row".to_owned(),
                source_record_digest: Digest::sha256(b"retained source row"),
                evidence_basis: FaunaEvidenceBasis::EmpiricalObservation,
            }],
            assumptions: vec![FaunaTraitAssumption {
                species: species(),
                trait_id: "resting-water-loss-rate".to_owned(),
                value: value(0, "g/day"),
                assumption_id: "bison-water-loss-placeholder".to_owned(),
                method_id: "explicit-missing-evidence-v1".to_owned(),
                method_digest: Digest::sha256(b"no scientific value admitted"),
                basis: FaunaAssumptionBasis::Placeholder,
                rationale: "No retained value has been scientifically validated.".to_owned(),
            }],
        }
    }

    #[test]
    fn provisional_manifest_round_trips_canonically() {
        let manifest = manifest();
        let bytes = manifest.canonical_bytes().expect("canonical manifest");
        assert_eq!(
            FaunaEvidenceManifest::from_canonical_slice(&bytes),
            Ok(manifest)
        );

        let mut pretty = bytes;
        pretty.push(b'\n');
        assert_eq!(
            FaunaEvidenceManifest::from_canonical_slice(&pretty),
            Err(FaunaEvidenceManifestError::NonCanonicalEncoding)
        );
    }

    #[test]
    fn source_identity_license_and_scope_are_pinned() {
        let mut wrong_license = manifest();
        wrong_license.sources[0].license = FaunaEvidenceLicense::Cc0_1_0;
        assert_eq!(
            wrong_license.validate(),
            Err(FaunaEvidenceManifestError::SourceMetadataMismatch(
                FaunaEvidenceSource::AmnioteLifeHistoryAugust2015
            ))
        );

        let mut missing_source = manifest();
        missing_source.sources.pop();
        assert_eq!(
            missing_source.validate(),
            Err(FaunaEvidenceManifestError::IncompleteSourceComposition)
        );

        let mut empty_artifacts = manifest();
        empty_artifacts.sources[0].source_artifact_count = 0;
        assert_eq!(
            empty_artifacts.validate(),
            Err(FaunaEvidenceManifestError::EmptySourceArtifactSet)
        );
    }

    #[test]
    fn assumptions_cannot_shadow_retained_evidence() {
        let mut shadowed = manifest();
        shadowed.assumptions[0].trait_id = "adult-body-mass".to_owned();
        assert_eq!(
            shadowed.validate(),
            Err(FaunaEvidenceManifestError::AssumptionShadowsEvidence)
        );
    }

    #[test]
    fn evidence_requires_exact_record_provenance() {
        let mut zero_record = manifest();
        zero_record.observed_traits[0].source_record_digest = Digest::ZERO;
        assert_eq!(
            zero_record.validate(),
            Err(FaunaEvidenceManifestError::ZeroDigest)
        );

        let mut bad_field = manifest();
        bad_field.observed_traits[0].source_field = "body mass (maybe)".to_owned();
        assert_eq!(
            bad_field.validate(),
            Err(FaunaEvidenceManifestError::InvalidSourceRecord)
        );

        let mut false_aggregate = manifest();
        false_aggregate.observed_traits[0].evidence_basis =
            FaunaEvidenceBasis::SourceCompiledSpeciesAggregate;
        assert_eq!(
            false_aggregate.validate(),
            Err(FaunaEvidenceManifestError::SourceEvidenceBasisMismatch)
        );
    }

    #[test]
    fn modeled_values_cannot_decode_as_observed_evidence() {
        let bytes = manifest().canonical_bytes().expect("canonical manifest");
        let json = String::from_utf8(bytes).expect("JSON is UTF-8");
        let forged = json.replace(
            "\"evidence_basis\":\"empirical_observation\"",
            "\"evidence_basis\":\"phylogenetic_estimate\"",
        );
        assert!(matches!(
            FaunaEvidenceManifest::from_canonical_slice(forged.as_bytes()),
            Err(FaunaEvidenceManifestError::Decode(_))
        ));
    }
}
