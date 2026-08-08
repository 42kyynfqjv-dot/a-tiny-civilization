# ADR 0138: canonical genesis has a database-free proof

Status: accepted

## Context

Canonical initialization verifies a large, content-addressed full-Earth artifact tree before it
constructs and commits genesis to PostgreSQL. A missing or temporarily inaccessible database must
not prevent operators from proving that a proposed portable input bundle is accepted by the exact
world constructor, or that its genesis event batch and snapshot replay to the same state.

A reduced fixture or a second implementation would not prove the production path. It could drift
from the code that is eventually persisted.

## Decision

The application exposes the side-effect-free
`construct_configured_genesis_with_materials` boundary. The persistent initialization use case and
the database-free verifier both use this same constructor.

The runner provides `verify-provisional-genesis`. It:

1. requires a portable, complete `SHA256SUMS` manifest with regular non-symlink artifacts;
2. verifies every composition reference and every canonical genesis input;
3. constructs the exact ruleset-32 genesis batch and snapshot without connecting to PostgreSQL;
4. replays the batch from event zero and independently replays the snapshot with an empty tail;
5. fails unless both replay paths, the constructed state, and their hashes agree; and
6. emits the resulting event, batch, snapshot-schema, and state identities for qualification
   evidence.

The command proves deterministic construction and replay only. It does not mechanically qualify
later ticks, prove PostgreSQL durability, scientifically admit provisional inputs, admit a launch
candidate, activate a world, or authorize deployment.

## Consequences

- Canonical mechanics can continue to be verified when local socket restrictions prevent a
  database-backed qualification.
- A successful proof can be repeated on another host before any durable world exists.
- PostgreSQL migration, atomic commit, bounded advancement, projection, Hindsight/cognition, and
  production admission remain separate required gates.
- Verifying the complete full-Earth tree is deliberately I/O-heavy; skipping those hashes would
  weaken the meaning of the proof.
