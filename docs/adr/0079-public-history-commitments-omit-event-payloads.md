# ADR 0079: Public history commitments omit canonical event payloads

## Status

Accepted on 2026-08-08.

## Decision

The observer API publishes bounded pages of canonical batch commitments. Each page identifies the
immutable world manifest and its hash, the current history heads, and ordered batch headers after an
explicit sequence cursor. A header contains the sequence, tick, schema and ruleset versions, event
count, previous-batch hash, batch hash, and post-state hash.

The endpoint never returns canonical event payloads. Some payloads necessarily retain private
mechanical evidence for deterministic replay, including reproductive, injury, mortality, and
cognition-input details that the public presentation policy forbids displaying. Public projections
remain the reviewed presentation boundary; possession of the canonical log is not a prerequisite
for checking that published history heads and commitment pages form one continuous chain.

The PostgreSQL adapter validates every selected batch before deriving a commitment. Nonzero cursors
must identify an existing batch, the first returned header must link to that cursor's stored hash,
and every later header must be contiguous. Limits are bounded to 256 batches.

## Consequences

- The public can retain and compare immutable history commitments without receiving unsafe details.
- Commitment pages prove hash-chain continuity and public-head consistency; they do not independently
  recompute state or event hashes without a separately governed verification bundle.
- Future public evidence views must derive from presentation-reviewed projections rather than adding
  raw payloads to this endpoint.
