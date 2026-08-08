//! Canonical provisional real-material reservoirs for one genesis population.
//!
//! Material identities are real and cited. Quantitative availability, renewal, and
//! species responses remain explicit engineering assumptions until replaced by a
//! scientifically admitted ecology artifact.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{
    Digest, MaterialIdentity, MaterialReservoirCommitment, OralTransferCommitment, S2CellId,
    SpeciesIdentity, WorldSeed,
};

use crate::ProvisionalOrganismBodyProfilePlan;

pub const PROVISIONAL_MATERIAL_RESOURCE_PLAN_SCHEMA_VERSION: u16 = 2;
pub const PROVISIONAL_MATERIAL_RESOURCE_PLAN_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.provisional-material-resource-plan+json";
pub const PROVISIONAL_MATERIAL_RESOURCE_PLAN_STATUS: &str =
    "provisional-not-scientifically-admitted";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalMaterialResourceSource {
    pub source_id: String,
    pub material: MaterialIdentity,
    pub anchor_patch: S2CellId,
    pub initial_mass_milligrams: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reservoir: Option<MaterialReservoirCommitment>,
    /// Strictly ordered by `(catalog, identifier)` with one profile per species.
    pub oral_transfer_profiles: Vec<OralTransferCommitment>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalMaterialResourcePlan {
    pub plan_schema_version: u16,
    pub status: String,
    pub world_seed: WorldSeed,
    pub tick_duration_seconds: u32,
    pub origin_environment_digest: Digest,
    pub fauna_population_plan_digest: Digest,
    pub organism_body_profile_plan_digest: Digest,
    pub embodied_patch: S2CellId,
    /// Strictly ordered by `(material catalog, material identifier, source_id)`.
    pub sources: Vec<ProvisionalMaterialResourceSource>,
}

impl ProvisionalMaterialResourcePlan {
    pub fn validate(
        &self,
        body_profiles: &ProvisionalOrganismBodyProfilePlan,
    ) -> Result<(), ProvisionalMaterialResourcePlanError> {
        if !matches!(
            self.plan_schema_version,
            1 | PROVISIONAL_MATERIAL_RESOURCE_PLAN_SCHEMA_VERSION
        ) {
            return Err(ProvisionalMaterialResourcePlanError::UnsupportedSchema(
                self.plan_schema_version,
            ));
        }
        if self.status != PROVISIONAL_MATERIAL_RESOURCE_PLAN_STATUS {
            return Err(ProvisionalMaterialResourcePlanError::InvalidStatus(
                self.status.clone(),
            ));
        }
        if self.tick_duration_seconds == 0
            || self.origin_environment_digest == Digest::ZERO
            || self.fauna_population_plan_digest == Digest::ZERO
            || self.organism_body_profile_plan_digest == Digest::ZERO
        {
            return Err(ProvisionalMaterialResourcePlanError::InvalidPlanBinding);
        }
        body_profiles.validate().map_err(|error| {
            ProvisionalMaterialResourcePlanError::InvalidBodyPlan(error.to_string())
        })?;
        let body_profile_bytes = body_profiles.canonical_bytes().map_err(|error| {
            ProvisionalMaterialResourcePlanError::InvalidBodyPlan(error.to_string())
        })?;
        if self.organism_body_profile_plan_digest != Digest::sha256(&body_profile_bytes) {
            return Err(ProvisionalMaterialResourcePlanError::InvalidPlanBinding);
        }
        if body_profiles.tick_duration_seconds != self.tick_duration_seconds {
            return Err(ProvisionalMaterialResourcePlanError::TickDurationMismatch);
        }
        if self.sources.is_empty() {
            return Err(ProvisionalMaterialResourcePlanError::EmptyPlan);
        }

        let expected_species = body_profiles
            .entries
            .iter()
            .map(|entry| entry.species.clone())
            .collect::<Vec<_>>();
        let mut previous_source_key = None;
        for source in &self.sources {
            source.material.validate().map_err(|error| {
                ProvisionalMaterialResourcePlanError::InvalidMaterial(error.to_string())
            })?;
            if !technical_slug(&source.source_id)
                || source.initial_mass_milligrams == 0
                || source.anchor_patch != self.embodied_patch
            {
                return Err(ProvisionalMaterialResourcePlanError::InvalidSource(
                    source.source_id.clone(),
                ));
            }
            match &source.reservoir {
                Some(reservoir) => {
                    reservoir.validate().map_err(|error| {
                        ProvisionalMaterialResourcePlanError::InvalidReservoir(error.to_string())
                    })?;
                    if source.initial_mass_milligrams > reservoir.maximum_mass_milligrams
                        || source.material != reservoir.material
                        || !reservoir.coverage_patch.contains(source.anchor_patch)
                    {
                        return Err(ProvisionalMaterialResourcePlanError::InvalidSource(
                            source.source_id.clone(),
                        ));
                    }
                }
                None => {
                    if self.plan_schema_version < 2 || !source.oral_transfer_profiles.is_empty() {
                        return Err(ProvisionalMaterialResourcePlanError::InvalidSource(
                            source.source_id.clone(),
                        ));
                    }
                }
            }
            let source_key = (
                source.material.catalog.as_str(),
                source.material.identifier.as_str(),
                source.source_id.as_str(),
            );
            if previous_source_key.is_some_and(|previous| previous >= source_key) {
                return Err(ProvisionalMaterialResourcePlanError::NonCanonicalSourceOrder);
            }
            previous_source_key = Some(source_key);

            if source.reservoir.is_some()
                && source.oral_transfer_profiles.len() != expected_species.len()
            {
                return Err(
                    ProvisionalMaterialResourcePlanError::IncompleteSpeciesCoverage(
                        source.source_id.clone(),
                    ),
                );
            }
            for (profile, expected) in source.oral_transfer_profiles.iter().zip(&expected_species) {
                profile.validate().map_err(|error| {
                    ProvisionalMaterialResourcePlanError::InvalidOralTransfer(error.to_string())
                })?;
                if profile.material != source.material || profile.species != *expected {
                    return Err(
                        ProvisionalMaterialResourcePlanError::IncompleteSpeciesCoverage(
                            source.source_id.clone(),
                        ),
                    );
                }
            }
        }

        for species in expected_species {
            let profiles = self.sources.iter().filter_map(|source| {
                source
                    .oral_transfer_profiles
                    .binary_search_by(|profile| {
                        species_key(&profile.species).cmp(&species_key(&species))
                    })
                    .ok()
                    .map(|index| &source.oral_transfer_profiles[index])
            });
            let mut has_energy = false;
            let mut has_hydration = false;
            for profile in profiles {
                has_energy |= profile.recoverable_energy_joules > 0;
                has_hydration |= profile.hydration_recovery_seconds > 0;
            }
            if !has_energy || !has_hydration {
                return Err(ProvisionalMaterialResourcePlanError::MissingSurvivalRoute {
                    catalog: species.catalog,
                    identifier: species.identifier,
                });
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(
        &self,
        body_profiles: &ProvisionalOrganismBodyProfilePlan,
    ) -> Result<Vec<u8>, ProvisionalMaterialResourcePlanError> {
        self.validate(body_profiles)?;
        serde_json::to_vec(self)
            .map_err(|error| ProvisionalMaterialResourcePlanError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(
        bytes: &[u8],
        body_profiles: &ProvisionalOrganismBodyProfilePlan,
    ) -> Result<Self, ProvisionalMaterialResourcePlanError> {
        let plan: Self = serde_json::from_slice(bytes)
            .map_err(|error| ProvisionalMaterialResourcePlanError::Decode(error.to_string()))?;
        if plan.canonical_bytes(body_profiles)? != bytes {
            return Err(ProvisionalMaterialResourcePlanError::NonCanonicalEncoding);
        }
        Ok(plan)
    }
}

fn species_key(species: &SpeciesIdentity) -> (&str, &str) {
    (&species.catalog, &species.identifier)
}

fn technical_slug(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProvisionalMaterialResourcePlanError {
    #[error("unsupported provisional material-resource plan schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid provisional material-resource plan status {0:?}")]
    InvalidStatus(String),
    #[error("provisional material-resource plan has an invalid or zero input binding")]
    InvalidPlanBinding,
    #[error("provisional material-resource plan contains no sources")]
    EmptyPlan,
    #[error("invalid organism body-profile plan: {0}")]
    InvalidBodyPlan(String),
    #[error("material-resource and body-profile tick durations differ")]
    TickDurationMismatch,
    #[error("invalid real-material identity: {0}")]
    InvalidMaterial(String),
    #[error("invalid material reservoir: {0}")]
    InvalidReservoir(String),
    #[error("invalid material source {0:?}")]
    InvalidSource(String),
    #[error("material sources are not in canonical order or contain a duplicate")]
    NonCanonicalSourceOrder,
    #[error("material source {0:?} does not cover every body-plan species exactly")]
    IncompleteSpeciesCoverage(String),
    #[error("invalid oral-transfer commitment: {0}")]
    InvalidOralTransfer(String),
    #[error("species {catalog}:{identifier} lacks a positive energy or hydration route")]
    MissingSurvivalRoute { catalog: String, identifier: String },
    #[error("decode error: {0}")]
    Decode(String),
    #[error("encoding error: {0}")]
    Encoding(String),
    #[error("noncanonical encoding")]
    NonCanonicalEncoding,
}
