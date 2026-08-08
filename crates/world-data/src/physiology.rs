//! Canonical, source-addressable organism physiology inputs.
//!
//! This is intentionally an evidence hand-off, not a physiology model. A later
//! engine ruleset may consume only a profile whose values and source rows are pinned.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{
    Digest, METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION, MetabolicRateCommitment, SpeciesIdentity,
};

use crate::{FaunaEvidenceBasis, FaunaEvidenceSource, ScaledFaunaTraitValue};

pub const FAUNA_PHYSIOLOGY_PROFILE_SET_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_PHYSIOLOGY_PROFILE_SET_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-physiology-profile-set+json";
pub const FAUNA_PHYSIOLOGY_PROFILE_CATALOG_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_PHYSIOLOGY_PROFILE_CATALOG_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-physiology-profile-catalog+json";
pub const FAUNA_METABOLIC_RATE_SELECTION_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_METABOLIC_RATE_SELECTION_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-metabolic-rate-selection+json";
pub const LEGACY_FAUNA_METABOLIC_RATE_PLAN_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_METABOLIC_RATE_PLAN_SCHEMA_VERSION: u16 = 2;
pub const FAUNA_METABOLIC_RATE_PLAN_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-metabolic-rate-plan+json";
pub const FAUNA_BODY_MASS_SELECTION_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_BODY_MASS_SELECTION_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-body-mass-selection+json";
pub const FAUNA_BODY_MASS_PLAN_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_BODY_MASS_PLAN_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-body-mass-plan+json";
pub const FAUNA_ECOLOGY_PLAN_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_ECOLOGY_PLAN_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-ecology-plan+json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaPhysiologyProfile {
    pub species: SpeciesIdentity,
    pub trait_id: String,
    pub value: ScaledFaunaTraitValue,
    pub source: FaunaEvidenceSource,
    pub source_field: String,
    pub source_record_id: String,
    pub source_record_digest: Digest,
    pub evidence_basis: FaunaEvidenceBasis,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaPhysiologyProfileSet {
    pub profile_set_schema_version: u16,
    pub source_artifact_digest: Digest,
    pub profiles: Vec<FaunaPhysiologyProfile>,
}

/// A small, immutable index over independently compiled profile sets.
///
/// The profile bytes remain independent artifacts because their values came from
/// distinct publications. A world can pin this catalog before selectively loading
/// profiles for its actual fauna plan; it never needs to collapse source provenance
/// into a fictional combined dataset.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaPhysiologyProfileCatalog {
    pub profile_catalog_schema_version: u16,
    pub profile_sets: Vec<FaunaPhysiologyProfileSetReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaPhysiologyProfileSetReference {
    pub profile_set_id: String,
    pub profile_set_digest: Digest,
    pub source_artifact_digest: Digest,
    pub profile_count: u64,
}

/// An immutable choice of one measured rate for one real taxon.
///
/// A profile set can legitimately retain several observations for a species.  A
/// caller must therefore commit this selector rather than silently averaging or
/// choosing an arbitrary record.  Resolving a selector does not imply that the
/// rate is appropriate for a particular environment or organism state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaMetabolicRateSelection {
    pub selection_schema_version: u16,
    pub profile_set_digest: Digest,
    pub species: SpeciesIdentity,
    pub source_record_id: String,
}

/// A canonical set of deliberately selected observed rates for covered participating taxa.
///
/// It is separate from a population plan: it says which measurement is carried by a
/// species, never how many individuals of it exist or what that measurement means
/// for survival. A missing selection is explicit absence of retained evidence, not
/// permission to label an estimate as a source measurement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaMetabolicRatePlan {
    pub plan_schema_version: u16,
    pub selections: Vec<FaunaMetabolicRateSelection>,
}

/// An explicit, immutable choice of one retained adult-mass observation.
///
/// Sources may contain several valid measurements for one taxon. Keeping this
/// choice separate from the profile set prevents a consumer from depending on
/// source row order or silently averaging unlike observations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaBodyMassSelection {
    pub selection_schema_version: u16,
    pub profile_set_digest: Digest,
    pub species: SpeciesIdentity,
    pub source_record_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaBodyMassPlan {
    pub plan_schema_version: u16,
    pub selections: Vec<FaunaBodyMassSelection>,
}

/// Exact source rows retained for one real taxon, without converting descriptive
/// ecology into an agent drive, action, affordance, or habitat decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaEcologyPlanEntry {
    pub species: SpeciesIdentity,
    pub source_record_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaEcologyPlan {
    pub plan_schema_version: u16,
    pub profile_set_digest: Digest,
    pub entries: Vec<FaunaEcologyPlanEntry>,
}

impl FaunaEcologyPlan {
    pub fn validate(&self) -> Result<(), FaunaPhysiologyProfileError> {
        if self.plan_schema_version != FAUNA_ECOLOGY_PLAN_SCHEMA_VERSION
            || self.profile_set_digest == Digest::ZERO
        {
            return Err(FaunaPhysiologyProfileError::InvalidEcologyPlan);
        }
        for pair in self.entries.windows(2) {
            if ecology_entry_key(&pair[0]) >= ecology_entry_key(&pair[1]) {
                return Err(FaunaPhysiologyProfileError::NonCanonicalEcologyPlanOrder);
            }
        }
        for entry in &self.entries {
            entry
                .species
                .validate()
                .map_err(|_| FaunaPhysiologyProfileError::InvalidEcologyPlan)?;
            if entry.source_record_ids.is_empty()
                || entry
                    .source_record_ids
                    .iter()
                    .any(|value| !technical(value))
                || entry
                    .source_record_ids
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
            {
                return Err(FaunaPhysiologyProfileError::InvalidEcologyPlan);
            }
        }
        Ok(())
    }

    pub fn resolve<'a>(
        &self,
        profiles: &'a FaunaPhysiologyProfileSet,
    ) -> Result<Vec<&'a FaunaPhysiologyProfile>, FaunaPhysiologyProfileError> {
        self.validate()?;
        if Digest::sha256(&profiles.canonical_bytes()?) != self.profile_set_digest {
            return Err(FaunaPhysiologyProfileError::EcologyProfileSetMismatch);
        }
        let mut resolved = Vec::new();
        for entry in &self.entries {
            for record_id in &entry.source_record_ids {
                let profile = profiles
                    .profiles
                    .iter()
                    .find(|profile| {
                        same_catalog_taxon(&profile.species, &entry.species)
                            && profile.source_record_id == *record_id
                            && (profile.trait_id.starts_with("diet-")
                                || profile.trait_id.starts_with("activity-"))
                    })
                    .ok_or(FaunaPhysiologyProfileError::SelectedEcologyProfileMissing)?;
                resolved.push(profile);
            }
        }
        Ok(resolved)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FaunaPhysiologyProfileError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaPhysiologyProfileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, FaunaPhysiologyProfileError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaPhysiologyProfileError::Decode(error.to_string()))?;
        if value.canonical_bytes()? != bytes {
            return Err(FaunaPhysiologyProfileError::NonCanonicalEncoding);
        }
        Ok(value)
    }
}

impl FaunaBodyMassPlan {
    pub fn validate(&self) -> Result<(), FaunaPhysiologyProfileError> {
        if self.plan_schema_version != FAUNA_BODY_MASS_PLAN_SCHEMA_VERSION {
            return Err(FaunaPhysiologyProfileError::UnsupportedBodyMassPlanSchema);
        }
        for pair in self.selections.windows(2) {
            if body_mass_selection_key(&pair[0]) >= body_mass_selection_key(&pair[1]) {
                return Err(FaunaPhysiologyProfileError::NonCanonicalBodyMassPlanOrder);
            }
        }
        for selection in &self.selections {
            selection.validate()?;
        }
        Ok(())
    }

    pub fn selection_for(&self, species: &SpeciesIdentity) -> Option<&FaunaBodyMassSelection> {
        self.selections
            .binary_search_by(|selection| {
                body_mass_selection_key(selection)
                    .cmp(&(species.catalog.as_str(), species.identifier.as_str()))
            })
            .ok()
            .map(|index| &self.selections[index])
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FaunaPhysiologyProfileError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaPhysiologyProfileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, FaunaPhysiologyProfileError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaPhysiologyProfileError::Decode(error.to_string()))?;
        if value.canonical_bytes()? != bytes {
            return Err(FaunaPhysiologyProfileError::NonCanonicalEncoding);
        }
        Ok(value)
    }
}

impl FaunaBodyMassSelection {
    pub fn validate(&self) -> Result<(), FaunaPhysiologyProfileError> {
        if self.selection_schema_version != FAUNA_BODY_MASS_SELECTION_SCHEMA_VERSION {
            return Err(FaunaPhysiologyProfileError::UnsupportedBodyMassSelectionSchema);
        }
        self.species
            .validate()
            .map_err(|_| FaunaPhysiologyProfileError::InvalidBodyMassSelection)?;
        if self.profile_set_digest == Digest::ZERO || !technical(&self.source_record_id) {
            return Err(FaunaPhysiologyProfileError::InvalidBodyMassSelection);
        }
        Ok(())
    }

    pub fn resolve<'a>(
        &self,
        profiles: &'a FaunaPhysiologyProfileSet,
    ) -> Result<&'a FaunaPhysiologyProfile, FaunaPhysiologyProfileError> {
        self.validate()?;
        if Digest::sha256(&profiles.canonical_bytes()?) != self.profile_set_digest {
            return Err(FaunaPhysiologyProfileError::BodyMassProfileSetMismatch);
        }
        let profile = profiles
            .profiles
            .iter()
            .find(|profile| {
                same_catalog_taxon(&profile.species, &self.species)
                    && profile.trait_id == "adult-body-mass"
                    && profile.source_record_id == self.source_record_id
            })
            .ok_or(FaunaPhysiologyProfileError::SelectedBodyMassProfileMissing)?;
        if profile.value.unit != "g" || profile.value.value <= 0 {
            return Err(FaunaPhysiologyProfileError::InvalidSelectedBodyMassProfile);
        }
        Ok(profile)
    }
}

impl FaunaMetabolicRatePlan {
    pub fn validate(&self) -> Result<(), FaunaPhysiologyProfileError> {
        match self.plan_schema_version {
            LEGACY_FAUNA_METABOLIC_RATE_PLAN_SCHEMA_VERSION => {
                if self.selections.is_empty() {
                    return Err(FaunaPhysiologyProfileError::EmptyMetabolicPlan);
                }
            }
            FAUNA_METABOLIC_RATE_PLAN_SCHEMA_VERSION => {}
            _ => return Err(FaunaPhysiologyProfileError::UnsupportedMetabolicPlanSchema),
        }
        for pair in self.selections.windows(2) {
            if metabolic_selection_key(&pair[0]) >= metabolic_selection_key(&pair[1]) {
                return Err(FaunaPhysiologyProfileError::NonCanonicalMetabolicPlanOrder);
            }
        }
        for selection in &self.selections {
            selection.validate()?;
        }
        Ok(())
    }

    pub fn selection_for(&self, species: &SpeciesIdentity) -> Option<&FaunaMetabolicRateSelection> {
        self.selections
            .binary_search_by(|selection| {
                metabolic_selection_key(selection)
                    .cmp(&(species.catalog.as_str(), species.identifier.as_str()))
            })
            .ok()
            .map(|index| &self.selections[index])
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FaunaPhysiologyProfileError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaPhysiologyProfileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, FaunaPhysiologyProfileError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaPhysiologyProfileError::Decode(error.to_string()))?;
        if value.canonical_bytes()? != bytes {
            return Err(FaunaPhysiologyProfileError::NonCanonicalEncoding);
        }
        Ok(value)
    }
}

impl FaunaMetabolicRateSelection {
    pub fn validate(&self) -> Result<(), FaunaPhysiologyProfileError> {
        if self.selection_schema_version != FAUNA_METABOLIC_RATE_SELECTION_SCHEMA_VERSION {
            return Err(FaunaPhysiologyProfileError::UnsupportedMetabolicSelectionSchema);
        }
        self.species
            .validate()
            .map_err(|_| FaunaPhysiologyProfileError::InvalidMetabolicSelection)?;
        if self.profile_set_digest == Digest::ZERO || !technical(&self.source_record_id) {
            return Err(FaunaPhysiologyProfileError::InvalidMetabolicSelection);
        }
        Ok(())
    }

    pub fn resolve<'a>(
        &self,
        profiles: &'a FaunaPhysiologyProfileSet,
    ) -> Result<&'a FaunaPhysiologyProfile, FaunaPhysiologyProfileError> {
        self.validate()?;
        let bytes = profiles.canonical_bytes()?;
        if Digest::sha256(&bytes) != self.profile_set_digest {
            return Err(FaunaPhysiologyProfileError::MetabolicProfileSetMismatch);
        }
        let profile = profiles
            .profiles
            .iter()
            .find(|profile| {
                same_catalog_taxon(&profile.species, &self.species)
                    && profile.trait_id == "standardized-metabolic-rate"
                    && profile.source_record_id == self.source_record_id
            })
            .ok_or(FaunaPhysiologyProfileError::SelectedMetabolicProfileMissing)?;
        if profile.value.unit != "W" || profile.value.value <= 0 {
            return Err(FaunaPhysiologyProfileError::InvalidSelectedMetabolicProfile);
        }
        Ok(profile)
    }

    /// Resolve this selection into the exact canonical body-state commitment.
    ///
    /// The resulting value remains a retained standardized measurement in watts;
    /// it deliberately carries no conversion into a dietary or survival model.
    pub fn resolve_commitment(
        &self,
        profiles: &FaunaPhysiologyProfileSet,
    ) -> Result<MetabolicRateCommitment, FaunaPhysiologyProfileError> {
        let profile = self.resolve(profiles)?;
        Ok(MetabolicRateCommitment {
            commitment_schema_version: METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION,
            evidence_basis: world_domain::PhysiologicalEvidenceBasis::SourceMeasurement,
            profile_set_digest: self.profile_set_digest,
            // Source datasets legitimately retain different scientific-name renderings
            // (for example, with or without taxonomic authority) for the same catalog
            // identifier. Canonical body state uses the world's selected identity while
            // the exact source row remains pinned by its digest and record identifier.
            observed_species: self.species.clone(),
            source_record_id: profile.source_record_id.clone(),
            source_record_digest: profile.source_record_digest,
            measured_power_value: profile.value.value,
            measured_power_decimal_places: profile.value.decimal_places,
        })
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FaunaPhysiologyProfileError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaPhysiologyProfileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, FaunaPhysiologyProfileError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaPhysiologyProfileError::Decode(error.to_string()))?;
        if value.canonical_bytes()? != bytes {
            return Err(FaunaPhysiologyProfileError::NonCanonicalEncoding);
        }
        Ok(value)
    }
}

impl FaunaPhysiologyProfileCatalog {
    pub fn validate(&self) -> Result<(), FaunaPhysiologyProfileError> {
        if self.profile_catalog_schema_version != FAUNA_PHYSIOLOGY_PROFILE_CATALOG_SCHEMA_VERSION {
            return Err(FaunaPhysiologyProfileError::UnsupportedCatalogSchema);
        }
        if self.profile_sets.is_empty() {
            return Err(FaunaPhysiologyProfileError::EmptyCatalog);
        }
        for pair in self.profile_sets.windows(2) {
            if pair[0].profile_set_id >= pair[1].profile_set_id {
                return Err(FaunaPhysiologyProfileError::NonCanonicalCatalogOrder);
            }
        }
        for profile_set in &self.profile_sets {
            if !slug(&profile_set.profile_set_id)
                || profile_set.profile_set_digest == Digest::ZERO
                || profile_set.source_artifact_digest == Digest::ZERO
                || profile_set.profile_count == 0
            {
                return Err(FaunaPhysiologyProfileError::InvalidCatalogReference);
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FaunaPhysiologyProfileError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaPhysiologyProfileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, FaunaPhysiologyProfileError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaPhysiologyProfileError::Decode(error.to_string()))?;
        if value.canonical_bytes()? != bytes {
            return Err(FaunaPhysiologyProfileError::NonCanonicalEncoding);
        }
        Ok(value)
    }
}

impl FaunaPhysiologyProfileSet {
    pub fn validate(&self) -> Result<(), FaunaPhysiologyProfileError> {
        if self.profile_set_schema_version != FAUNA_PHYSIOLOGY_PROFILE_SET_SCHEMA_VERSION {
            return Err(FaunaPhysiologyProfileError::UnsupportedSchema);
        }
        if self.source_artifact_digest == Digest::ZERO || self.profiles.is_empty() {
            return Err(FaunaPhysiologyProfileError::EmptyOrUnpinned);
        }
        for pair in self.profiles.windows(2) {
            if profile_key(&pair[0]) >= profile_key(&pair[1]) {
                return Err(FaunaPhysiologyProfileError::NonCanonicalOrder);
            }
        }
        for profile in &self.profiles {
            profile
                .species
                .validate()
                .map_err(|_| FaunaPhysiologyProfileError::InvalidProfile)?;
            if !slug(&profile.trait_id)
                || !technical(&profile.source_field)
                || profile.source_record_id.is_empty()
                || profile.source_record_digest == Digest::ZERO
                || profile.value.unit.is_empty()
                || profile.value.decimal_places > 9
            {
                return Err(FaunaPhysiologyProfileError::InvalidProfile);
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FaunaPhysiologyProfileError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaPhysiologyProfileError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, FaunaPhysiologyProfileError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaPhysiologyProfileError::Decode(error.to_string()))?;
        if value.canonical_bytes()? != bytes {
            return Err(FaunaPhysiologyProfileError::NonCanonicalEncoding);
        }
        Ok(value)
    }
}

fn profile_key(profile: &FaunaPhysiologyProfile) -> (&str, &str, &str, &str) {
    (
        &profile.species.catalog,
        &profile.species.identifier,
        &profile.trait_id,
        &profile.source_record_id,
    )
}

fn metabolic_selection_key(selection: &FaunaMetabolicRateSelection) -> (&str, &str) {
    (&selection.species.catalog, &selection.species.identifier)
}

fn body_mass_selection_key(selection: &FaunaBodyMassSelection) -> (&str, &str) {
    (&selection.species.catalog, &selection.species.identifier)
}

fn ecology_entry_key(entry: &FaunaEcologyPlanEntry) -> (&str, &str) {
    (&entry.species.catalog, &entry.species.identifier)
}

fn same_catalog_taxon(left: &SpeciesIdentity, right: &SpeciesIdentity) -> bool {
    left.catalog == right.catalog && left.identifier == right.identifier
}
fn slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
fn technical(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FaunaPhysiologyProfileError {
    #[error("unsupported fauna physiology profile schema")]
    UnsupportedSchema,
    #[error("fauna physiology profiles must be nonempty and source-pinned")]
    EmptyOrUnpinned,
    #[error("fauna physiology profiles are not canonically ordered")]
    NonCanonicalOrder,
    #[error("invalid fauna physiology profile")]
    InvalidProfile,
    #[error("unsupported fauna physiology profile catalog schema")]
    UnsupportedCatalogSchema,
    #[error("fauna physiology profile catalog must not be empty")]
    EmptyCatalog,
    #[error("fauna physiology profile catalog is not canonically ordered")]
    NonCanonicalCatalogOrder,
    #[error("invalid fauna physiology profile catalog reference")]
    InvalidCatalogReference,
    #[error("unsupported fauna metabolic-rate selection schema")]
    UnsupportedMetabolicSelectionSchema,
    #[error("unsupported fauna metabolic-rate plan schema")]
    UnsupportedMetabolicPlanSchema,
    #[error("fauna metabolic-rate plan must not be empty")]
    EmptyMetabolicPlan,
    #[error("fauna metabolic-rate plan is not canonically ordered")]
    NonCanonicalMetabolicPlanOrder,
    #[error("invalid fauna metabolic-rate selection")]
    InvalidMetabolicSelection,
    #[error("metabolic-rate selection does not match the supplied profile set")]
    MetabolicProfileSetMismatch,
    #[error("selected metabolic-rate profile is absent")]
    SelectedMetabolicProfileMissing,
    #[error("selected metabolic-rate profile is not a positive watt observation")]
    InvalidSelectedMetabolicProfile,
    #[error("unsupported fauna body-mass selection schema")]
    UnsupportedBodyMassSelectionSchema,
    #[error("unsupported fauna body-mass plan schema")]
    UnsupportedBodyMassPlanSchema,
    #[error("fauna body-mass plan is not canonically ordered")]
    NonCanonicalBodyMassPlanOrder,
    #[error("invalid fauna body-mass selection")]
    InvalidBodyMassSelection,
    #[error("body-mass selection does not match the supplied profile set")]
    BodyMassProfileSetMismatch,
    #[error("selected body-mass profile is absent")]
    SelectedBodyMassProfileMissing,
    #[error("selected body-mass profile is not a positive gram observation")]
    InvalidSelectedBodyMassProfile,
    #[error("invalid fauna ecology plan")]
    InvalidEcologyPlan,
    #[error("fauna ecology plan is not canonically ordered")]
    NonCanonicalEcologyPlanOrder,
    #[error("fauna ecology plan does not match the supplied profile set")]
    EcologyProfileSetMismatch,
    #[error("selected fauna ecology profile is absent")]
    SelectedEcologyProfileMissing,
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
    #[test]
    fn profiles_are_canonical_and_provenance_complete() {
        let profile = FaunaPhysiologyProfile {
            species: SpeciesIdentity::new(
                "gbif",
                "5219173",
                "Canis lupus",
                "https://www.gbif.org/species/5219173",
            )
            .expect("valid fixture species"),
            trait_id: "adult-body-mass".to_owned(),
            value: ScaledFaunaTraitValue {
                value: 30,
                decimal_places: 0,
                unit: "kg".to_owned(),
            },
            source: FaunaEvidenceSource::AnimalTraitsV1_0_7,
            source_field: "body_mass".to_owned(),
            source_record_id: "row-1".to_owned(),
            source_record_digest: Digest::sha256(b"row"),
            evidence_basis: FaunaEvidenceBasis::EmpiricalObservation,
        };
        let set = FaunaPhysiologyProfileSet {
            profile_set_schema_version: 1,
            source_artifact_digest: Digest::sha256(b"source"),
            profiles: vec![profile],
        };
        let bytes = set.canonical_bytes().expect("canonical fixture");
        assert_eq!(
            FaunaPhysiologyProfileSet::from_canonical_slice(&bytes),
            Ok(set)
        );
    }

    #[test]
    fn catalog_pins_independent_profile_sets_without_erasing_sources() {
        let catalog = FaunaPhysiologyProfileCatalog {
            profile_catalog_schema_version: 1,
            profile_sets: vec![FaunaPhysiologyProfileSetReference {
                profile_set_id: "amniote-life-history-v1".to_owned(),
                profile_set_digest: Digest::sha256(b"compiled profiles"),
                source_artifact_digest: Digest::sha256(b"source artifact"),
                profile_count: 1,
            }],
        };
        let bytes = catalog.canonical_bytes().expect("canonical fixture");
        assert_eq!(
            FaunaPhysiologyProfileCatalog::from_canonical_slice(&bytes),
            Ok(catalog)
        );
    }

    #[test]
    fn metabolic_selection_pins_one_exact_source_observation() {
        let species = SpeciesIdentity::new(
            "gbif",
            "5219173",
            "Canis lupus",
            "https://www.gbif.org/species/5219173",
        )
        .expect("valid fixture species");
        let profiles = FaunaPhysiologyProfileSet {
            profile_set_schema_version: 1,
            source_artifact_digest: Digest::sha256(b"source"),
            profiles: vec![FaunaPhysiologyProfile {
                species: species.clone(),
                trait_id: "standardized-metabolic-rate".to_owned(),
                value: ScaledFaunaTraitValue {
                    value: 125,
                    decimal_places: 3,
                    unit: "W".to_owned(),
                },
                source: FaunaEvidenceSource::AnimalTraitsV1_0_7,
                source_field: "metabolic_rate".to_owned(),
                source_record_id: "animaltraits-observations-line-7".to_owned(),
                source_record_digest: Digest::sha256(b"row"),
                evidence_basis: FaunaEvidenceBasis::EmpiricalObservation,
            }],
        };
        let selection = FaunaMetabolicRateSelection {
            selection_schema_version: 1,
            profile_set_digest: Digest::sha256(
                &profiles.canonical_bytes().expect("canonical profile set"),
            ),
            species,
            source_record_id: "animaltraits-observations-line-7".to_owned(),
        };
        assert_eq!(
            selection
                .resolve(&profiles)
                .expect("exact profile resolves")
                .value
                .unit,
            "W"
        );
        let commitment = selection
            .resolve_commitment(&profiles)
            .expect("exact commitment resolves");
        assert_eq!(commitment.measured_power_value, 125);
        assert_eq!(commitment.source_record_digest, Digest::sha256(b"row"));
        let bytes = selection.canonical_bytes().expect("canonical selection");
        assert_eq!(
            FaunaMetabolicRateSelection::from_canonical_slice(&bytes),
            Ok(selection.clone())
        );
        let plan = FaunaMetabolicRatePlan {
            plan_schema_version: 1,
            selections: vec![selection.clone()],
        };
        assert_eq!(
            FaunaMetabolicRatePlan::from_canonical_slice(
                &plan.canonical_bytes().expect("canonical plan")
            ),
            Ok(plan)
        );
        let missing = FaunaMetabolicRateSelection {
            source_record_id: "animaltraits-observations-line-8".to_owned(),
            ..selection
        };
        assert_eq!(
            missing.resolve(&profiles),
            Err(FaunaPhysiologyProfileError::SelectedMetabolicProfileMissing)
        );
    }

    #[test]
    fn body_mass_selection_pins_one_observation_without_averaging() {
        let species = SpeciesIdentity::new(
            "gbif",
            "5219173",
            "Canis lupus",
            "https://www.gbif.org/species/5219173",
        )
        .expect("valid fixture species");
        let profiles = FaunaPhysiologyProfileSet {
            profile_set_schema_version: 1,
            source_artifact_digest: Digest::sha256(b"source"),
            profiles: vec![FaunaPhysiologyProfile {
                species: species.clone(),
                trait_id: "adult-body-mass".to_owned(),
                value: ScaledFaunaTraitValue {
                    value: 30_000,
                    decimal_places: 0,
                    unit: "g".to_owned(),
                },
                source: FaunaEvidenceSource::AnimalTraitsV1_0_7,
                source_field: "body_mass".to_owned(),
                source_record_id: "animaltraits-observations-line-8".to_owned(),
                source_record_digest: Digest::sha256(b"mass row"),
                evidence_basis: FaunaEvidenceBasis::EmpiricalObservation,
            }],
        };
        let selection = FaunaBodyMassSelection {
            selection_schema_version: FAUNA_BODY_MASS_SELECTION_SCHEMA_VERSION,
            profile_set_digest: Digest::sha256(
                &profiles.canonical_bytes().expect("canonical profile set"),
            ),
            species,
            source_record_id: "animaltraits-observations-line-8".to_owned(),
        };
        assert_eq!(
            selection
                .resolve(&profiles)
                .expect("exact profile resolves")
                .value
                .value,
            30_000
        );
        let plan = FaunaBodyMassPlan {
            plan_schema_version: FAUNA_BODY_MASS_PLAN_SCHEMA_VERSION,
            selections: vec![selection],
        };
        let bytes = plan.canonical_bytes().expect("canonical plan");
        assert_eq!(FaunaBodyMassPlan::from_canonical_slice(&bytes), Ok(plan));
    }

    #[test]
    fn ecology_plan_retains_rows_without_creating_behavior() {
        let species = SpeciesIdentity::new(
            "gbif",
            "5219173",
            "Canis lupus",
            "https://www.gbif.org/species/5219173",
        )
        .expect("valid fixture species");
        let profile = FaunaPhysiologyProfile {
            species: species.clone(),
            trait_id: "diet-terrestrial-vertebrate-share-percent".to_owned(),
            value: ScaledFaunaTraitValue {
                value: 80,
                decimal_places: 0,
                unit: "percent".to_owned(),
            },
            source: FaunaEvidenceSource::EltonTraitsV1_0,
            source_field: "VertEnd".to_owned(),
            source_record_id: "elton-mammal-line-7-diet-vertend".to_owned(),
            source_record_digest: Digest::sha256(b"ecology row"),
            evidence_basis: FaunaEvidenceBasis::SourceCompiledSpeciesAggregate,
        };
        let profiles = FaunaPhysiologyProfileSet {
            profile_set_schema_version: 1,
            source_artifact_digest: Digest::sha256(b"Elton source"),
            profiles: vec![profile.clone()],
        };
        let plan = FaunaEcologyPlan {
            plan_schema_version: FAUNA_ECOLOGY_PLAN_SCHEMA_VERSION,
            profile_set_digest: Digest::sha256(
                &profiles.canonical_bytes().expect("canonical profiles"),
            ),
            entries: vec![FaunaEcologyPlanEntry {
                species,
                source_record_ids: vec![profile.source_record_id.clone()],
            }],
        };
        assert_eq!(plan.resolve(&profiles), Ok(vec![&profile]));
        let bytes = plan.canonical_bytes().expect("canonical ecology plan");
        assert_eq!(FaunaEcologyPlan::from_canonical_slice(&bytes), Ok(plan));
    }

    #[test]
    fn metabolic_selection_joins_catalog_identity_not_name_rendering() {
        let world_species = SpeciesIdentity::new(
            "gbif",
            "9510564",
            "Turdus migratorius",
            "https://www.gbif.org/species/9510564",
        )
        .expect("world identity");
        let source_species = SpeciesIdentity::new(
            "gbif",
            "9510564",
            "Turdus migratorius Linnaeus, 1766",
            "https://www.gbif.org/species/9510564",
        )
        .expect("source identity");
        let profiles = FaunaPhysiologyProfileSet {
            profile_set_schema_version: 1,
            source_artifact_digest: Digest::sha256(b"source with authority names"),
            profiles: vec![FaunaPhysiologyProfile {
                species: source_species,
                trait_id: "standardized-metabolic-rate".to_owned(),
                value: ScaledFaunaTraitValue {
                    value: 944,
                    decimal_places: 3,
                    unit: "W".to_owned(),
                },
                source: FaunaEvidenceSource::AnimalTraitsV1_0_7,
                source_field: "metabolic_rate".to_owned(),
                source_record_id: "animaltraits-observations-line-1".to_owned(),
                source_record_digest: Digest::sha256(b"exact source row"),
                evidence_basis: FaunaEvidenceBasis::EmpiricalObservation,
            }],
        };
        let selection = FaunaMetabolicRateSelection {
            selection_schema_version: 1,
            profile_set_digest: Digest::sha256(
                &profiles.canonical_bytes().expect("canonical profiles"),
            ),
            species: world_species.clone(),
            source_record_id: "animaltraits-observations-line-1".to_owned(),
        };
        let commitment = selection
            .resolve_commitment(&profiles)
            .expect("stable taxon identifier resolves across label rendering");
        assert_eq!(commitment.observed_species, world_species);
        assert_eq!(
            commitment.source_record_digest,
            Digest::sha256(b"exact source row")
        );
    }

    #[test]
    fn current_metabolic_plan_can_record_complete_source_absence() {
        let plan = FaunaMetabolicRatePlan {
            plan_schema_version: FAUNA_METABOLIC_RATE_PLAN_SCHEMA_VERSION,
            selections: Vec::new(),
        };
        let bytes = plan
            .canonical_bytes()
            .expect("canonical empty coverage plan");
        assert_eq!(
            FaunaMetabolicRatePlan::from_canonical_slice(&bytes),
            Ok(plan)
        );
        let legacy = FaunaMetabolicRatePlan {
            plan_schema_version: LEGACY_FAUNA_METABOLIC_RATE_PLAN_SCHEMA_VERSION,
            selections: Vec::new(),
        };
        assert_eq!(
            legacy.validate(),
            Err(FaunaPhysiologyProfileError::EmptyMetabolicPlan)
        );
    }
}
