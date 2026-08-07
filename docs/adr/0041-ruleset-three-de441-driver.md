# ADR 0041: Ruleset-three DE441 source driver

Accepted on 2026-08-07.

Ruleset three records one fixed-scale `CelestialState` after every tick advances.
The runner evaluates the project-owned `civilization-data inspect jpl-de441-epoch`
command against the read-only, content-addressed DE441 files staged at
`/runtime/data/source-cache/jpl-de441`. The epoch is the next simulation tick
multiplied by the immutable `tick_duration_seconds` committed at genesis.

The returned state is committed in the canonical event batch. Replayers use that
recorded event only; they never open DE441 files or invoke the evaluator. The
engine rejects a ruleset-three tick with no state, a duplicate state for one tick,
or a non-monotonic celestial time.

This is a source-execution boundary, not scientific admission. It does not yet
apply Earth orientation, atmospheric forcing, illumination, ocean response, animal
behaviour, or ecological effects. Public projections discard the event entirely.

The current runner default remains ruleset two. A new provisional world must
explicitly select ruleset three, and must have the staged DE441 artifacts available.
This avoids retroactively changing an existing world's committed causal rules.

## Verification

An isolated PostgreSQL integration world (`4c3e4c81-6df4-4595-8c10-06775c4bf2a7`)
was initialized with ruleset three against the staged pinned artifacts. Its first
tick recorded the DE441 state at TDB second 300; replay from genesis and
snapshot-plus-tail matched the committed cursor through tick 687.
