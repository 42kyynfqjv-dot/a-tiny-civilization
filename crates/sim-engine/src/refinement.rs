//! Private exact conservation proof for initial L10-to-L14 ecology refinement.
//!
//! This module allocates generic sourced extensive quantities. It deliberately does
//! not invent ecological values, persist refined state, or expose a canonical API.

use thiserror::Error;
use world_domain::{Digest, S2CellId, S2CellIdError, WorldSeed};

const PLANETARY_LEVEL: u8 = 10;
const REGIONAL_LEVEL: u8 = 14;
const REGIONAL_CHILD_COUNT: usize = 1 << (2 * (REGIONAL_LEVEL - PLANETARY_LEVEL));
const RESIDUAL_STREAM_DOMAIN: &[u8] = b"a-tiny-civilization/refinement-residual/v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct QuantityCode(u16);

impl QuantityCode {
    pub(super) fn new(value: u16) -> Result<Self, RefinementError> {
        if value == 0 {
            return Err(RefinementError::ZeroQuantityCode);
        }
        Ok(Self(value))
    }

    const fn get(self) -> u16 {
        self.0
    }
}

/// Immutable stream inputs for resolving otherwise equal integer residuals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RefinementContext {
    world_seed: WorldSeed,
    world_data_bundle_digest: Digest,
    policy_version: u16,
    process_code: u16,
    refinement_generation: u64,
}

impl RefinementContext {
    pub(super) fn new(
        world_seed: WorldSeed,
        world_data_bundle_digest: Digest,
        policy_version: u16,
        process_code: u16,
        refinement_generation: u64,
    ) -> Result<Self, RefinementError> {
        if world_data_bundle_digest == Digest::ZERO {
            return Err(RefinementError::ZeroWorldDataBundleDigest);
        }
        if policy_version == 0 {
            return Err(RefinementError::ZeroPolicyVersion);
        }
        if process_code == 0 {
            return Err(RefinementError::ZeroProcessCode);
        }
        Ok(Self {
            world_seed,
            world_data_bundle_digest,
            policy_version,
            process_code,
            refinement_generation,
        })
    }
}

/// One evidence-derived relative weight for a required L14 descendant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ChildEvidence {
    cell: S2CellId,
    weight: u64,
}

impl ChildEvidence {
    #[must_use]
    pub(super) const fn new(cell: S2CellId, weight: u64) -> Self {
        Self { cell, weight }
    }
}

/// Validated input for one sourced extensive quantity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RefinementRequest {
    context: RefinementContext,
    parent: S2CellId,
    quantity: QuantityCode,
    parent_total: u64,
    evidence: Vec<ChildEvidence>,
}

impl RefinementRequest {
    pub(super) fn new(
        context: RefinementContext,
        parent: S2CellId,
        quantity: QuantityCode,
        parent_total: u64,
        mut evidence: Vec<ChildEvidence>,
    ) -> Result<Self, RefinementError> {
        validate_parent(parent)?;
        for child in &evidence {
            if child.cell.level() != REGIONAL_LEVEL {
                return Err(RefinementError::WrongChildLevel {
                    cell: child.cell,
                    expected: REGIONAL_LEVEL,
                    actual: child.cell.level(),
                });
            }
            if child.cell.ancestor(PLANETARY_LEVEL)? != parent {
                return Err(RefinementError::ChildOutsideParent {
                    child: child.cell,
                    parent,
                });
            }
        }
        evidence.sort_unstable_by_key(|child| child.cell);
        if let Some(pair) = evidence
            .windows(2)
            .find(|pair| pair[0].cell == pair[1].cell)
        {
            return Err(RefinementError::DuplicateChild(pair[0].cell));
        }
        if evidence.len() != REGIONAL_CHILD_COUNT {
            return Err(RefinementError::IncompleteChildCoverage {
                expected: REGIONAL_CHILD_COUNT,
                actual: evidence.len(),
            });
        }
        let expected = regional_children(parent)?;
        if evidence
            .iter()
            .map(|child| child.cell)
            .ne(expected.iter().copied())
        {
            return Err(RefinementError::IncompleteChildCoverage {
                expected: REGIONAL_CHILD_COUNT,
                actual: evidence.len(),
            });
        }
        if !evidence.iter().any(|child| child.weight > 0) {
            return Err(RefinementError::AllWeightsZero);
        }

        Ok(Self {
            context,
            parent,
            quantity,
            parent_total,
            evidence,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ChildAllocation {
    cell: S2CellId,
    amount: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RefinedLayer {
    parent: S2CellId,
    quantity: QuantityCode,
    children: Vec<ChildAllocation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AllocationCandidate {
    cell: S2CellId,
    amount: u64,
    remainder: u128,
    tie_break: Digest,
}

/// Allocate one integer extensive total without creating or losing a unit.
pub(super) fn refine(request: RefinementRequest) -> Result<RefinedLayer, RefinementError> {
    let weight_sum = request.evidence.iter().try_fold(0_u128, |sum, child| {
        sum.checked_add(u128::from(child.weight))
            .ok_or(RefinementError::ArithmeticOverflow)
    })?;
    if weight_sum == 0 {
        return Err(RefinementError::AllWeightsZero);
    }

    let mut candidates = Vec::with_capacity(REGIONAL_CHILD_COUNT);
    let mut base_sum = 0_u128;
    for child in &request.evidence {
        let numerator = u128::from(request.parent_total)
            .checked_mul(u128::from(child.weight))
            .ok_or(RefinementError::ArithmeticOverflow)?;
        let base = numerator / weight_sum;
        let amount = u64::try_from(base).map_err(|_| RefinementError::ArithmeticOverflow)?;
        base_sum = base_sum
            .checked_add(base)
            .ok_or(RefinementError::ArithmeticOverflow)?;
        candidates.push(AllocationCandidate {
            cell: child.cell,
            amount,
            remainder: numerator % weight_sum,
            tie_break: residual_tie_break(
                request.context,
                request.parent,
                request.quantity,
                child.cell,
            ),
        });
    }

    let residual = u128::from(request.parent_total)
        .checked_sub(base_sum)
        .ok_or(RefinementError::ArithmeticOverflow)?;
    let residual = usize::try_from(residual).map_err(|_| RefinementError::ResidualTooLarge)?;
    if residual >= REGIONAL_CHILD_COUNT {
        return Err(RefinementError::ResidualTooLarge);
    }

    candidates.sort_unstable_by(|left, right| {
        right
            .remainder
            .cmp(&left.remainder)
            .then_with(|| left.tie_break.cmp(&right.tie_break))
            .then_with(|| left.cell.cmp(&right.cell))
    });
    let mut distributed = 0_usize;
    for candidate in candidates
        .iter_mut()
        .filter(|candidate| candidate.remainder > 0)
        .take(residual)
    {
        candidate.amount = candidate
            .amount
            .checked_add(1)
            .ok_or(RefinementError::ArithmeticOverflow)?;
        distributed += 1;
    }
    if distributed != residual {
        return Err(RefinementError::ResidualTooLarge);
    }
    candidates.sort_unstable_by_key(|candidate| candidate.cell);

    let layer = RefinedLayer {
        parent: request.parent,
        quantity: request.quantity,
        children: candidates
            .into_iter()
            .map(|candidate| ChildAllocation {
                cell: candidate.cell,
                amount: candidate.amount,
            })
            .collect(),
    };
    let coarsened = coarsen(&layer)?;
    if coarsened != request.parent_total {
        return Err(RefinementError::ConservationFailure {
            expected: request.parent_total,
            actual: coarsened,
        });
    }
    Ok(layer)
}

/// Validate a refined layer and exactly reaggregate its parent total.
pub(super) fn coarsen(layer: &RefinedLayer) -> Result<u64, RefinementError> {
    validate_parent(layer.parent)?;
    if layer.children.len() != REGIONAL_CHILD_COUNT {
        return Err(RefinementError::IncompleteChildCoverage {
            expected: REGIONAL_CHILD_COUNT,
            actual: layer.children.len(),
        });
    }
    if layer
        .children
        .windows(2)
        .any(|pair| pair[0].cell >= pair[1].cell)
    {
        return Err(RefinementError::NonCanonicalChildOrder);
    }

    let expected = regional_children(layer.parent)?;
    let mut total = 0_u128;
    for (child, expected_cell) in layer.children.iter().zip(expected) {
        if child.cell != expected_cell {
            return Err(RefinementError::IncompleteChildCoverage {
                expected: REGIONAL_CHILD_COUNT,
                actual: layer.children.len(),
            });
        }
        total = total
            .checked_add(u128::from(child.amount))
            .ok_or(RefinementError::ArithmeticOverflow)?;
    }
    u64::try_from(total).map_err(|_| RefinementError::ArithmeticOverflow)
}

fn residual_tie_break(
    context: RefinementContext,
    parent: S2CellId,
    quantity: QuantityCode,
    child: S2CellId,
) -> Digest {
    let mut material =
        Vec::with_capacity(RESIDUAL_STREAM_DOMAIN.len() + 2 + 8 + 32 + 8 + 2 + 8 + 2 + 8);
    material.extend_from_slice(RESIDUAL_STREAM_DOMAIN);
    material.extend_from_slice(&context.policy_version.to_be_bytes());
    material.extend_from_slice(&context.world_seed.get().to_be_bytes());
    material.extend_from_slice(context.world_data_bundle_digest.as_bytes());
    material.extend_from_slice(&parent.get().to_be_bytes());
    material.extend_from_slice(&context.process_code.to_be_bytes());
    material.extend_from_slice(&context.refinement_generation.to_be_bytes());
    material.extend_from_slice(&quantity.get().to_be_bytes());
    material.extend_from_slice(&child.get().to_be_bytes());
    Digest::sha256(&material)
}

fn validate_parent(parent: S2CellId) -> Result<(), RefinementError> {
    if parent.level() != PLANETARY_LEVEL {
        return Err(RefinementError::WrongParentLevel {
            expected: PLANETARY_LEVEL,
            actual: parent.level(),
        });
    }
    Ok(())
}

fn regional_children(parent: S2CellId) -> Result<Vec<S2CellId>, RefinementError> {
    validate_parent(parent)?;
    parent
        .descendants_at(REGIONAL_LEVEL)
        .map_err(RefinementError::S2)
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub(super) enum RefinementError {
    #[error("world-data bundle content digest must not be zero")]
    ZeroWorldDataBundleDigest,
    #[error("refinement policy version must be nonzero")]
    ZeroPolicyVersion,
    #[error("refinement process code must be nonzero")]
    ZeroProcessCode,
    #[error("conserved quantity code must be nonzero")]
    ZeroQuantityCode,
    #[error("refinement parent must be S2 L{expected}, got L{actual}")]
    WrongParentLevel { expected: u8, actual: u8 },
    #[error("refinement child {cell} must be S2 L{expected}, got L{actual}")]
    WrongChildLevel {
        cell: S2CellId,
        expected: u8,
        actual: u8,
    },
    #[error("refinement child {child} is outside parent {parent}")]
    ChildOutsideParent { child: S2CellId, parent: S2CellId },
    #[error("refinement child {0} occurs more than once")]
    DuplicateChild(S2CellId),
    #[error("regional child coverage must contain {expected} cells, got {actual}")]
    IncompleteChildCoverage { expected: usize, actual: usize },
    #[error("regional children are not in strict numeric S2 order")]
    NonCanonicalChildOrder,
    #[error("at least one refinement evidence weight must be positive")]
    AllWeightsZero,
    #[error("refinement arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("refinement residual exceeds the child allocation bound")]
    ResidualTooLarge,
    #[error("refinement changed the parent total: expected {expected}, got {actual}")]
    ConservationFailure { expected: u64, actual: u64 },
    #[error(transparent)]
    S2(#[from] S2CellIdError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use world_domain::MAX_S2_LEVEL;

    const L10_CELL_COUNT_PER_FACE: u64 = 1 << (2 * PLANETARY_LEVEL);

    fn parent_on_face(face: u64) -> S2CellId {
        parent_on_face_at_index(face, 0)
    }

    fn parent_on_face_at_index(face: u64, index: u64) -> S2CellId {
        assert!(face < 6);
        assert!(index < L10_CELL_COUNT_PER_FACE);
        let low_bit = 1_u64 << (2 * (MAX_S2_LEVEL - PLANETARY_LEVEL));
        let offset = (index * 2 + 1) * low_bit;
        S2CellId::new((face << 61) + offset).expect("valid indexed L10 parent")
    }

    fn context() -> RefinementContext {
        RefinementContext::new(
            WorldSeed::new(0x0123_4567_89ab_cdef),
            Digest::sha256(b"refinement test data manifest"),
            1,
            17,
            23,
        )
        .expect("valid refinement context")
    }

    fn quantity(value: u16) -> QuantityCode {
        QuantityCode::new(value).expect("nonzero test quantity")
    }

    fn evidence(parent: S2CellId) -> Vec<ChildEvidence> {
        regional_children(parent)
            .expect("valid regional children")
            .into_iter()
            .enumerate()
            .map(|(index, cell)| {
                let index = u64::try_from(index).expect("regional index fits u64");
                ChildEvidence::new(cell, (index * 37 + 11) % 101 + 1)
            })
            .collect()
    }

    fn equal_evidence(parent: S2CellId) -> Vec<ChildEvidence> {
        regional_children(parent)
            .expect("valid regional children")
            .into_iter()
            .map(|cell| ChildEvidence::new(cell, 1))
            .collect()
    }

    fn selected_cells(layer: &RefinedLayer) -> BTreeSet<S2CellId> {
        layer
            .children
            .iter()
            .filter(|child| child.amount > 0)
            .map(|child| child.cell)
            .collect()
    }

    fn amount_for(layer: &RefinedLayer, cell: S2CellId) -> u64 {
        layer
            .children
            .iter()
            .find(|child| child.cell == cell)
            .map_or(0, |child| child.amount)
    }

    fn layer_fingerprint(layer: &RefinedLayer) -> Digest {
        let mut material = Vec::with_capacity(32 + layer.children.len() * 16);
        material.extend_from_slice(b"a-tiny-civilization/refined-layer-test/v1\0");
        material.extend_from_slice(&layer.parent.get().to_be_bytes());
        material.extend_from_slice(&layer.quantity.get().to_be_bytes());
        material.extend_from_slice(
            &u32::try_from(layer.children.len())
                .expect("test layer length fits u32")
                .to_be_bytes(),
        );
        for child in &layer.children {
            material.extend_from_slice(&child.cell.get().to_be_bytes());
            material.extend_from_slice(&child.amount.to_be_bytes());
        }
        Digest::sha256(&material)
    }

    #[test]
    fn enumerates_every_l14_descendant_across_every_face_range() {
        for face in 0..6 {
            for index in [0, L10_CELL_COUNT_PER_FACE / 2, L10_CELL_COUNT_PER_FACE - 1] {
                let parent = parent_on_face_at_index(face, index);
                let children = regional_children(parent).expect("valid child enumeration");
                assert_eq!(children.len(), REGIONAL_CHILD_COUNT);
                assert!(children.windows(2).all(|pair| pair[0] < pair[1]));
                assert!(children.iter().all(|child| child.level() == REGIONAL_LEVEL));
                assert!(
                    children
                        .iter()
                        .all(|child| child.ancestor(PLANETARY_LEVEL) == Ok(parent))
                );
            }
        }
    }

    #[test]
    fn refinement_and_coarsening_conserve_extreme_totals_on_every_face() {
        for face in 0..6 {
            let parent = parent_on_face(face);
            for total in [0, 1, 255, 256, 257, u64::MAX] {
                let request =
                    RefinementRequest::new(context(), parent, quantity(1), total, evidence(parent))
                        .expect("valid request");
                let layer = refine(request).expect("valid conserved refinement");
                assert_eq!(coarsen(&layer), Ok(total));
            }
        }
    }

    #[test]
    fn every_allocation_is_a_quota_floor_or_ceiling_and_zero_weights_stay_zero() {
        let parent = parent_on_face(1);
        let evidence = regional_children(parent)
            .expect("valid children")
            .into_iter()
            .enumerate()
            .map(|(index, cell)| {
                let weight = if index % 7 == 0 {
                    0
                } else {
                    u64::try_from(index % 23 + 1).expect("small weight")
                };
                ChildEvidence::new(cell, weight)
            })
            .collect::<Vec<_>>();
        let total = u64::MAX;
        let request =
            RefinementRequest::new(context(), parent, quantity(2), total, evidence.clone())
                .expect("valid quota request");
        let layer = refine(request).expect("valid quota refinement");
        let weight_sum = evidence
            .iter()
            .map(|child| u128::from(child.weight))
            .sum::<u128>();
        let base_sum = evidence
            .iter()
            .map(|child| u128::from(total) * u128::from(child.weight) / weight_sum)
            .sum::<u128>();
        let expected_residual = u128::from(total) - base_sum;
        let mut actual_residual = 0_u128;

        for child in evidence {
            let floor = u128::from(total) * u128::from(child.weight) / weight_sum;
            let actual = u128::from(amount_for(&layer, child.cell));
            assert!(actual == floor || actual == floor + 1);
            if child.weight == 0 {
                assert_eq!(actual, 0);
            }
            actual_residual += actual - floor;
        }
        assert_eq!(actual_residual, expected_residual);
        assert!(actual_residual < REGIONAL_CHILD_COUNT as u128);
        assert_eq!(coarsen(&layer), Ok(total));
    }

    #[test]
    fn hamilton_allocation_is_explicitly_not_population_monotone() {
        let parent = parent_on_face(5);
        let children = regional_children(parent).expect("valid children");
        let weights = [1_500_u64, 1_500, 900, 500, 500, 200];
        let evidence = children
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                ChildEvidence::new(*cell, weights.get(index).copied().unwrap_or(0))
            })
            .collect::<Vec<_>>();
        let at_25 = refine(
            RefinementRequest::new(context(), parent, quantity(9), 25, evidence.clone())
                .expect("valid 25-unit request"),
        )
        .expect("valid 25-unit allocation");
        let at_26 = refine(
            RefinementRequest::new(context(), parent, quantity(9), 26, evidence)
                .expect("valid 26-unit request"),
        )
        .expect("valid 26-unit allocation");

        // This classic Alabama-paradox vector prevents future callers from assuming
        // that a synthesized allocation can be recalculated after the total changes.
        assert_eq!(amount_for(&at_25, children[3]), 3);
        assert_eq!(amount_for(&at_26, children[3]), 2);
        assert_eq!(amount_for(&at_25, children[4]), 3);
        assert_eq!(amount_for(&at_26, children[4]), 2);
        assert!(
            children[6..]
                .iter()
                .all(|cell| amount_for(&at_25, *cell) == 0 && amount_for(&at_26, *cell) == 0)
        );
    }

    #[test]
    fn identical_generation_bundle_and_evidence_round_trip_repeatably() {
        let parent = parent_on_face(2);
        let forward = evidence(parent);
        let mut reversed = forward.clone();
        reversed.reverse();
        let request =
            RefinementRequest::new(context(), parent, quantity(7), 9_876_543_210, forward)
                .expect("valid forward request");
        let reversed_request =
            RefinementRequest::new(context(), parent, quantity(7), 9_876_543_210, reversed)
                .expect("valid reversed request");

        let first = refine(request.clone()).expect("valid first refinement");
        let second = refine(reversed_request).expect("valid reversed refinement");
        assert_eq!(first, second);
        let coarsened = coarsen(&first).expect("valid coarsening");
        assert_eq!(coarsened, 9_876_543_210);
        assert_eq!(refine(request), Ok(first));
    }

    #[test]
    fn every_stream_component_separates_equal_remainder_choices() {
        let parent = parent_on_face(4);
        let base_context = context();
        let base = refine(
            RefinementRequest::new(
                base_context,
                parent,
                quantity(3),
                64,
                equal_evidence(parent),
            )
            .expect("valid base request"),
        )
        .expect("valid base refinement");
        let baseline = selected_cells(&base);

        let contexts = [
            RefinementContext::new(
                WorldSeed::new(base_context.world_seed.get() + 1),
                base_context.world_data_bundle_digest,
                base_context.policy_version,
                base_context.process_code,
                base_context.refinement_generation,
            )
            .expect("changed seed"),
            RefinementContext::new(
                base_context.world_seed,
                Digest::sha256(b"different world-data bundle"),
                base_context.policy_version,
                base_context.process_code,
                base_context.refinement_generation,
            )
            .expect("changed bundle"),
            RefinementContext::new(
                base_context.world_seed,
                base_context.world_data_bundle_digest,
                base_context.policy_version + 1,
                base_context.process_code,
                base_context.refinement_generation,
            )
            .expect("changed policy"),
            RefinementContext::new(
                base_context.world_seed,
                base_context.world_data_bundle_digest,
                base_context.policy_version,
                base_context.process_code + 1,
                base_context.refinement_generation,
            )
            .expect("changed process"),
            RefinementContext::new(
                base_context.world_seed,
                base_context.world_data_bundle_digest,
                base_context.policy_version,
                base_context.process_code,
                base_context.refinement_generation + 1,
            )
            .expect("changed generation"),
        ];
        for changed_context in contexts {
            let changed = refine(
                RefinementRequest::new(
                    changed_context,
                    parent,
                    quantity(3),
                    64,
                    equal_evidence(parent),
                )
                .expect("valid changed request"),
            )
            .expect("valid changed refinement");
            assert_ne!(selected_cells(&changed), baseline);
        }

        let changed_quantity = refine(
            RefinementRequest::new(
                base_context,
                parent,
                quantity(4),
                64,
                equal_evidence(parent),
            )
            .expect("valid changed quantity request"),
        )
        .expect("valid changed quantity refinement");
        assert_ne!(selected_cells(&changed_quantity), baseline);
    }

    #[test]
    fn reference_allocation_has_a_stable_byte_fingerprint() {
        let parent = parent_on_face(4);
        let layer = refine(
            RefinementRequest::new(context(), parent, quantity(3), 64, equal_evidence(parent))
                .expect("valid reference request"),
        )
        .expect("valid reference allocation");
        assert_eq!(
            layer_fingerprint(&layer).to_string(),
            "149f5b6557a602e194b4b72c71ff64ae4c1b9288000d57e7a0b7b6faaf2d4e9f"
        );
    }

    #[test]
    fn invalid_context_coverage_and_weights_fail_closed() {
        assert!(matches!(
            RefinementContext::new(WorldSeed::new(1), Digest::ZERO, 1, 1, 0),
            Err(RefinementError::ZeroWorldDataBundleDigest)
        ));
        assert!(matches!(
            RefinementContext::new(WorldSeed::new(1), Digest::sha256(b"x"), 0, 1, 0),
            Err(RefinementError::ZeroPolicyVersion)
        ));
        assert!(matches!(
            RefinementContext::new(WorldSeed::new(1), Digest::sha256(b"x"), 1, 0, 0),
            Err(RefinementError::ZeroProcessCode)
        ));
        assert!(matches!(
            QuantityCode::new(0),
            Err(RefinementError::ZeroQuantityCode)
        ));

        let parent = parent_on_face(0);
        let mut missing = evidence(parent);
        missing.pop();
        assert!(matches!(
            RefinementRequest::new(context(), parent, quantity(1), 1, missing),
            Err(RefinementError::IncompleteChildCoverage { actual: 255, .. })
        ));

        let mut duplicate = evidence(parent);
        duplicate[1] = duplicate[0];
        assert!(matches!(
            RefinementRequest::new(context(), parent, quantity(1), 1, duplicate),
            Err(RefinementError::DuplicateChild(_))
        ));

        let zeros = regional_children(parent)
            .expect("valid children")
            .into_iter()
            .map(|cell| ChildEvidence::new(cell, 0))
            .collect();
        assert!(matches!(
            RefinementRequest::new(context(), parent, quantity(1), 1, zeros),
            Err(RefinementError::AllWeightsZero)
        ));

        let mut wrong_level = evidence(parent);
        wrong_level[0] = ChildEvidence::new(parent, 1);
        assert!(matches!(
            RefinementRequest::new(context(), parent, quantity(1), 1, wrong_level),
            Err(RefinementError::WrongChildLevel { .. })
        ));

        let mut outside = evidence(parent);
        outside[0] = evidence(parent_on_face(1))[0];
        assert!(matches!(
            RefinementRequest::new(context(), parent, quantity(1), 1, outside),
            Err(RefinementError::ChildOutsideParent { .. })
        ));
    }

    #[test]
    fn coarsening_rejects_reordered_missing_and_overflowing_children() {
        let parent = parent_on_face(3);
        let layer = refine(
            RefinementRequest::new(context(), parent, quantity(1), 100, evidence(parent))
                .expect("valid request"),
        )
        .expect("valid layer");

        let mut reordered = layer.clone();
        reordered.children.swap(0, 1);
        assert!(matches!(
            coarsen(&reordered),
            Err(RefinementError::NonCanonicalChildOrder)
        ));

        let mut missing = layer.clone();
        missing.children.pop();
        assert!(matches!(
            coarsen(&missing),
            Err(RefinementError::IncompleteChildCoverage { actual: 255, .. })
        ));

        let mut overflowing = layer;
        overflowing.children[0].amount = u64::MAX;
        overflowing.children[1].amount = 1;
        assert!(matches!(
            coarsen(&overflowing),
            Err(RefinementError::ArithmeticOverflow)
        ));
    }
}
