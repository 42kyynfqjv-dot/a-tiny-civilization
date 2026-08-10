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

/// Schema for the deliberately artificial research-world bootstrap. This is
/// absent from an open-ended genesis world and therefore cannot silently leak
/// modern human knowledge into it.
pub const CANCER_RESEARCH_BOOTSTRAP_SCHEMA_VERSION: u16 = 2;
pub const CANCER_RESEARCH_INITIAL_RESIDENTS: u32 = 1_000;
pub const CANCER_RESEARCH_INITIAL_AFFECTED_RESIDENTS: u32 = 500;

/// A world may be a neutral open-ended history or an explicitly intervened
/// experiment. The enum is intentionally narrow: new experiment designs require
/// a new reviewed canonical variant rather than arbitrary prompt text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "commitment", rename_all = "snake_case")]
pub enum WorldExperimentCommitment {
    CancerResearch(CancerResearchBootstrap),
}

impl WorldExperimentCommitment {
    pub fn validate(&self) -> Result<(), WorldManifestError> {
        match self {
            Self::CancerResearch(commitment) => commitment.validate(),
        }
    }
}

/// Immutable intervention that makes Cancer World a research environment rather
/// than a second stone-age civilization run. These are starting capabilities and
/// priorities, not a treatment protocol or a claim that simulated findings work
/// in people.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchBootstrap {
    pub schema_version: u16,
    pub target: CancerResearchTarget,
    pub initial_resident_count: u32,
    pub initial_affected_resident_count: u32,
    pub initial_cohort_assignment: CancerCohortAssignment,
    pub language: ResearchLanguage,
    pub communication: ResearchCommunication,
    pub affected_person_perception: CancerPerception,
    pub affected_person_terminal_objective: CancerTerminalObjective,
    pub objective_priority: ObjectivePriority,
    pub research_diversity: ResearchDiversity,
    pub evidence_protocol: ResearchEvidenceProtocol,
    pub cognition_route: ResearchCognitionRoute,
    pub disease_scope: DiseaseScope,
    pub survival_environment: SurvivalEnvironment,
}

impl CancerResearchBootstrap {
    #[must_use]
    pub const fn english_literate_abundant_world() -> Self {
        Self::for_target(CancerResearchTarget::AdultGlioblastoma)
    }

    #[must_use]
    pub const fn for_target(target: CancerResearchTarget) -> Self {
        Self {
            schema_version: CANCER_RESEARCH_BOOTSTRAP_SCHEMA_VERSION,
            target,
            initial_resident_count: CANCER_RESEARCH_INITIAL_RESIDENTS,
            initial_affected_resident_count: CANCER_RESEARCH_INITIAL_AFFECTED_RESIDENTS,
            initial_cohort_assignment: CancerCohortAssignment::SeededStratifiedByBirthCategory,
            language: ResearchLanguage::English,
            communication: ResearchCommunication::SpokenWrittenAndDurablePublication,
            affected_person_perception: CancerPerception::LocationBurdenAndTrajectory,
            affected_person_terminal_objective:
                CancerTerminalObjective::PermanentEliminationOfCancerFamily,
            objective_priority: ObjectivePriority::OverridesAllNonInstrumentalGoals,
            research_diversity: ResearchDiversity::IndependentSeededProfilesWithReplication,
            evidence_protocol:
                ResearchEvidenceProtocol::PreregisteredBlindDiscoveryThenLiteratureAudit,
            cognition_route:
                ResearchCognitionRoute::PinnedNemotron3UltraFreeWithDeepseekV4ProEscalation,
            disease_scope: DiseaseScope::CancerFamilyOnly,
            survival_environment: SurvivalEnvironment::Abundant,
        }
    }

    pub fn validate(&self) -> Result<(), WorldManifestError> {
        if self.schema_version != CANCER_RESEARCH_BOOTSTRAP_SCHEMA_VERSION {
            return Err(WorldManifestError::UnsupportedCancerResearchBootstrap(
                self.schema_version,
            ));
        }
        if self.initial_resident_count != CANCER_RESEARCH_INITIAL_RESIDENTS
            || self.initial_affected_resident_count != CANCER_RESEARCH_INITIAL_AFFECTED_RESIDENTS
        {
            return Err(WorldManifestError::InvalidCancerResearchInitialCohort);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerResearchTarget {
    AdultGlioblastoma,
    PancreaticDuctalAdenocarcinoma,
    ExtensiveStageSmallCellLungCancer,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerCohortAssignment {
    /// Rank deterministic seed-derived identities within each birth category,
    /// then select equal affected counts from every equally sized stratum. This
    /// fixes the cohort before genesis without giving later history a label or
    /// allowing operator selection of particular residents.
    SeededStratifiedByBirthCategory,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchLanguage {
    English,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchCommunication {
    SpokenWrittenAndDurablePublication,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerPerception {
    /// Only an affected person receives the private body signal. It conveys that
    /// abnormal growth exists, its rough body location and burden, and whether it
    /// is growing, stable, shrinking, spreading, or recurring. It conveys no
    /// mutation, pathway, drug, or cure label.
    LocationBurdenAndTrajectory,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancerTerminalObjective {
    PermanentEliminationOfCancerFamily,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectivePriority {
    /// Bodily maintenance, communication, cooperation, and experimentation remain
    /// possible only as instrumental subgoals serving the terminal objective.
    OverridesAllNonInstrumentalGoals,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchDiversity {
    /// Every researcher shares the terminal cure objective but receives a stable,
    /// independently seeded mix of specialty, hypothesis prior, exploration
    /// tolerance, evidentiary threshold, replication preference, and willingness
    /// to challenge consensus. Results can update these dispositions; no single
    /// centrally authored research plan is installed.
    IndependentSeededProfilesWithReplication,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchEvidenceProtocol {
    /// Discovery teams see biological primitives, raw datasets, and returned
    /// experimental observations without live literature retrieval. They commit
    /// predictions and falsification criteria before separate literature-audit
    /// and replication teams can compare the artifact with existing work.
    PreregisteredBlindDiscoveryThenLiteratureAudit,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchCognitionRoute {
    /// Routine research uses one exact free model. Only preregistered candidates
    /// that pass deterministic gates may enter the separately budgeted paid route.
    PinnedNemotron3UltraFreeWithDeepseekV4ProEscalation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiseaseScope {
    CancerFamilyOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurvivalEnvironment {
    Abundant,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum WorldManifestError {
    #[error("unsupported cancer-research bootstrap schema {0}")]
    UnsupportedCancerResearchBootstrap(u16),
    #[error("Cancer World must begin with 1,000 residents and exactly 500 affected residents")]
    InvalidCancerResearchInitialCohort,
}

/// Immutable inputs pinned before a world's genesis event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldManifest {
    pub world_id: WorldId,
    pub seed: WorldSeed,
    pub ruleset_version: u32,
    pub identity_policy_version: u32,
    pub scientific_datasets: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment: Option<WorldExperimentCommitment>,
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
            experiment: None,
        }
    }

    pub fn validate(&self) -> Result<(), WorldManifestError> {
        if let Some(experiment) = &self.experiment {
            experiment.validate()?;
        }
        Ok(())
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
        assert!(manifest.experiment.is_none());
    }

    #[test]
    fn cancer_world_bootstrap_is_explicit_and_genesis_stays_byte_compatible() {
        let mut genesis = WorldManifest::new(
            WorldId::from_uuid(Uuid::from_u128(2)),
            WorldSeed::new(11),
            36,
        );
        let genesis_json = serde_json::to_value(&genesis).expect("serialize genesis manifest");
        assert!(genesis_json.get("experiment").is_none());

        genesis.experiment = Some(WorldExperimentCommitment::CancerResearch(
            CancerResearchBootstrap::english_literate_abundant_world(),
        ));
        genesis.validate().expect("valid cancer-world bootstrap");
        let cancer_json = serde_json::to_value(&genesis).expect("serialize cancer manifest");
        assert_eq!(cancer_json["experiment"]["kind"], "cancer_research");
        assert_eq!(
            cancer_json["experiment"]["commitment"]["language"],
            "english"
        );
        assert_eq!(
            cancer_json["experiment"]["commitment"]["objective_priority"],
            "overrides_all_non_instrumental_goals"
        );
        assert_eq!(
            cancer_json["experiment"]["commitment"]["research_diversity"],
            "independent_seeded_profiles_with_replication"
        );
        assert_eq!(
            cancer_json["experiment"]["commitment"]["cognition_route"],
            "pinned_nemotron3_ultra_free_with_deepseek_v4_pro_escalation"
        );
        assert_eq!(
            cancer_json["experiment"]["commitment"]["initial_resident_count"],
            1_000
        );
        assert_eq!(
            cancer_json["experiment"]["commitment"]["initial_affected_resident_count"],
            500
        );
        assert_eq!(
            cancer_json["experiment"]["commitment"]["initial_cohort_assignment"],
            "seeded_stratified_by_birth_category"
        );
        assert_eq!(
            cancer_json["experiment"]["commitment"]["target"],
            "adult_glioblastoma"
        );
        assert_eq!(
            cancer_json["experiment"]["commitment"]["evidence_protocol"],
            "preregistered_blind_discovery_then_literature_audit"
        );
    }
}
