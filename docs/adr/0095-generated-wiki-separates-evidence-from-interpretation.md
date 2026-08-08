# ADR 0095: The generated wiki separates evidence from interpretation

Accepted on 2026-08-08.

## Decision

The first observer wiki is a deterministic read model composed from the existing finding and
artifact projections. `GET /api/v1/worlds/{world_id}/wiki` returns stable entries ordered by their
latest cited event sequence. Each entry carries an interpretation provenance independently of its
event-level evidence provenance and external scientific citations.

The endpoint accepts no authored text. It performs no model calls and cannot read private memory,
cognition payloads, raw reproductive mechanisms, observer labels, or canonical state beyond the
existing public projection ports. Research papers and in-world writing remain absent until durable
world evidence can support them; the wiki does not manufacture an invention category to make the
site appear active.

For an altered material object, the force-caused physical trace is labeled `world_fact`, while the
observatory's decision to file the object as an artifact is labeled `observer_inference`. The entry
explicitly refuses to infer purpose, symbolism, or meaning.

## Enforcement

- Composition is a pure function with deterministic ordering and a fixed version.
- Every entry contains at least one exact event identifier, sequence, and simulation tick.
- Artifact source identity retains its cited HTTPS source.
- The observer API reads only the finding and artifact projection ports.
- A regression test proves that observer classification never changes the physical evidence label.
- The live ruleset-19 qualification world returned the cited silicon-dioxide entry with evidence at
  sequences 73 and 749 without changing its canonical cursor or any projection.

