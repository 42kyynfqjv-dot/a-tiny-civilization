# ADR 0006: Seeds, live rules, extinction, and successors are publicly provenance-bound

- Status: accepted
- Date: 2026-08-06

## Context

Previewing seeds, tuning a live world for engagement, terminating an inconvenient
history, or silently changing rules can curate outcomes while preserving the appearance
of emergence. Extinction must be recoverable operationally without being reversible
historically.

## Decision

- A world manifest commits its explicit unpreviewed seed, ruleset version, scientific
  dataset versions, identity tiers, and deterministic configuration before genesis.
- A seed is never rerolled because its geography or early history looks uninteresting.
- A live world ends only when its versioned mechanical extinction condition is met.
- Extinction and archive are monotonic, idempotent transitions. Archived event and
  manifest records are immutable.
- Successor creation is a separate, explicitly authorized operator command after
  archival. It uses a new world identity and seed and never overwrites its predecessor.
- Critical correctness or security fixes may activate in a live world only at a
  recorded sequence with a public rationale and new ruleset version.
- Behavioral tuning, difficulty adjustment, and engagement-driven changes wait for a
  successor world.

## Consequences

- “History without a script” has a verifiable seed and ruleset trail.
- Bug fixes remain possible without silently rewriting prior ticks.
- Extinction is final for a world while the overall project can continue.
- Operator authorization must be authenticated and audited, but can be implemented
  after the pure lifecycle state machine.
