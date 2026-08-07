# ADR 0042: A public genesis requires a quality-world gate

## Status

Accepted on 2026-08-07.

## Context

The first live record is useful as an operations and audit integration world, but it
has only genesis events and a clock-only ruleset. Replacing it as soon as a later
ruleset can emit placeholder activity would make the observatory appear more alive
without making the world meaningfully more true. The project is an open experiment,
not a staged activity feed.

## Decision

- No replacement public world is initialized or deployed merely because a new engine
  checkpoint exists. Existing world histories remain immutable and retain their
  explicit provisional status.
- A proposed public genesis must pass a reviewed quality-world gate before it is
  authorized. The gate requires source-pinned local environmental input, resolved
  embodied movement and material effects, bodily survival regulation, real-taxon
  fauna/ecology handling, non-explicit reproduction and mortality mechanics, and a
  learnable memory/communication/discovery path.
- Each capability must have deterministic replay coverage, an event and state-hash
  migration story, public-safe projections, and a documented evidence/assumption
  boundary. “It produces interesting events” is not evidence of readiness.
- Full-Earth scientific admission remains a later and stricter gate: all coupled
  validation gaps in the provisional composition must be resolved or explicitly
  replaced by an admitted, reproducible world-data bundle. A quality provisional
  world must never be described as scientifically admitted.
- Deployment work may continue for observability, security, and operations, but does
  not authorize a new public genesis.

## Consequences

Implementation can progress in small, replayable checkpoints without creating a
social or marketing deadline for an immature world. The current public record stays
useful as an openly labelled integration artifact. The next genesis is a deliberate
technical and scientific decision, not an automatic side-effect of deploying code.
