# ADR 0014: Initial ecology refinement exactly conserves sourced totals

## Status

Accepted on 2026-08-06 as a private reference contract. This does not yet define a
causal refinement trigger, durable refined state, ecological quantity registry, event,
snapshot field, database record, or canonical genesis path.

## Context

The full-Earth model keeps canonical ecology at S2 L10 and materializes finer causal
detail only when the world requires it. An initial L10-to-L14 refinement must not
create or destroy water, nutrients, biomass, or abundance merely because integer
totals do not divide evenly. It must also produce the same result regardless of input
iteration order, platform scheduling, or observer activity.

Freezing durable ecology before exact global source artifacts and coupled ecological
constraints exist would turn placeholder quantities into permanent history. The
smallest honest checkpoint is therefore a private, generic proof for one sourced
extensive scalar at a time.

## Decision

- One valid S2 L10 parent has exactly 256 S2 L14 descendants. A request must contain
  every descendant exactly once; wrong levels, foreign cells, duplicates, missing
  cells, and an all-zero evidence vector fail closed.
- Each child supplies a nonnegative integer evidence weight. A weight is evidence for
  proportional allocation, not a measured child quantity. Its source, units,
  uncertainty, inference status, and normalization remain part of the scientific
  bundle rather than this arithmetic kernel.
- Allocation uses exact Hamilton largest-remainder apportionment. For each child,
  checked `u128` arithmetic computes the quota floor and remainder. Remaining units go
  by descending remainder, then ascending SHA-256 rank, then ascending numeric CellId.
  No floating point or collection iteration order participates.
- The versioned SHA-256 residual stream commits the refinement policy version, world
  seed, exact normalized world-data bundle content digest, parent CellId, process code,
  retained refinement generation, provisional quantity code, and child CellId. Every
  integer is fixed-width and big-endian. Changing any component selects a distinct
  synthesis stream.
- Zero-weight children remain zero. All 256 allocations are explicit and returned in
  strict numeric CellId order, including zero allocations.
- Coarsening accepts only that exact complete canonical coverage, sums through checked
  `u128`, and rejects totals outside `u64`. Refinement reaggregates its own result and
  fails if it differs from the parent total.
- This algorithm is for **one-time initial synthesis only**. The resulting child
  allocation, its generation, and later causal deltas must be retained. A parent-total
  change must never cause the original children to be recomputed.

## Why initial allocations must be retained

Hamilton apportionment satisfies the per-child quota bound but is not population
monotone: increasing a parent total can reduce a child's allocation (the Alabama
paradox). Re-running refinement after an ecological change could therefore move matter
between untouched children without a causal event. Likewise, incrementing the
refinement generation produces a different deterministic synthesis; it is not a
reload operation.

Once durable refinement exists, changes apply as explicit causal deltas to retained
children. Coarsening reaggregates those retained values and their deltas; it does not
erase history by synthesizing them again.

## Deliberate non-decisions

- Quantity and process codes are provisional private types, not a frozen registry.
- The private request accepts weights beside a claimed bundle digest; it does not yet
  prove that those weights were derived from records under that digest. Canonical
  integration must construct evidence only from a verified bundle, not accept an
  arbitrary caller vector.
- The proof's in-memory `RefinedLayer` retains only parent, quantity, and child amounts.
  It does not satisfy the future requirement to persist context, evidence identity,
  generation, or causal deltas.
- Independently conserving several scalars can violate coupled constraints: separate
  water, carbon, biomass, organism-count, age-cohort, or stoichiometric allocations may
  describe an impossible ecosystem. A sourced, versioned vector-allocation policy must
  define those relationships before real ecological state uses this kernel.
- Evidence-weight construction, normalized units, scientific uncertainty, and
  inference rules await exact global source snapshots. This proof never invents
  ecological measurements.
- Refinement triggers, causal epochs, movement/flow integration, delta retention,
  coarsening eligibility, durable positions, events, snapshots, PostgreSQL tables,
  configuration binding, and partition-barrier integration remain later decisions.
- An observer read cannot invoke this private module and will never be a canonical
  refinement trigger.

## Verification

Reference tests prove:

- exact enumeration of all 256 L14 descendants at low, middle, and high numeric L10
  parents on all six S2 faces;
- conservation for totals including zero, values around the child count, and
  `u64::MAX`;
- every allocation is its exact quota floor or ceiling and zero weights stay zero;
- evidence permutations produce identical allocations and identical inputs reproduce
  the same byte fingerprint;
- every residual-stream component separates an equal-remainder reference allocation;
- malformed contexts, coverage, ordering, and overflowing coarsening fail closed; and
- the classic 25-to-26-seat Alabama-paradox example remains a regression guard against
  treating this one-time synthesis as a dynamic rebalance rule.

## Consequences

The repository now has an exact, order-independent conservation proof between its
first two causal S2 tiers without claiming durable ecology. Together with
[ADR 0012](0012-deterministic-partition-barrier.md) and
[ADR 0013](0013-fixed-point-ecef-s2-routing.md), this removes another arithmetic
ambiguity before embodied state is frozen. The engine now carries embodied positions
and an empty durable schedule, but production canonical genesis remains gated on exact
source bundles, coupled ecological quantities, retained refined state and deltas,
movement, the first real scheduled process, and the persistence barrier.
