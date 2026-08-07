//! Canonical, explicitly provisional body profiles for a genesis population.
//!
//! This artifact can wire real taxa and source-addressed bodily commitments into a
//! ruleset-14/15 world without representing those profiles as scientifically admitted.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{
    HeritableDispositionProfile, MetabolicRateCommitment, PhysiologicalRegulationCommitment,
    ReproductivePhysiologyCommitment, SpeciesIdentity,
};

pub const LEGACY_PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION: u16 = 1;
pub const PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION: u16 = 2;
pub const PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.provisional-organism-body-profile-plan+json";
pub const PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_STATUS: &str =
    "provisional-not-scientifically-admitted";

/// One species-bound genesis body profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalOrganismBodyProfileEntry {
    pub species: SpeciesIdentity,
    pub initial_age_ticks: u64,
    pub metabolic_rate: MetabolicRateCommitment,
    pub physiological_regulation: PhysiologicalRegulationCommitment,
    pub reproductive_physiology: ReproductivePhysiologyCommitment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heritable_disposition_profile: Option<HeritableDispositionProfile>,
}

/// A canonical later-body-ruleset input that remains structurally marked as provisional.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalOrganismBodyProfilePlan {
    pub plan_schema_version: u16,
    pub status: String,
    pub tick_duration_seconds: u32,
    /// Strictly ordered by `(catalog, identifier)` with no repeated taxon.
    pub entries: Vec<ProvisionalOrganismBodyProfileEntry>,
}

impl ProvisionalOrganismBodyProfilePlan {
    pub fn validate(&self) -> Result<(), ProvisionalOrganismBodyProfilePlanError> {
        if !matches!(
            self.plan_schema_version,
            LEGACY_PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION
                | PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION
        ) {
            return Err(ProvisionalOrganismBodyProfilePlanError::UnsupportedSchema(
                self.plan_schema_version,
            ));
        }
        if self.status != PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_STATUS {
            return Err(ProvisionalOrganismBodyProfilePlanError::InvalidStatus(
                self.status.clone(),
            ));
        }
        if self.tick_duration_seconds == 0 {
            return Err(ProvisionalOrganismBodyProfilePlanError::ZeroTickDuration);
        }
        if self.entries.is_empty() {
            return Err(ProvisionalOrganismBodyProfilePlanError::EmptyPlan);
        }

        for pair in self.entries.windows(2) {
            let first = species_key(&pair[0].species);
            let second = species_key(&pair[1].species);
            if first == second {
                return Err(ProvisionalOrganismBodyProfilePlanError::DuplicateSpecies {
                    catalog: second.0.to_owned(),
                    identifier: second.1.to_owned(),
                });
            }
            if first > second {
                return Err(ProvisionalOrganismBodyProfilePlanError::NonCanonicalSpeciesOrder);
            }
        }

        for entry in &self.entries {
            entry.species.validate().map_err(|error| {
                ProvisionalOrganismBodyProfilePlanError::InvalidSpecies(error.to_string())
            })?;
            entry.metabolic_rate.validate().map_err(|error| {
                ProvisionalOrganismBodyProfilePlanError::InvalidMetabolicCommitment(
                    error.to_string(),
                )
            })?;
            entry.physiological_regulation.validate().map_err(|error| {
                ProvisionalOrganismBodyProfilePlanError::InvalidRegulationCommitment(
                    error.to_string(),
                )
            })?;
            entry.reproductive_physiology.validate().map_err(|error| {
                ProvisionalOrganismBodyProfilePlanError::InvalidReproductiveCommitment(
                    error.to_string(),
                )
            })?;
            if let Some(profile) = &entry.heritable_disposition_profile {
                profile.validate().map_err(|error| {
                    ProvisionalOrganismBodyProfilePlanError::InvalidHeritableDispositionProfile(
                        error.to_string(),
                    )
                })?;
            }

            if entry.metabolic_rate.observed_species != entry.species {
                return Err(ProvisionalOrganismBodyProfilePlanError::SpeciesMismatch(
                    "metabolic_rate",
                ));
            }
            if entry.physiological_regulation.species != entry.species {
                return Err(ProvisionalOrganismBodyProfilePlanError::SpeciesMismatch(
                    "physiological_regulation",
                ));
            }
            if entry.reproductive_physiology.species != entry.species {
                return Err(ProvisionalOrganismBodyProfilePlanError::SpeciesMismatch(
                    "reproductive_physiology",
                ));
            }
            if entry
                .heritable_disposition_profile
                .as_ref()
                .is_some_and(|profile| profile.species != entry.species)
            {
                return Err(ProvisionalOrganismBodyProfilePlanError::SpeciesMismatch(
                    "heritable_disposition_profile",
                ));
            }
            match (
                self.plan_schema_version,
                entry.heritable_disposition_profile.is_some(),
            ) {
                (LEGACY_PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION, false)
                | (PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION, true) => {}
                (LEGACY_PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION, true) => {
                    return Err(ProvisionalOrganismBodyProfilePlanError::HeredityRequiresSchemaTwo);
                }
                (PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION, false) => {
                    return Err(
                        ProvisionalOrganismBodyProfilePlanError::MissingHeritableDispositionProfile,
                    );
                }
                _ => unreachable!("plan schema checked above"),
            }
            if entry.reproductive_physiology.tick_duration_seconds != self.tick_duration_seconds {
                return Err(
                    ProvisionalOrganismBodyProfilePlanError::ReproductiveTickDurationMismatch {
                        plan: self.tick_duration_seconds,
                        commitment: entry.reproductive_physiology.tick_duration_seconds,
                    },
                );
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn entry_for(
        &self,
        species: &SpeciesIdentity,
    ) -> Option<&ProvisionalOrganismBodyProfileEntry> {
        self.entries
            .binary_search_by(|entry| species_key(&entry.species).cmp(&species_key(species)))
            .ok()
            .map(|index| &self.entries[index])
            .filter(|entry| entry.species == *species)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProvisionalOrganismBodyProfilePlanError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| ProvisionalOrganismBodyProfilePlanError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(
        bytes: &[u8],
    ) -> Result<Self, ProvisionalOrganismBodyProfilePlanError> {
        let plan: Self = serde_json::from_slice(bytes)
            .map_err(|error| ProvisionalOrganismBodyProfilePlanError::Decode(error.to_string()))?;
        if plan.canonical_bytes()? != bytes {
            return Err(ProvisionalOrganismBodyProfilePlanError::NonCanonicalEncoding);
        }
        Ok(plan)
    }
}

fn species_key(species: &SpeciesIdentity) -> (&str, &str) {
    (&species.catalog, &species.identifier)
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProvisionalOrganismBodyProfilePlanError {
    #[error("unsupported provisional organism body-profile plan schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid provisional organism body-profile plan status {0:?}")]
    InvalidStatus(String),
    #[error("provisional organism body-profile plan tick duration must be nonzero")]
    ZeroTickDuration,
    #[error("provisional organism body-profile plan must contain at least one entry")]
    EmptyPlan,
    #[error("invalid species identity: {0}")]
    InvalidSpecies(String),
    #[error("provisional organism body-profile entries are not in canonical species order")]
    NonCanonicalSpeciesOrder,
    #[error("duplicate species {catalog}:{identifier} in provisional organism body-profile plan")]
    DuplicateSpecies { catalog: String, identifier: String },
    #[error("invalid metabolic-rate commitment: {0}")]
    InvalidMetabolicCommitment(String),
    #[error("invalid physiological-regulation commitment: {0}")]
    InvalidRegulationCommitment(String),
    #[error("invalid reproductive-physiology commitment: {0}")]
    InvalidReproductiveCommitment(String),
    #[error("invalid heritable-disposition profile: {0}")]
    InvalidHeritableDispositionProfile(String),
    #[error("heritable-disposition profiles require body-profile plan schema two")]
    HeredityRequiresSchemaTwo,
    #[error("body-profile plan schema two requires a heritable-disposition profile per species")]
    MissingHeritableDispositionProfile,
    #[error("{0} commitment species does not exactly match its plan entry")]
    SpeciesMismatch(&'static str),
    #[error("reproductive tick duration {commitment} does not match plan tick duration {plan}")]
    ReproductiveTickDurationMismatch { plan: u32, commitment: u32 },
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
    use world_domain::{
        BirthCategory, Digest, HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
        METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION, OffspringCategoryWeight,
        PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION, PhysiologicalEvidenceBasis,
        REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION, REPRODUCTIVE_PROBABILITY_SCALE,
        ReproductiveCategoryPair,
    };

    fn species(identifier: &str, scientific_name: &str) -> SpeciesIdentity {
        SpeciesIdentity::new(
            "gbif",
            identifier,
            scientific_name,
            format!("https://www.gbif.org/species/{identifier}"),
        )
        .expect("species")
    }

    fn category(value: &str) -> BirthCategory {
        BirthCategory::new(value).expect("category")
    }

    fn entry(species: SpeciesIdentity) -> ProvisionalOrganismBodyProfileEntry {
        ProvisionalOrganismBodyProfileEntry {
            species: species.clone(),
            initial_age_ticks: 20,
            metabolic_rate: MetabolicRateCommitment {
                commitment_schema_version: METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION,
                profile_set_digest: Digest::sha256(b"metabolic profile set"),
                observed_species: species.clone(),
                source_record_id: "test-metabolic-row".to_owned(),
                source_record_digest: Digest::sha256(b"metabolic row"),
                measured_power_value: 100,
                measured_power_decimal_places: 0,
            },
            physiological_regulation: PhysiologicalRegulationCommitment {
                commitment_schema_version: PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION,
                profile_id: "test-regulation-v1".to_owned(),
                profile_digest: Digest::sha256(b"regulation profile"),
                species: species.clone(),
                evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
                usable_energy_reserve_joules: 10_000,
                hydration_failure_seconds: 20_000,
                fatigue_failure_seconds: 30_000,
                fatigue_recovery_seconds: 10_000,
                thermoneutral_min_millicelsius: 18_000,
                thermoneutral_max_millicelsius: 26_000,
                thermal_failure_millicelsius_seconds: 40_000,
                thermal_recovery_seconds: 10_000,
            },
            reproductive_physiology: ReproductivePhysiologyCommitment {
                commitment_schema_version: REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION,
                profile_id: "test-reproduction-v1".to_owned(),
                profile_digest: Digest::sha256(b"reproduction profile"),
                species: species.clone(),
                evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
                tick_duration_seconds: 300,
                maturity_age_ticks: 10,
                development_ticks: 3,
                recovery_ticks: 4,
                opportunity_interval_ticks: 1,
                initiation_probability_millionths: REPRODUCTIVE_PROBABILITY_SCALE,
                compatible_pairs: vec![ReproductiveCategoryPair {
                    first: category("female"),
                    second: category("male"),
                    developing_parent: category("female"),
                }],
                offspring_categories: vec![
                    OffspringCategoryWeight {
                        category: category("female"),
                        weight: 1,
                    },
                    OffspringCategoryWeight {
                        category: category("male"),
                        weight: 1,
                    },
                ],
            },
            heritable_disposition_profile: Some(HeritableDispositionProfile {
                profile_schema_version: HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
                profile_id: "test-heritable-disposition-v1".to_owned(),
                profile_digest: Digest::sha256(b"heritable disposition profile"),
                species,
                evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
                minimum_action_weight: 4,
                neutral_action_weight: 16,
                maximum_action_weight: 28,
                founder_variation_steps: 3,
                mutation_probability_millionths: 100_000,
                mutation_max_step: 2,
            }),
        }
    }

    fn plan() -> ProvisionalOrganismBodyProfilePlan {
        ProvisionalOrganismBodyProfilePlan {
            plan_schema_version: PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION,
            status: PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_STATUS.to_owned(),
            tick_duration_seconds: 300,
            entries: vec![
                entry(species("2436436", "Homo sapiens")),
                entry(species("5219173", "Canis lupus")),
            ],
        }
    }

    #[test]
    fn canonical_plan_round_trips_and_supports_exact_species_lookup() {
        let plan = plan();
        let bytes = plan.canonical_bytes().expect("canonical plan");
        assert_eq!(
            ProvisionalOrganismBodyProfilePlan::from_canonical_slice(&bytes),
            Ok(plan.clone())
        );
        assert_eq!(
            plan.entry_for(&plan.entries[1].species),
            Some(&plan.entries[1])
        );

        let mut changed_identity = plan.entries[1].species.clone();
        changed_identity.scientific_name = "Canis familiaris".to_owned();
        assert_eq!(plan.entry_for(&changed_identity), None);
        assert!(
            String::from_utf8(bytes)
                .expect("UTF-8 JSON")
                .contains("\"status\":\"provisional-not-scientifically-admitted\"")
        );
    }

    #[test]
    fn noncanonical_json_is_rejected() {
        let plan = plan();
        let mut bytes = plan.canonical_bytes().expect("canonical plan");
        bytes.push(b'\n');
        assert_eq!(
            ProvisionalOrganismBodyProfilePlan::from_canonical_slice(&bytes),
            Err(ProvisionalOrganismBodyProfilePlanError::NonCanonicalEncoding)
        );
    }

    #[test]
    fn schema_one_without_heredity_remains_byte_canonical() {
        let mut plan = plan();
        plan.plan_schema_version = LEGACY_PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION;
        for entry in &mut plan.entries {
            entry.heritable_disposition_profile = None;
        }
        let bytes = plan.canonical_bytes().expect("legacy canonical plan");
        assert!(
            !String::from_utf8(bytes.clone())
                .expect("JSON")
                .contains("heritable_disposition_profile")
        );
        assert_eq!(
            ProvisionalOrganismBodyProfilePlan::from_canonical_slice(&bytes),
            Ok(plan)
        );
    }

    #[test]
    fn duplicate_and_out_of_order_species_are_rejected() {
        let mut duplicate = plan();
        duplicate.entries.push(duplicate.entries[1].clone());
        assert!(matches!(
            duplicate.validate(),
            Err(ProvisionalOrganismBodyProfilePlanError::DuplicateSpecies { .. })
        ));

        let mut out_of_order = plan();
        out_of_order.entries.swap(0, 1);
        assert_eq!(
            out_of_order.validate(),
            Err(ProvisionalOrganismBodyProfilePlanError::NonCanonicalSpeciesOrder)
        );
    }

    #[test]
    fn every_commitment_must_match_the_entry_species_exactly() {
        let mut plan = plan();
        plan.entries[0].physiological_regulation.species = species("5219173", "Canis lupus");
        assert_eq!(
            plan.validate(),
            Err(ProvisionalOrganismBodyProfilePlanError::SpeciesMismatch(
                "physiological_regulation"
            ))
        );
    }

    #[test]
    fn reproductive_commitment_must_use_the_plan_tick_duration() {
        let mut plan = plan();
        plan.entries[0]
            .reproductive_physiology
            .tick_duration_seconds = 301;
        assert_eq!(
            plan.validate(),
            Err(
                ProvisionalOrganismBodyProfilePlanError::ReproductiveTickDurationMismatch {
                    plan: 300,
                    commitment: 301,
                }
            )
        );
    }
}
