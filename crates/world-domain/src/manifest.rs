use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{WorldId, WorldSeed};

/// A sourced real-world taxon identity visible to the engine, never directly to agents.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpeciesIdentity {
    pub catalog: String,
    pub identifier: String,
    pub scientific_name: String,
    pub source_url: String,
}

impl SpeciesIdentity {
    pub fn new(
        catalog: impl Into<String>,
        identifier: impl Into<String>,
        scientific_name: impl Into<String>,
        source_url: impl Into<String>,
    ) -> Result<Self, SpeciesIdentityError> {
        let identity = Self {
            catalog: catalog.into(),
            identifier: identifier.into(),
            scientific_name: scientific_name.into(),
            source_url: source_url.into(),
        };
        identity.validate()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<(), SpeciesIdentityError> {
        if self.catalog.trim().is_empty()
            || self.identifier.trim().is_empty()
            || self.scientific_name.trim().is_empty()
        {
            return Err(SpeciesIdentityError::MissingIdentityField);
        }
        if !self.source_url.starts_with("https://") {
            return Err(SpeciesIdentityError::NonHttpsSource);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SpeciesIdentityError {
    #[error("species catalog, identifier, and scientific name are required")]
    MissingIdentityField,
    #[error("species source URL must use HTTPS")]
    NonHttpsSource,
}

/// Immutable inputs pinned before a world's genesis event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldManifest {
    pub world_id: WorldId,
    pub seed: WorldSeed,
    pub ruleset_version: u32,
    pub identity_policy_version: u32,
    pub scientific_datasets: BTreeMap<String, String>,
}

impl WorldManifest {
    #[must_use]
    pub fn new(world_id: WorldId, seed: WorldSeed, ruleset_version: u32) -> Self {
        Self {
            world_id,
            seed,
            ruleset_version,
            identity_policy_version: 1,
            scientific_datasets: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn species_identity_requires_a_citable_real_taxon() {
        let valid = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        );
        assert!(valid.is_ok());

        let missing_source = SpeciesIdentity::new("gbif", "2436436", "Homo sapiens", "");
        assert_eq!(missing_source, Err(SpeciesIdentityError::NonHttpsSource));

        let manifest =
            WorldManifest::new(WorldId::from_uuid(Uuid::from_u128(1)), WorldSeed::new(7), 1);
        assert!(manifest.scientific_datasets.is_empty());
    }
}
