//! Canonical, source-addressable organism physiology inputs.
//!
//! This is intentionally an evidence hand-off, not a physiology model. A later
//! engine ruleset may consume only a profile whose values and source rows are pinned.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, SpeciesIdentity};

use crate::{FaunaEvidenceBasis, FaunaEvidenceSource, ScaledFaunaTraitValue};

pub const FAUNA_PHYSIOLOGY_PROFILE_SET_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_PHYSIOLOGY_PROFILE_SET_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-physiology-profile-set+json";

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
}
