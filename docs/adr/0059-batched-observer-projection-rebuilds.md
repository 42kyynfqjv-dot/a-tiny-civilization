# ADR 0059: Observer rebuilds load history once and commit bounded projection ranges

## Status

Accepted on 2026-08-08 for the provisional ruleset-17 quality world.

## Context

The projector originally loaded the same immutable event history separately for the
timeline, organism index, and deterministic findings. It also opened one transaction
and rechecked one committed batch for every projection and sequence. That behavior was
correct but made a rebuild perform thousands of avoidable database round trips.

Observer projections are disposable read models. Their rebuild speed cannot alter,
delay, or select canonical work, but slow rebuilds make recovery and projection-version
changes unnecessarily expensive.

## Decision

- A projector pass reads the three durable cursors, loads committed batches once from
  the earliest cursor, and gives each projection only its unconsumed suffix.
- Each projection locks only its own cursor and atomically applies one contiguous
  range. The range is checked against immutable batch checksums before any cursor is
  advanced.
- Timeline, organism, and finding ranges may execute concurrently. They never share a
  cursor or write canonical tables.
- Retrying a complete or partially overlapping range is idempotent. A gap, changed
  checksum, mixed world, or noncontiguous range fails the transaction.
- The single-batch store interfaces remain valid and delegate to the same range
  semantics where practical.

## Evidence

The 1,205-batch provisional quality history contained 509,563 canonical events. A
clean projection rebuild completed in 82.945 seconds on the development host, down
from approximately 120 seconds before history sharing and 91.410 seconds after only
sharing the load. All six deterministic projection-table digests matched a reference
database rebuilt through the prior single-batch path; all three cursors ended at
sequence 1,205. Incremental passes read only the tail.

This is an engineering checkpoint, not a scale claim. Event decoding and projection
logic still inspect the committed history in each independent projection. A later
optimization may introduce a versioned shared decoded-event traversal, but it must
preserve independent cursors and byte-equivalent public rows.

## Consequences

- Projection recovery has far fewer transactions and history reads.
- A range transaction may be larger than one old batch transaction. The runner is
  isolated from projection connection pools, and a failed range is safely retried.
- Projection table output, provenance, and canonical history are unchanged.
