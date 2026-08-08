//! Source-addressable real-world material identities.
//!
//! A material reference identifies a physical substance or mineral in a cited
//! external catalog. It deliberately carries no culturally privileged use or
//! inferred affordance; mechanics must separately pin measurements and effects.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Digest, S2CellId, SpeciesIdentity};

pub const ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION: u16 = 1;
pub const MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION: u16 = 1;

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

/// Weakest evidence class used by the physical oral-transfer profile. The profile is
/// canonical world evidence, never a property or conclusion exposed to an organism.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OralTransferEvidenceBasis {
    SourceMeasurement,
    LiteratureApproximation,
    EngineeringAssumption,
}

/// Canonical physical behavior of one spatially anchored material reservoir.
///
/// The coverage and replenishment values are infrastructure-visible mechanics, not
/// concepts exposed to organisms. A source identity tells observers what the material
/// really is; the evidence basis says whether the quantitative ecology is measured or
/// merely an explicit engineering assumption.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaterialReservoirCommitment {
    pub commitment_schema_version: u16,
    pub profile_id: String,
    pub profile_digest: Digest,
    pub material: MaterialIdentity,
    pub evidence_basis: OralTransferEvidenceBasis,
    /// Every embodied patch contained by this cell can physically access the source.
    pub coverage_patch: S2CellId,
    pub maximum_mass_milligrams: u64,
    pub replenishment_mass_milligrams_per_tick: u64,
}

impl MaterialReservoirCommitment {
    pub fn validate(&self) -> Result<(), MaterialReservoirCommitmentError> {
        if self.commitment_schema_version != MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION {
            return Err(MaterialReservoirCommitmentError::UnsupportedSchema);
        }
        self.material
            .validate()
            .map_err(|_| MaterialReservoirCommitmentError::InvalidCommitment)?;
        if !is_technical(&self.profile_id)
            || self.profile_digest == Digest::ZERO
            || self.maximum_mass_milligrams == 0
            || self.replenishment_mass_milligrams_per_tick == 0
            || self.replenishment_mass_milligrams_per_tick > self.maximum_mass_milligrams
        {
            return Err(MaterialReservoirCommitmentError::InvalidCommitment);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum MaterialReservoirCommitmentError {
    #[error("unsupported material-reservoir commitment schema")]
    UnsupportedSchema,
    #[error("invalid material-reservoir commitment")]
    InvalidCommitment,
}

/// Species-specific physical consequences of transferring one retained portion of a
/// material through the mouth. This deliberately does not say that the material is
/// food, safe, desirable, or known. Version one models only exact energy and hydration
/// recovery; toxicity and injury require later, separately versioned causal state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OralTransferCommitment {
    pub commitment_schema_version: u16,
    pub profile_id: String,
    pub profile_digest: Digest,
    pub material: MaterialIdentity,
    pub species: SpeciesIdentity,
    pub evidence_basis: OralTransferEvidenceBasis,
    pub transfer_mass_milligrams: u64,
    pub recoverable_energy_joules: u64,
    /// Exact reduction in the target regulator's hydration-failure time load.
    pub hydration_recovery_seconds: u64,
}

impl OralTransferCommitment {
    pub fn validate(&self) -> Result<(), OralTransferCommitmentError> {
        if self.commitment_schema_version != ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION {
            return Err(OralTransferCommitmentError::UnsupportedSchema);
        }
        if !is_technical(&self.profile_id)
            || self.profile_digest == Digest::ZERO
            || self.transfer_mass_milligrams == 0
        {
            return Err(OralTransferCommitmentError::InvalidCommitment);
        }
        self.material
            .validate()
            .map_err(|_| OralTransferCommitmentError::InvalidCommitment)?;
        self.species
            .validate()
            .map_err(|_| OralTransferCommitmentError::InvalidCommitment)?;
        Ok(())
    }
}

fn is_technical(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OralTransferCommitmentError {
    #[error("unsupported oral-transfer commitment schema")]
    UnsupportedSchema,
    #[error("invalid oral-transfer commitment")]
    InvalidCommitment,
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

    #[test]
    fn oral_transfer_profiles_bind_material_species_and_evidence() {
        let material = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("citable material");
        let species = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("citable species");
        let profile = OralTransferCommitment {
            commitment_schema_version: ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
            profile_id: "water-human-fixture-v1".to_owned(),
            profile_digest: Digest::sha256(b"oral transfer fixture"),
            material,
            species,
            evidence_basis: OralTransferEvidenceBasis::EngineeringAssumption,
            transfer_mass_milligrams: 250_000,
            recoverable_energy_joules: 0,
            hydration_recovery_seconds: 14_400,
        };
        profile.validate().expect("valid oral transfer profile");

        let mut unsupported = profile.clone();
        unsupported.commitment_schema_version = 2;
        assert_eq!(
            unsupported.validate(),
            Err(OralTransferCommitmentError::UnsupportedSchema)
        );
        let mut zero_mass = profile;
        zero_mass.transfer_mass_milligrams = 0;
        assert_eq!(
            zero_mass.validate(),
            Err(OralTransferCommitmentError::InvalidCommitment)
        );
    }

    #[test]
    fn material_reservoirs_are_bounded_spatial_and_explicitly_evidenced() {
        let material = MaterialIdentity::new(
            "pubchem",
            "962",
            "water",
            "https://pubchem.ncbi.nlm.nih.gov/compound/962",
        )
        .expect("citable material");
        let reservoir = MaterialReservoirCommitment {
            commitment_schema_version: MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
            profile_id: "provisional-water-reservoir-v1".to_owned(),
            profile_digest: Digest::sha256(b"water reservoir fixture"),
            material,
            evidence_basis: OralTransferEvidenceBasis::EngineeringAssumption,
            coverage_patch: S2CellId::new(1_u64 << 60).expect("face root"),
            maximum_mass_milligrams: 1_000_000,
            replenishment_mass_milligrams_per_tick: 250_000,
        };
        reservoir.validate().expect("valid material reservoir");

        let mut impossible = reservoir;
        impossible.replenishment_mass_milligrams_per_tick = 1_000_001;
        assert_eq!(
            impossible.validate(),
            Err(MaterialReservoirCommitmentError::InvalidCommitment)
        );
    }
}
