# ADR 0031: Full-Earth lives retain durable S2 embodied-patch positions

## Status

Accepted on 2026-08-06. This adds the durable location/movement envelope required for
full-Earth execution; it does not define locomotion physics, a public coordinate feed,
or authorize canonical genesis.

## Context

The full-Earth configuration already declares an S2 L23 embodied-patch resolution,
but prior canonical organism state carried only an optional opaque `location_id`. That
cannot locate a life on Earth, verify movement, route work to a partition, or replay
physical interactions. Retrofitting it after a public world begins would rewrite state
hash and event semantics.

## Decision

- Event schema v4 adds an optional `embodied_patch` to initialization and birth facts,
  plus an `organism_moved` fact that names both the expected source patch and resolved
  destination patch.
- A full-Earth configuration requires each initialized or born organism to have an
  S2 CellId exactly at its declared `embodied_patch` level. Bounded legacy worlds must
  not carry one.
- Movement is conditional on the organism's current patch matching the event source;
  replay rejects stale, wrong-level, or configuration-incompatible movement.
- Full-Earth state hashes and snapshots use new schema versions. Configured legacy and
  schema-v1 worlds retain their existing bytes and replay rules.
- Observer projections intentionally omit patch identities and movement records. A
  later public map is a separate, versioned, privacy/presentation-reviewed projection;
  it cannot feed position changes back into the runner.

## Consequences

The engine can now retain and replay where an embodied life is at the declared causal
resolution without teaching that S2 address to the life itself. Locomotion, terrain
collision, perception ranges, occupancy policy, cross-patch interactions, and the
partition queue still need deterministic physical rules before full-Earth genesis is
enabled.
