//! Source-backed real-taxon identities and tick-zero fauna identity policy.
//!
//! This catalog binds engine identity decisions to retained taxonomic evidence. It
//! deliberately says nothing about occurrence, abundance, habitat, physiology, or
//! behavior; those require separately licensed and validated evidence.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{BirthCategory, Digest, SpeciesIdentity};

pub const FAUNA_CATALOG_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_CATALOG_MEDIA_TYPE: &str = "application/vnd.atinycivilization.fauna-catalog+json";

/// Whether organisms at one declared life stage retain individual or cohort identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FaunaIdentityTier {
    Individual,
    Cohort,
}

/// Identity and history requirements for one real species at one life stage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaIdentityPolicy {
    pub life_stage: String,
    pub identity_tier: FaunaIdentityTier,
    /// Individual birth facts carry stable organism identity and deterministic order.
    pub emits_individual_birth_events: bool,
    /// Individual death facts preserve the end of every retained biography.
    pub emits_individual_death_events: bool,
    /// Birth facts retain parent identities whenever the simulation knows them.
    pub retains_parent_lineage_when_known: bool,
    /// Observer-side naming may be offered only after all individual-history
    /// prerequisites are active. This flag never causes or changes a birth.
    pub supporter_naming_eligible: bool,
}

/// One actual Earth animal taxon and its versioned engine identity declaration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaCatalogEntry {
    pub species: SpeciesIdentity,
    /// Digest of this exact normalized taxon record within the aggregate catalog.
    pub taxon_record_digest: Digest,
    /// Versioned categories that may appear on canonical birth facts. These are
    /// engine metadata and are never agent-visible labels or explicit public copy.
    pub sex_categories: Vec<BirthCategory>,
    /// Strictly life-stage-ordered identity declarations.
    pub identity_policies: Vec<FaunaIdentityPolicy>,
}

/// A canonical identity-policy catalog derived from one retained taxonomy snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaCatalog {
    pub catalog_schema_version: u16,
    pub catalog_id: String,
    pub identity_policy_version: u32,
    pub source_snapshot_digest: Digest,
    /// Digest of the canonical aggregate taxonomic artifact from which entries came.
    pub source_artifact_digest: Digest,
    /// Strictly ordered by `(catalog, identifier)` with no repeated real taxon.
    pub entries: Vec<FaunaCatalogEntry>,
}

impl FaunaCatalog {
    pub fn validate(&self) -> Result<(), FaunaCatalogError> {
        if self.catalog_schema_version != FAUNA_CATALOG_SCHEMA_VERSION {
            return Err(FaunaCatalogError::UnsupportedSchema(
                self.catalog_schema_version,
            ));
        }
        if !slug(&self.catalog_id) {
            return Err(FaunaCatalogError::InvalidIdentifier);
        }
        if self.identity_policy_version == 0 {
            return Err(FaunaCatalogError::ZeroIdentityPolicyVersion);
        }
        if self.source_snapshot_digest == Digest::ZERO
            || self.source_artifact_digest == Digest::ZERO
        {
            return Err(FaunaCatalogError::ZeroDigest);
        }
        if self.entries.is_empty() {
            return Err(FaunaCatalogError::EmptyCatalog);
        }

        let mut previous_species: Option<(&str, &str)> = None;
        for entry in &self.entries {
            entry
                .species
                .validate()
                .map_err(|error| FaunaCatalogError::InvalidSpecies(error.to_string()))?;
            let species_key = (
                entry.species.catalog.as_str(),
                entry.species.identifier.as_str(),
            );
            if previous_species.is_some_and(|previous| previous >= species_key) {
                return Err(FaunaCatalogError::NonCanonicalSpeciesOrder);
            }
            previous_species = Some(species_key);
            entry.validate()?;
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FaunaCatalogError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| FaunaCatalogError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, FaunaCatalogError> {
        let catalog: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaCatalogError::Decode(error.to_string()))?;
        catalog.validate()?;
        if catalog.canonical_bytes()? != bytes {
            return Err(FaunaCatalogError::NonCanonicalEncoding);
        }
        Ok(catalog)
    }
}

impl FaunaCatalogEntry {
    fn validate(&self) -> Result<(), FaunaCatalogError> {
        if self.taxon_record_digest == Digest::ZERO {
            return Err(FaunaCatalogError::ZeroDigest);
        }
        if self.sex_categories.is_empty() {
            return Err(FaunaCatalogError::MissingSexCategories);
        }
        if self
            .sex_categories
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(FaunaCatalogError::NonCanonicalSexCategoryOrder);
        }
        if self.identity_policies.is_empty() {
            return Err(FaunaCatalogError::MissingIdentityPolicy);
        }

        let mut previous_stage: Option<&str> = None;
        for policy in &self.identity_policies {
            if !slug(&policy.life_stage) {
                return Err(FaunaCatalogError::InvalidIdentifier);
            }
            if previous_stage.is_some_and(|previous| previous >= policy.life_stage.as_str()) {
                return Err(FaunaCatalogError::NonCanonicalLifeStageOrder);
            }
            previous_stage = Some(&policy.life_stage);
            policy.validate()?;
        }
        Ok(())
    }
}

impl FaunaIdentityPolicy {
    fn validate(&self) -> Result<(), FaunaCatalogError> {
        let retains_complete_individual_history = self.emits_individual_birth_events
            && self.emits_individual_death_events
            && self.retains_parent_lineage_when_known;

        match self.identity_tier {
            FaunaIdentityTier::Individual if !retains_complete_individual_history => {
                Err(FaunaCatalogError::IncompleteIndividualHistory)
            }
            FaunaIdentityTier::Cohort
                if self.emits_individual_birth_events
                    || self.emits_individual_death_events
                    || self.retains_parent_lineage_when_known
                    || self.supporter_naming_eligible =>
            {
                Err(FaunaCatalogError::CohortClaimsIndividualHistory)
            }
            FaunaIdentityTier::Individual | FaunaIdentityTier::Cohort => Ok(()),
        }
    }
}

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FaunaCatalogError {
    #[error("unsupported fauna-catalog schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid fauna-catalog identifier")]
    InvalidIdentifier,
    #[error("fauna identity-policy version must be nonzero")]
    ZeroIdentityPolicyVersion,
    #[error("fauna provenance digest must not be zero")]
    ZeroDigest,
    #[error("fauna catalog must contain at least one real taxon")]
    EmptyCatalog,
    #[error("invalid real species identity: {0}")]
    InvalidSpecies(String),
    #[error("fauna entries must be strictly ordered by catalog and identifier")]
    NonCanonicalSpeciesOrder,
    #[error("fauna taxon must declare at least one sex category")]
    MissingSexCategories,
    #[error("fauna sex categories must be strictly ordered")]
    NonCanonicalSexCategoryOrder,
    #[error("fauna taxon must declare at least one life-stage identity policy")]
    MissingIdentityPolicy,
    #[error("fauna identity policies must be strictly ordered by life stage")]
    NonCanonicalLifeStageOrder,
    #[error("individual fauna tiers must retain birth, death, and known-parent lineage facts")]
    IncompleteIndividualHistory,
    #[error("cohort fauna tiers cannot claim individual history or supporter eligibility")]
    CohortClaimsIndividualHistory,
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

    fn category(value: &str) -> BirthCategory {
        BirthCategory::new(value).expect("valid birth category")
    }

    fn entry(identifier: &str, scientific_name: &str) -> FaunaCatalogEntry {
        FaunaCatalogEntry {
            species: SpeciesIdentity::new(
                "gbif",
                identifier,
                scientific_name,
                format!("https://www.gbif.org/species/{identifier}"),
            )
            .expect("valid real species identity"),
            taxon_record_digest: Digest::sha256(identifier.as_bytes()),
            sex_categories: vec![category("female"), category("male")],
            identity_policies: vec![FaunaIdentityPolicy {
                life_stage: "all-life-stages".to_owned(),
                identity_tier: FaunaIdentityTier::Individual,
                emits_individual_birth_events: true,
                emits_individual_death_events: true,
                retains_parent_lineage_when_known: true,
                supporter_naming_eligible: true,
            }],
        }
    }

    fn catalog() -> FaunaCatalog {
        FaunaCatalog {
            catalog_schema_version: FAUNA_CATALOG_SCHEMA_VERSION,
            catalog_id: "gbif-animalia-2023-08-28-provisional".to_owned(),
            identity_policy_version: 1,
            source_snapshot_digest: Digest::sha256(b"gbif-backbone-snapshot"),
            source_artifact_digest: Digest::sha256(b"gbif-animalia-derived-catalog"),
            entries: vec![
                entry("2441176", "Bison bison"),
                entry("5219173", "Canis lupus"),
            ],
        }
    }

    #[test]
    fn real_taxon_identity_catalog_round_trips_canonically() {
        let catalog = catalog();
        let bytes = catalog.canonical_bytes().expect("canonical fauna catalog");
        assert_eq!(FaunaCatalog::from_canonical_slice(&bytes), Ok(catalog));

        let mut noncanonical = bytes;
        noncanonical.push(b'\n');
        assert_eq!(
            FaunaCatalog::from_canonical_slice(&noncanonical),
            Err(FaunaCatalogError::NonCanonicalEncoding)
        );
    }

    #[test]
    fn rejects_unsourced_or_reordered_real_taxa() {
        let mut zero_record = catalog();
        zero_record.entries[0].taxon_record_digest = Digest::ZERO;
        assert_eq!(zero_record.validate(), Err(FaunaCatalogError::ZeroDigest));

        let mut reordered = catalog();
        reordered.entries.swap(0, 1);
        assert_eq!(
            reordered.validate(),
            Err(FaunaCatalogError::NonCanonicalSpeciesOrder)
        );

        let mut fictional = catalog();
        fictional.entries[0].species.source_url.clear();
        assert!(matches!(
            fictional.validate(),
            Err(FaunaCatalogError::InvalidSpecies(_))
        ));
    }

    #[test]
    fn individual_tiers_require_birth_death_and_known_parentage() {
        let mut incomplete = catalog();
        incomplete.entries[0].identity_policies[0].emits_individual_death_events = false;
        assert_eq!(
            incomplete.validate(),
            Err(FaunaCatalogError::IncompleteIndividualHistory)
        );
    }

    #[test]
    fn cohort_tiers_cannot_claim_biographies_or_supporter_eligibility() {
        let mut cohort = catalog();
        let policy = &mut cohort.entries[0].identity_policies[0];
        policy.identity_tier = FaunaIdentityTier::Cohort;
        policy.emits_individual_birth_events = false;
        policy.emits_individual_death_events = false;
        policy.retains_parent_lineage_when_known = false;
        policy.supporter_naming_eligible = false;
        assert!(cohort.validate().is_ok());

        cohort.entries[0].identity_policies[0].supporter_naming_eligible = true;
        assert_eq!(
            cohort.validate(),
            Err(FaunaCatalogError::CohortClaimsIndividualHistory)
        );
    }

    #[test]
    fn sex_categories_and_life_stages_have_canonical_order() {
        let mut duplicate_category = catalog();
        duplicate_category.entries[0].sex_categories[1] = category("female");
        assert_eq!(
            duplicate_category.validate(),
            Err(FaunaCatalogError::NonCanonicalSexCategoryOrder)
        );

        let mut duplicate_stage = catalog();
        let repeated = duplicate_stage.entries[0].identity_policies[0].clone();
        duplicate_stage.entries[0].identity_policies.push(repeated);
        assert_eq!(
            duplicate_stage.validate(),
            Err(FaunaCatalogError::NonCanonicalLifeStageOrder)
        );
    }
}
