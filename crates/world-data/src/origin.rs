//! Deterministic, source-auditable provisional origin selection.
//!
//! This chooses from source-confirmed land patches using the committed world seed;
//! it deliberately makes no habitat-suitability or population claim.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, S2CellId, WorldSeed};

pub const PROVISIONAL_LAND_ORIGIN_SELECTION_SCHEMA_VERSION: u16 = 1;
pub const PROVISIONAL_LAND_ORIGIN_SELECTION_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.provisional-land-origin-selection+json";
const SELECTION_POLICY: &str = "digest-rank-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProvisionalLandOriginSelection {
    pub selection_schema_version: u16,
    pub selection_policy: String,
    pub world_seed: WorldSeed,
    pub land_reference_root_digest: Digest,
    pub eligible_patch_count: u64,
    pub selected_patch: S2CellId,
    pub selected_rank: Digest,
    pub embodied_patch_level: u8,
    pub selected_embodied_patch: S2CellId,
}

impl ProvisionalLandOriginSelection {
    pub fn select(
        world_seed: WorldSeed,
        land_reference_root_digest: Digest,
        eligible_patches: impl IntoIterator<Item = S2CellId>,
        embodied_patch_level: u8,
    ) -> Result<Self, ProvisionalLandOriginSelectionError> {
        if land_reference_root_digest == Digest::ZERO {
            return Err(ProvisionalLandOriginSelectionError::ZeroDigest);
        }
        let mut previous = None;
        let mut eligible_patch_count = 0_u64;
        let mut selected = None;
        for patch in eligible_patches {
            if previous.is_some_and(|last| last >= patch) {
                return Err(ProvisionalLandOriginSelectionError::NonCanonicalEligiblePatches);
            }
            previous = Some(patch);
            eligible_patch_count = eligible_patch_count
                .checked_add(1)
                .ok_or(ProvisionalLandOriginSelectionError::EligibleCountOverflow)?;
            let rank = rank(world_seed, land_reference_root_digest, patch);
            if selected
                .as_ref()
                .is_none_or(|(best, best_patch)| (rank, patch) < (*best, *best_patch))
            {
                selected = Some((rank, patch));
            }
        }
        let (selected_rank, selected_patch) =
            selected.ok_or(ProvisionalLandOriginSelectionError::NoEligiblePatches)?;
        if embodied_patch_level < selected_patch.level() || embodied_patch_level > 30 {
            return Err(ProvisionalLandOriginSelectionError::InvalidEmbodiedPatchLevel);
        }
        Ok(Self {
            selection_schema_version: PROVISIONAL_LAND_ORIGIN_SELECTION_SCHEMA_VERSION,
            selection_policy: SELECTION_POLICY.to_owned(),
            world_seed,
            land_reference_root_digest,
            eligible_patch_count,
            selected_patch,
            selected_rank,
            embodied_patch_level,
            selected_embodied_patch: derive_embodied_patch(
                world_seed,
                land_reference_root_digest,
                selected_patch,
                embodied_patch_level,
            )?,
        })
    }

    pub fn validate(&self) -> Result<(), ProvisionalLandOriginSelectionError> {
        if self.selection_schema_version != PROVISIONAL_LAND_ORIGIN_SELECTION_SCHEMA_VERSION {
            return Err(ProvisionalLandOriginSelectionError::UnsupportedSchema(
                self.selection_schema_version,
            ));
        }
        if self.selection_policy != SELECTION_POLICY
            || self.land_reference_root_digest == Digest::ZERO
        {
            return Err(ProvisionalLandOriginSelectionError::InvalidSelection);
        }
        if self.eligible_patch_count == 0 || self.selected_rank == Digest::ZERO {
            return Err(ProvisionalLandOriginSelectionError::InvalidSelection);
        }
        if self.embodied_patch_level < self.selected_patch.level()
            || self.embodied_patch_level > 30
            || self.selected_embodied_patch.level() != self.embodied_patch_level
            || self
                .selected_embodied_patch
                .ancestor(self.selected_patch.level())
                .map_err(|_| ProvisionalLandOriginSelectionError::InvalidSelection)?
                != self.selected_patch
            || self.selected_embodied_patch
                != derive_embodied_patch(
                    self.world_seed,
                    self.land_reference_root_digest,
                    self.selected_patch,
                    self.embodied_patch_level,
                )?
        {
            return Err(ProvisionalLandOriginSelectionError::InvalidSelection);
        }
        if self.selected_rank
            != rank(
                self.world_seed,
                self.land_reference_root_digest,
                self.selected_patch,
            )
        {
            return Err(ProvisionalLandOriginSelectionError::InvalidSelection);
        }
        Ok(())
    }

    pub fn validate_against(
        &self,
        eligible_patches: impl IntoIterator<Item = S2CellId>,
    ) -> Result<(), ProvisionalLandOriginSelectionError> {
        self.validate()?;
        if Self::select(
            self.world_seed,
            self.land_reference_root_digest,
            eligible_patches,
            self.embodied_patch_level,
        )? != *self
        {
            return Err(ProvisionalLandOriginSelectionError::InvalidSelection);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProvisionalLandOriginSelectionError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| ProvisionalLandOriginSelectionError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, ProvisionalLandOriginSelectionError> {
        let selection: Self = serde_json::from_slice(bytes)
            .map_err(|error| ProvisionalLandOriginSelectionError::Decode(error.to_string()))?;
        if selection.canonical_bytes()? != bytes {
            return Err(ProvisionalLandOriginSelectionError::NonCanonicalEncoding);
        }
        Ok(selection)
    }
}

fn rank(world_seed: WorldSeed, root_digest: Digest, patch: S2CellId) -> Digest {
    let mut bytes = Vec::with_capacity(96);
    bytes.extend_from_slice(b"a-tiny-civilization/provisional-land-origin/v1");
    bytes.extend_from_slice(&world_seed.get().to_le_bytes());
    bytes.extend_from_slice(root_digest.as_bytes());
    bytes.extend_from_slice(&patch.get().to_be_bytes());
    Digest::sha256(&bytes)
}

fn derive_embodied_patch(
    world_seed: WorldSeed,
    root_digest: Digest,
    selected_patch: S2CellId,
    target_level: u8,
) -> Result<S2CellId, ProvisionalLandOriginSelectionError> {
    let mut patch = selected_patch;
    while patch.level() < target_level {
        let mut bytes = Vec::with_capacity(96);
        bytes.extend_from_slice(b"a-tiny-civilization/provisional-land-origin-child/v1");
        bytes.extend_from_slice(&world_seed.get().to_le_bytes());
        bytes.extend_from_slice(root_digest.as_bytes());
        bytes.extend_from_slice(&patch.get().to_be_bytes());
        bytes.push(patch.level());
        let child_index = usize::from(Digest::sha256(&bytes).as_bytes()[0] & 3);
        patch = patch
            .children()
            .map_err(|_| ProvisionalLandOriginSelectionError::InvalidEmbodiedPatchLevel)?
            [child_index];
    }
    Ok(patch)
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProvisionalLandOriginSelectionError {
    #[error("unsupported provisional land-origin schema {0}")]
    UnsupportedSchema(u16),
    #[error("provisional land-origin selection requires a nonzero source digest")]
    ZeroDigest,
    #[error("eligible land patches must be strictly canonical S2 order")]
    NonCanonicalEligiblePatches,
    #[error("eligible land patch count overflow")]
    EligibleCountOverflow,
    #[error("the source supplied no eligible land patches")]
    NoEligiblePatches,
    #[error("invalid provisional embodied-patch level")]
    InvalidEmbodiedPatchLevel,
    #[error("invalid provisional land-origin selection")]
    InvalidSelection,
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

    fn patches() -> Vec<S2CellId> {
        vec![
            S2CellId::new(1_u64 << 60).expect("root"),
            S2CellId::new((1_u64 << 61) | (1_u64 << 60)).expect("root"),
        ]
    }

    #[test]
    fn selection_is_replay_stable_and_canonical() {
        let selection = ProvisionalLandOriginSelection::select(
            WorldSeed::new(42),
            Digest::sha256(b"land"),
            patches(),
            4,
        )
        .expect("selection");
        selection
            .validate_against(patches())
            .expect("verified selection");
        let bytes = selection.canonical_bytes().expect("canonical bytes");
        assert_eq!(
            ProvisionalLandOriginSelection::from_canonical_slice(&bytes),
            Ok(selection)
        );
    }

    #[test]
    fn selection_rejects_reordered_or_tampered_inputs() {
        let mut unordered = patches();
        unordered.reverse();
        assert_eq!(
            ProvisionalLandOriginSelection::select(
                WorldSeed::new(42),
                Digest::sha256(b"land"),
                unordered,
                4,
            ),
            Err(ProvisionalLandOriginSelectionError::NonCanonicalEligiblePatches)
        );
        let mut selection = ProvisionalLandOriginSelection::select(
            WorldSeed::new(42),
            Digest::sha256(b"land"),
            patches(),
            4,
        )
        .expect("selection");
        selection.eligible_patch_count = 3;
        assert_eq!(
            selection.validate_against(patches()),
            Err(ProvisionalLandOriginSelectionError::InvalidSelection)
        );
    }
}
