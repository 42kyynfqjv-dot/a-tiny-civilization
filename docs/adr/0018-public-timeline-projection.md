# ADR 0018: The public timeline is a safe, cursor-driven observer projection

## Status

Accepted on 2026-08-06.

## Decision

- `observer-projection` deterministically maps only committed canonical event batches
  into public timeline items. Every item carries its source event ID, world, sequence,
  tick, event index, projection version, and `world_fact` provenance.
- The mapping intentionally excludes birth category, parentage, location, species
  identity, mortality mechanism, raw internal identifiers, and tick-only noise. Public
  language reports restrained outcomes such as a birth, a life ending, extinction, or
  archival; it does not describe sexual activity or violence.
- `civilization-projector` runs outside the runner. It reads committed batches after a
  versioned PostgreSQL cursor, atomically appends idempotent timeline items and advances
  that cursor, then exposes no writes to canonical state.
- The observer API exposes a bounded read-only timeline endpoint. Projection rows are
  append-only; correcting presentation requires a new projection version, never a
  rewrite of public historical evidence.
- The public world index includes its manifest hash, canonical event-chain head, and
  state hash at the displayed cursor. These are copied from the committed world cursor,
  not calculated by the browser or an observer projection. They make the displayed
  history independently auditable without releasing sensitive event payloads.

## Consequences

The observatory can show a factual, replay-derived timeline without an LLM narrator or
observer influence. It remains a finding aid, not a source of agent knowledge. Richer
wiki claims, artifact classifications, first/record/streak digests, biographies, and
maps can use the same source-event/provenance pattern in later projection versions.
