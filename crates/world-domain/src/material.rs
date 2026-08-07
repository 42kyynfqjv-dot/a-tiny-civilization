//! Source-addressable real-world material identities.
//!
//! A material reference identifies a physical substance or mineral in a cited
//! external catalog. It deliberately carries no culturally privileged use or
//! inferred affordance; mechanics must separately pin measurements and effects.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A stable, citable identity for a real-world physical material.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaterialIdentity {
    pub catalog: String,
    pub identifier: String,
    pub canonical_name: String,
    pub source_url: String,
}

impl MaterialIdentity {
    pub fn new(
        catalog: impl Into<String>,
        identifier: impl Into<String>,
        canonical_name: impl Into<String>,
        source_url: impl Into<String>,
    ) -> Result<Self, MaterialIdentityError> {
        let identity = Self {
            catalog: catalog.into(),
            identifier: identifier.into(),
            canonical_name: canonical_name.into(),
            source_url: source_url.into(),
        };
        identity.validate()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<(), MaterialIdentityError> {
        if self.catalog.trim().is_empty()
            || self.identifier.trim().is_empty()
            || self.canonical_name.trim().is_empty()
        {
            return Err(MaterialIdentityError::MissingIdentityField);
        }
        if !self.source_url.starts_with("https://") {
            return Err(MaterialIdentityError::NonHttpsSource);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum MaterialIdentityError {
    #[error("material catalog, identifier, and canonical name are required")]
    MissingIdentityField,
    #[error("material source URL must use HTTPS")]
    NonHttpsSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materials_require_a_citable_real_world_identity() {
        let water = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("citable material");
        assert_eq!(water.canonical_name, "water");
        assert!(matches!(
            MaterialIdentity::new("", "962", "water", "https://example.test/material"),
            Err(MaterialIdentityError::MissingIdentityField)
        ));
    }
}
