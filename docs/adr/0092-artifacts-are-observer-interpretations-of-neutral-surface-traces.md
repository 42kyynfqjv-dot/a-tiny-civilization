# ADR 0092: Artifacts are observer interpretations of neutral surface traces

Accepted on 2026-08-08 as ruleset 19.

## Decision

A held material object can acquire a bounded `surface_trace_units` value only when its holder
executes the existing primitive `ApplyForce` action. The canonical event records the exact previous
value, applied force, and resulting value. The same transition creates a direct touch perception of
the new scalar value for that organism. The runner supplies no artifact, tool, mark, symbol,
writing, purpose, or affordance label.

The response is intentionally a provisional engineering scalar. It is enough to implement and test
durable physical alteration without pretending that the current model captures material fracture,
plasticity, abrasion, or hardness. Scientific admission can later replace or refine the physical
response in a new ruleset; ruleset-19 history remains replayable as recorded.

`public-artifact-v1` is an observer-only append-only projection. It joins the real cited material
identity introduced at genesis to the first and latest surface-trace events and exposes only that
provenance and accumulated trace. The term "artifact" is a filing choice made by the observatory,
not knowledge in the world. The generic timeline, organism index, and finding aid continue to omit
the private actor and mechanism event.

## Enforcement

- Event schema 21 requires exact force-to-trace arithmetic and rejects overflow.
- Rulesets before 19 reject nonzero surface traces and retain their historical hashes.
- The engine derives both the trace and direct perception from the selected action and rejects
  missing or fabricated coupled events.
- Snapshot/state schema 22 carries trace state; zero values are omitted for legacy hash stability.
- The artifact projection has its own durable cursor and append-only database triggers.
- Backend readiness and pre-genesis qualification require all five projections to reach the world
  cursor.
- The runner has no dependency on observer projection, API, supporter, authentication, or payment
  code.

## Compatibility proof

Migration 22 and `public-artifact-v1` were applied to the preserved ruleset-18 qualification world
at tick 2,347 and sequence 2,388. The projector consumed every historical batch, produced no
artifact rows, and advanced its independent cursor to 2,388. Full replay and the fail-closed
qualification report then passed with five of five projections current. No canonical event,
snapshot, memory record, cognition record, or world cursor was changed.
