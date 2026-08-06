# ADR 0009: World scale and scientific inputs are immutable tick-zero facts

## Status

Accepted on 2026-08-06. Its immutability and compatibility decisions remain active;
its bounded raster is retained as schema v1 for fixtures but is superseded for the
canonical world by [ADR 0010](0010-full-earth-causal-refinement.md).

## Context

The deterministic foundation proved replay with an event-schema-v1 fixture before a
real biome was selected. A durable world additionally needs an exact causal time
scale, spatial reference, scientific input bundle, and event-volume limit. If those
values remain ambient process configuration, restarting on a different host could
silently change history. Rewriting the v1 proof to add them would invalidate already
published verification bytes.

Scientific source files are also external inputs. Replay cannot depend on a remote
URL continuing to exist or returning the same bytes.

## Decision

- Event schema v1 remains supported exactly as published. Its golden batch, demo
  bundle, state hash, and event-chain head are not regenerated under new semantics.
- Event schema v2 adds one `WorldConfigured` event. It is committed at tick zero,
  immediately after `WorldStarted` and before any organism is initialized. A world
  accepts it exactly once.
- Configured state hashes and snapshots use their corresponding schema v2. Legacy
  unconfigured state hashes and snapshots continue using schema v1 so published proof
  bytes remain identical. A snapshot's schema must match whether its state is
  configured; adding the field without changing the schema marker is rejected.
- A configured world uses event schema v2 for every later batch. Replay rejects a
  schema downgrade even when the individual event payload would be valid under v1.
- Legacy configuration schema v1 pins:
  - its own schema version;
  - the positive whole-second duration represented by every simulation tick;
  - an EPSG coordinate reference, integer-millimetre raster origin and cell size,
    and integer grid dimensions;
  - a world-data bundle schema, identifier, version, SHA-256 content digest, HTTPS
    publication URL, and license expression;
  - the maximum ordered events allowed in any atomic transition, including genesis.
- Canonical configuration schema v2 instead pins the full-Earth S2 hierarchy, physical
  reference frames, causal resolution tiers, refinement policy, deterministic
  partition scheduler, durable-individual person representation, per-partition event
  budget, and pause-at-committed-boundary capacity policy.
- Before configured genesis is committed, the application layer must have the exact
  bundle bytes locally and verify their SHA-256 digest. The pure engine never fetches
  a remote source during live execution or replay.
- The normalized bundle retains every upstream source identifier, retrieval/version
  metadata, units, uncertainty, transformation, license, and assumption. Its content
  hash covers those records as well as engine parameters.
- Any raster or global cell hierarchy is an environment data structure, not an agent
  concept. Organisms may
  later occupy fixed-point positions within it, but they are never told a cell label,
  EPSG code, species name, material name, or modern map category.
- Every canonical tick is executed. Wall-clock scheduling may make ticks arrive
  faster or slower to observers, but it cannot skip causal steps or alter the
  configured tick duration.
- Procedurally normalized geometry or placement is permitted only when recorded as an
  assumption. It may combine actual measured terrain/ecological inputs, but it must
  never be described as an exact historical reconstruction.

## Consequences

- A world cannot begin until its scientific bundle is complete enough to hash and
  archive. This is intentional protection against an attractive but irreproducible
  launch.
- Operators may fetch a bundle from a local mirror without changing history when its
  content digest remains identical. The publication URL recorded at genesis is itself
  immutable metadata. Any parameter change requires a new bundle version and a new
  world.
- Integer geometry and time make cross-platform replay tractable. Finer physical
  processes can use separately versioned fixed-point units without changing an
  organism's conceptual vocabulary.
- Event-volume limits become visible world facts rather than host-dependent failure
  behavior. A transition exceeding its configured limit fails before it can commit.
  For schema v2 the limit is per deterministic partition, never a global population
  cap. See [ADR 0011](0011-population-scale-and-capacity.md).
