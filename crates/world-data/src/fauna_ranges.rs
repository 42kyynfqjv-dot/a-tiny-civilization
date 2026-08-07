//! Point-scoped, source-backed fauna range candidates.
//!
//! A candidate means only that a pinned iNaturalist modeled-range polygon contains
//! one exact geographic query point. It deliberately carries neither abundance nor
//! a decision to create an organism.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, GeographicCoordinateE7, S2CellId, SpeciesIdentity, WorldSeed};

use crate::ProvisionalOriginEnvironment;

pub const FAUNA_RANGE_CANDIDATE_SET_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_RANGE_CANDIDATE_SET_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-range-candidate-set+json";
const INATURALIST_RANGE_RELEASE: &str = "2.20";
pub const FAUNA_SEEDED_SELECTION_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_SEEDED_SELECTION_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.fauna-seeded-selection+json";
pub const FAUNA_POPULATION_PLAN_SCHEMA_VERSION: u16 = 1;
pub const FAUNA_POPULATION_PLAN_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.provisional-fauna-population-plan+json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaRangeCandidate {
    pub species: SpeciesIdentity,
    pub inaturalist_taxon_id: u64,
    pub range_package: String,
    pub range_feature_fid: u64,
}

/// Portable wire form for the exact WGS84 E7 point tested against source geometry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaRangeQueryPoint {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}

impl FaunaRangeQueryPoint {
    fn validate(self) -> Result<(), FaunaRangeCandidateSetError> {
        GeographicCoordinateE7::new(self.latitude_e7, self.longitude_e7)
            .map(|_| ())
            .map_err(|_| FaunaRangeCandidateSetError::InvalidQueryPoint)
    }
}

/// Exact modeled-range candidates at one source-query point.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaRangeCandidateSet {
    pub candidate_set_schema_version: u16,
    pub candidate_set_id: String,
    pub inaturalist_release: String,
    pub query_point: FaunaRangeQueryPoint,
    pub source_crosswalk_digest: Digest,
    pub source_gbif_catalog_digest: Digest,
    pub source_inaturalist_taxonomy_digest: Digest,
    pub candidates: Vec<FaunaRangeCandidate>,
}

/// A replay-stable subset of a local candidate pool. This still means only
/// "eligible for a later ecological decision"; it is not a population plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaSeededSelection {
    pub selection_schema_version: u16,
    pub candidate_set_digest: Digest,
    pub world_seed: WorldSeed,
    pub species_limit: u32,
    /// Ordered by the derived seed priority, never by an authored species list.
    pub selected_candidates: Vec<FaunaRangeCandidate>,
}

/// An explicitly planned initial count for a real selected species.
///
/// A count is an engine input, never a claim that a range source measured abundance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaPopulationPlanEntry {
    pub species: SpeciesIdentity,
    pub initial_individual_count: u32,
}

/// A non-admitted hand-off from an ecological planner to genesis.
///
/// It binds the later planner's decision to an exact local origin, candidate pool,
/// and seeded subset. It has no admitted state that could relabel a provisional
/// count as a measured population.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FaunaPopulationPlan {
    pub population_plan_schema_version: u16,
    pub status: String,
    pub world_seed: WorldSeed,
    pub origin_environment_digest: Digest,
    pub embodied_patch: S2CellId,
    pub candidate_set_digest: Digest,
    pub seeded_selection_digest: Digest,
    /// Strictly ordered by numeric GBIF species key.
    pub entries: Vec<FaunaPopulationPlanEntry>,
}

impl FaunaRangeCandidateSet {
    pub fn validate(&self) -> Result<(), FaunaRangeCandidateSetError> {
        if self.candidate_set_schema_version != FAUNA_RANGE_CANDIDATE_SET_SCHEMA_VERSION {
            return Err(FaunaRangeCandidateSetError::UnsupportedSchema(
                self.candidate_set_schema_version,
            ));
        }
        if !slug(&self.candidate_set_id) || self.inaturalist_release != INATURALIST_RANGE_RELEASE {
            return Err(FaunaRangeCandidateSetError::InvalidIdentity);
        }
        self.query_point.validate()?;
        if self.source_crosswalk_digest == Digest::ZERO
            || self.source_gbif_catalog_digest == Digest::ZERO
            || self.source_inaturalist_taxonomy_digest == Digest::ZERO
        {
            return Err(FaunaRangeCandidateSetError::ZeroDigest);
        }
        let mut previous_key = None;
        for candidate in &self.candidates {
            candidate.validate()?;
            let key = candidate.order_key()?;
            if previous_key.is_some_and(|previous| previous >= key) {
                return Err(FaunaRangeCandidateSetError::NonCanonicalCandidateOrder);
            }
            previous_key = Some(key);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, FaunaRangeCandidateSetError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaRangeCandidateSetError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, FaunaRangeCandidateSetError> {
        let set: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaRangeCandidateSetError::Decode(error.to_string()))?;
        set.validate()?;
        if set.canonical_bytes()? != bytes {
            return Err(FaunaRangeCandidateSetError::NonCanonicalEncoding);
        }
        Ok(set)
    }

    /// Select a bounded taxon pool by a domain-separated digest of the committed
    /// world seed and each source-backed candidate. This is deterministic across
    /// hosts and independent of candidate input ordering after validation.
    pub fn select_seeded_candidates(
        &self,
        world_seed: WorldSeed,
        species_limit: u32,
    ) -> Result<FaunaSeededSelection, FaunaRangeCandidateSetError> {
        if species_limit == 0 {
            return Err(FaunaRangeCandidateSetError::ZeroSpeciesLimit);
        }
        let candidate_set_digest = Digest::sha256(&self.canonical_bytes()?);
        let mut ranked = self
            .candidates
            .iter()
            .cloned()
            .map(|candidate| {
                let mut bytes = Vec::with_capacity(128);
                bytes.extend_from_slice(b"a-tiny-civilization/fauna-seed-selection/v1");
                bytes.extend_from_slice(&world_seed.get().to_le_bytes());
                bytes.extend_from_slice(candidate_set_digest.as_bytes());
                bytes.extend_from_slice(&candidate.order_key()?.to_le_bytes());
                bytes.extend_from_slice(&candidate.inaturalist_taxon_id.to_le_bytes());
                bytes.extend_from_slice(&candidate.range_feature_fid.to_le_bytes());
                Ok::<_, FaunaRangeCandidateSetError>((
                    Digest::sha256(&bytes),
                    candidate.order_key()?,
                    candidate,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        ranked.sort_unstable_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        let limit = usize::try_from(species_limit)
            .map_err(|_| FaunaRangeCandidateSetError::InvalidSeededSelection)?;
        let selected_candidates = ranked
            .into_iter()
            .take(limit)
            .map(|(_, _, candidate)| candidate)
            .collect();
        Ok(FaunaSeededSelection {
            selection_schema_version: FAUNA_SEEDED_SELECTION_SCHEMA_VERSION,
            candidate_set_digest,
            world_seed,
            species_limit,
            selected_candidates,
        })
    }
}

impl FaunaSeededSelection {
    pub fn validate_against(
        &self,
        candidates: &FaunaRangeCandidateSet,
    ) -> Result<(), FaunaRangeCandidateSetError> {
        if self.selection_schema_version != FAUNA_SEEDED_SELECTION_SCHEMA_VERSION {
            return Err(FaunaRangeCandidateSetError::UnsupportedSelectionSchema(
                self.selection_schema_version,
            ));
        }
        if self.species_limit == 0 || self.candidate_set_digest == Digest::ZERO {
            return Err(FaunaRangeCandidateSetError::InvalidSeededSelection);
        }
        let expected = candidates.select_seeded_candidates(self.world_seed, self.species_limit)?;
        if self != &expected {
            return Err(FaunaRangeCandidateSetError::InvalidSeededSelection);
        }
        Ok(())
    }

    pub fn canonical_bytes_against(
        &self,
        candidates: &FaunaRangeCandidateSet,
    ) -> Result<Vec<u8>, FaunaRangeCandidateSetError> {
        self.validate_against(candidates)?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaRangeCandidateSetError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice_against(
        bytes: &[u8],
        candidates: &FaunaRangeCandidateSet,
    ) -> Result<Self, FaunaRangeCandidateSetError> {
        let selection: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaRangeCandidateSetError::Decode(error.to_string()))?;
        if selection.canonical_bytes_against(candidates)? != bytes {
            return Err(FaunaRangeCandidateSetError::NonCanonicalEncoding);
        }
        Ok(selection)
    }
}

impl FaunaPopulationPlan {
    pub fn validate_against(
        &self,
        candidates: &FaunaRangeCandidateSet,
        selection: &FaunaSeededSelection,
    ) -> Result<(), FaunaRangeCandidateSetError> {
        if self.population_plan_schema_version != FAUNA_POPULATION_PLAN_SCHEMA_VERSION {
            return Err(
                FaunaRangeCandidateSetError::UnsupportedPopulationPlanSchema(
                    self.population_plan_schema_version,
                ),
            );
        }
        if self.status != "provisional-not-scientifically-admitted"
            || self.origin_environment_digest == Digest::ZERO
            || self.candidate_set_digest == Digest::ZERO
            || self.seeded_selection_digest == Digest::ZERO
            || self.entries.is_empty()
        {
            return Err(FaunaRangeCandidateSetError::InvalidPopulationPlan);
        }
        selection.validate_against(candidates)?;
        if self.world_seed != selection.world_seed
            || self.candidate_set_digest != selection.candidate_set_digest
            || self.seeded_selection_digest
                != Digest::sha256(&selection.canonical_bytes_against(candidates)?)
        {
            return Err(FaunaRangeCandidateSetError::InvalidPopulationPlan);
        }
        let mut previous_key = None;
        for entry in &self.entries {
            entry
                .species
                .validate()
                .map_err(|error| FaunaRangeCandidateSetError::InvalidSpecies(error.to_string()))?;
            let key = entry
                .species
                .identifier
                .parse::<u64>()
                .map_err(|_| FaunaRangeCandidateSetError::InvalidPopulationPlan)?;
            if entry.species.catalog != "gbif"
                || key == 0
                || entry.initial_individual_count == 0
                || !selection
                    .selected_candidates
                    .iter()
                    .any(|candidate| candidate.species == entry.species)
                || previous_key.is_some_and(|previous| previous >= key)
            {
                return Err(FaunaRangeCandidateSetError::InvalidPopulationPlan);
            }
            previous_key = Some(key);
        }
        Ok(())
    }

    pub fn canonical_bytes_against(
        &self,
        candidates: &FaunaRangeCandidateSet,
        selection: &FaunaSeededSelection,
    ) -> Result<Vec<u8>, FaunaRangeCandidateSetError> {
        self.validate_against(candidates, selection)?;
        serde_json::to_vec(self)
            .map_err(|error| FaunaRangeCandidateSetError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice_against(
        bytes: &[u8],
        candidates: &FaunaRangeCandidateSet,
        selection: &FaunaSeededSelection,
    ) -> Result<Self, FaunaRangeCandidateSetError> {
        let plan: Self = serde_json::from_slice(bytes)
            .map_err(|error| FaunaRangeCandidateSetError::Decode(error.to_string()))?;
        if plan.canonical_bytes_against(candidates, selection)? != bytes {
            return Err(FaunaRangeCandidateSetError::NonCanonicalEncoding);
        }
        Ok(plan)
    }

    pub fn validate_against_environment(
        &self,
        candidates: &FaunaRangeCandidateSet,
        selection: &FaunaSeededSelection,
        environment: &ProvisionalOriginEnvironment,
    ) -> Result<(), FaunaRangeCandidateSetError> {
        self.validate_against(candidates, selection)?;
        let environment_bytes = environment
            .canonical_bytes()
            .map_err(|_| FaunaRangeCandidateSetError::InvalidPopulationPlan)?;
        if self.origin_environment_digest != Digest::sha256(&environment_bytes)
            || self.embodied_patch != environment.selected_embodied_patch
        {
            return Err(FaunaRangeCandidateSetError::InvalidPopulationPlan);
        }
        Ok(())
    }

    pub fn from_canonical_slice_against_environment(
        bytes: &[u8],
        candidates: &FaunaRangeCandidateSet,
        selection: &FaunaSeededSelection,
        environment: &ProvisionalOriginEnvironment,
    ) -> Result<Self, FaunaRangeCandidateSetError> {
        let plan = Self::from_canonical_slice_against(bytes, candidates, selection)?;
        plan.validate_against_environment(candidates, selection, environment)?;
        Ok(plan)
    }
}

impl FaunaRangeCandidate {
    fn validate(&self) -> Result<(), FaunaRangeCandidateSetError> {
        self.species
            .validate()
            .map_err(|error| FaunaRangeCandidateSetError::InvalidSpecies(error.to_string()))?;
        if self.species.catalog != "gbif"
            || self
                .species
                .identifier
                .parse::<u64>()
                .ok()
                .filter(|key| *key > 0)
                .is_none()
            || self.inaturalist_taxon_id == 0
            || self.range_feature_fid == 0
            || !package_identifier(&self.range_package)
        {
            return Err(FaunaRangeCandidateSetError::InvalidCandidate);
        }
        Ok(())
    }

    fn order_key(&self) -> Result<u64, FaunaRangeCandidateSetError> {
        self.species
            .identifier
            .parse()
            .map_err(|_| FaunaRangeCandidateSetError::InvalidCandidate)
    }
}

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn package_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FaunaRangeCandidateSetError {
    #[error("unsupported fauna range candidate-set schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid fauna range candidate-set identity")]
    InvalidIdentity,
    #[error("fauna range candidate-set provenance digest must not be zero")]
    ZeroDigest,
    #[error("invalid real species identity: {0}")]
    InvalidSpecies(String),
    #[error("invalid source-backed fauna range candidate")]
    InvalidCandidate,
    #[error("invalid fauna range query point")]
    InvalidQueryPoint,
    #[error("fauna range candidates must be strictly ordered by numeric GBIF taxon key")]
    NonCanonicalCandidateOrder,
    #[error("fauna seeded selection has an unsupported schema {0}")]
    UnsupportedSelectionSchema(u16),
    #[error("fauna seeded selection needs a nonzero species limit")]
    ZeroSpeciesLimit,
    #[error(
        "fauna seeded selection does not exactly follow its source candidate set and world seed"
    )]
    InvalidSeededSelection,
    #[error("fauna population plan has an unsupported schema {0}")]
    UnsupportedPopulationPlanSchema(u16),
    #[error("fauna population plan is not a canonical, source-bound provisional plan")]
    InvalidPopulationPlan,
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

    fn candidate(key: u64, name: &str) -> FaunaRangeCandidate {
        FaunaRangeCandidate {
            species: SpeciesIdentity::new(
                "gbif",
                key.to_string(),
                name,
                format!("https://www.gbif.org/species/{key}"),
            )
            .expect("valid source species"),
            inaturalist_taxon_id: key + 1,
            range_package: "mammalia".to_owned(),
            range_feature_fid: key + 2,
        }
    }

    fn set() -> FaunaRangeCandidateSet {
        FaunaRangeCandidateSet {
            candidate_set_schema_version: FAUNA_RANGE_CANDIDATE_SET_SCHEMA_VERSION,
            candidate_set_id: "inaturalist-v2-20-point-candidates".to_owned(),
            inaturalist_release: INATURALIST_RANGE_RELEASE.to_owned(),
            query_point: FaunaRangeQueryPoint {
                latitude_e7: 446_000_000,
                longitude_e7: -1_105_000_000,
            },
            source_crosswalk_digest: Digest::sha256(b"crosswalk"),
            source_gbif_catalog_digest: Digest::sha256(b"gbif"),
            source_inaturalist_taxonomy_digest: Digest::sha256(b"taxonomy"),
            candidates: vec![
                candidate(12, "Canis lupus"),
                candidate(2441176, "Bison bison"),
            ],
        }
    }

    #[test]
    fn candidate_set_round_trips_only_in_canonical_order() {
        let candidate_set = set();
        let bytes = candidate_set
            .canonical_bytes()
            .expect("canonical candidate set");
        assert_eq!(
            FaunaRangeCandidateSet::from_canonical_slice(&bytes),
            Ok(candidate_set)
        );

        let mut unordered = set();
        unordered.candidates.reverse();
        assert_eq!(
            unordered.validate(),
            Err(FaunaRangeCandidateSetError::NonCanonicalCandidateOrder)
        );
    }

    #[test]
    fn candidate_set_rejects_an_abundance_like_or_unpinned_record() {
        let mut invalid = set();
        invalid.candidates[0].range_feature_fid = 0;
        assert_eq!(
            invalid.validate(),
            Err(FaunaRangeCandidateSetError::InvalidCandidate)
        );
        let mut noncanonical = set().canonical_bytes().expect("canonical bytes");
        noncanonical.push(b'\n');
        assert_eq!(
            FaunaRangeCandidateSet::from_canonical_slice(&noncanonical),
            Err(FaunaRangeCandidateSetError::NonCanonicalEncoding)
        );
    }

    #[test]
    fn external_point_query_wire_shape_is_canonical() {
        let bytes = br#"{"candidate_set_schema_version":1,"candidate_set_id":"inaturalist-v2-20-point-n446000000-w110500000","inaturalist_release":"2.20","query_point":{"latitude_e7":446000000,"longitude_e7":-110500000},"source_crosswalk_digest":"e922f1507760d7156b740382133bb5924a8561a183262db4bfd011c41c144ee0","source_gbif_catalog_digest":"b0597d47bc616b8ed2c18e7ba625a460538e9bac4bbae920f3f016095b966fa0","source_inaturalist_taxonomy_digest":"78c2cbab7a045c2ef299ef481552fb0d9c3c021ab8fc4851ead71854670cb297","candidates":[]}"#;
        let decoded: FaunaRangeCandidateSet =
            serde_json::from_slice(bytes).expect("decode wire shape");
        assert_eq!(decoded.canonical_bytes().expect("canonical bytes"), bytes);
    }

    #[test]
    fn seeded_selection_is_replay_stable_and_canonical() {
        let candidate_set = set();
        let world_seed = WorldSeed::new(42);
        let selection = candidate_set
            .select_seeded_candidates(world_seed, 1)
            .expect("seeded selection");
        assert_eq!(
            candidate_set.select_seeded_candidates(world_seed, 1),
            Ok(selection.clone())
        );
        assert_eq!(selection.selected_candidates.len(), 1);
        assert!(
            candidate_set
                .candidates
                .contains(&selection.selected_candidates[0])
        );
        let bytes = selection
            .canonical_bytes_against(&candidate_set)
            .expect("canonical selection");
        assert_eq!(
            FaunaSeededSelection::from_canonical_slice_against(&bytes, &candidate_set),
            Ok(selection)
        );
    }

    #[test]
    fn seeded_selection_rejects_zero_limit_and_tampering() {
        let candidate_set = set();
        assert_eq!(
            candidate_set.select_seeded_candidates(WorldSeed::new(42), 0),
            Err(FaunaRangeCandidateSetError::ZeroSpeciesLimit)
        );
        let mut selection = candidate_set
            .select_seeded_candidates(WorldSeed::new(42), 1)
            .expect("seeded selection");
        selection.selected_candidates = candidate_set.candidates.clone();
        assert_eq!(
            selection.validate_against(&candidate_set),
            Err(FaunaRangeCandidateSetError::InvalidSeededSelection)
        );
    }

    #[test]
    fn population_plan_binds_counts_to_selected_species_and_origin_evidence() {
        let candidates = set();
        let selection = candidates
            .select_seeded_candidates(WorldSeed::new(42), 1)
            .expect("selection");
        let plan = FaunaPopulationPlan {
            population_plan_schema_version: FAUNA_POPULATION_PLAN_SCHEMA_VERSION,
            status: "provisional-not-scientifically-admitted".to_owned(),
            world_seed: WorldSeed::new(42),
            origin_environment_digest: Digest::sha256(b"environment"),
            embodied_patch: "1000000000000001".parse().expect("valid L30 patch"),
            candidate_set_digest: selection.candidate_set_digest,
            seeded_selection_digest: Digest::sha256(
                &selection
                    .canonical_bytes_against(&candidates)
                    .expect("selection bytes"),
            ),
            entries: vec![FaunaPopulationPlanEntry {
                species: selection.selected_candidates[0].species.clone(),
                initial_individual_count: 2,
            }],
        };
        let bytes = plan
            .canonical_bytes_against(&candidates, &selection)
            .expect("canonical plan");
        assert_eq!(
            FaunaPopulationPlan::from_canonical_slice_against(&bytes, &candidates, &selection),
            Ok(plan.clone())
        );
        let mut invalid = plan;
        invalid.entries[0].species = candidate(999, "Other species").species;
        assert_eq!(
            invalid.validate_against(&candidates, &selection),
            Err(FaunaRangeCandidateSetError::InvalidPopulationPlan)
        );
    }
}
