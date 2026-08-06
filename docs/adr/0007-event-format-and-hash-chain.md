# ADR 0007: Event batches use versioned stable JSON and a SHA-256 hash chain

- Status: accepted
- Date: 2026-08-06

## Context

Replay must detect missing, reordered, or modified history and remain verifiable
outside PostgreSQL. Database JSONB is not itself a canonical byte representation, and
large integers cannot be represented exactly by every JSON consumer.

## Decision

- One event batch represents one atomic state transition and has a world-scoped total
  sequence. Events inside it have a zero-based index and deterministic UUIDv5 identity.
- Hash material is compact UTF-8 JSON emitted from schema-owned Rust structs. Hash
  schemas may use structs, sequences, integers, strings, and `BTreeMap`; they may not
  use floating-point values, unordered maps, wall time, or implementation debug text.
- All replay-relevant `u64` values serialize as decimal strings.
- A batch hash is SHA-256 over its schema version, world, sequence, tick, ruleset,
  previous batch hash, ordered events, and post-state hash.
- State hashes use a separately versioned causal-state view. Snapshots store the state
  hash, last event hash, and through-sequence.
- Golden-vector tests make serialization changes explicit. A new incompatible format
  receives a new schema version and decoder; prior bytes are never reinterpreted.
- Publicly reported final hashes and downloaded bundles anchor the chain. SHA-256
  detects corruption and inconsistency but is not represented as an operator signature.

## Consequences

- Removing, changing, or reordering a batch breaks replay at a precise sequence.
- A snapshot plus its tail can be compared directly with replay from genesis.
- Cross-language verifiers must implement the documented field order and decimal-string
  rules or consume the exact hash-material bytes included in a verification bundle.
- If stronger authorship guarantees become necessary, signatures can cover published
  chain heads without changing canonical simulation events.
