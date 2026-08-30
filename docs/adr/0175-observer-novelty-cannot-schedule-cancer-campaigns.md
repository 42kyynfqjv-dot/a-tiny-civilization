# ADR 0175: Observer novelty cannot schedule Cancer campaigns

Date: 2026-08-30

Status: Accepted

## Context

The campaign scheduler joined the current Europe PMC novelty audit before selecting
an unresolved blind-discovery root. That made an observer convenience part of the
write-side research schedule: a network outage, a revised novelty method, or a
`known_overlap` assessment could freeze otherwise valid internal falsification work.
Live inspection also found mature unresolved lineages held solely because their
method-2 audit had not yet arrived.

Tissue-candidate reconstruction had a separate liveness fault. It treated every
completed campaign child as a successful model delivery. A durable, receipt-less
provider failure therefore made a later valid survivor look corrupt even though the
scheduler correctly continued past that failed attempt.

## Decision

- Campaign selection and reconstruction use only immutable internal research
  requests/results, the current deterministic virtual result, and lineage state.
  They never join or interpret an observer novelty audit.
- Europe PMC audits remain durable, method-versioned observer evidence. They may
  change how the console labels or ranks work for a human observer, but cannot
  create, suppress, delay, or redirect a canonical/model research turn.
- A completed follow-up with a valid ladder result but no receipt is a recorded
  delivery failure, not an experiment. Reconstruction validates its immutable
  request/result provenance and then skips it.
- A receipt-less replication row must not carry a virtual-experiment result. A
  receipt-less synthesis row is likewise skipped. Either contradiction fails
  closed as corrupt provenance.
- Test and synthesis counts include only successful deliveries. Tissue admission
  requires exactly the selected successful synthesis and the successful virtual
  experiments named by that synthesis.

This decision supersedes the novelty-gating and novelty-reconstruction clauses in
ADR 0151. Existing novelty rows, research receipts, campaign rows, and event history
remain immutable; no schema or history rewrite is required.

## Verification

- The same campaign root is selected with no novelty row, a historical method-1
  row, and a current `known_overlap` row.
- A failed replication delivery followed by enough successful tests can still
  enter tissue refinement.
- A failed synthesis delivery followed by the selected successful synthesis can
  still enter tissue refinement.
- A receipt-less child carrying a virtual result is rejected as corrupt.
- Observer novelty backfill can stop or restart without changing campaign selection.

## Consequences

The public console can still identify rediscovery and literature overlap, and the
authorized Europe PMC refresh can continue independently. Research lineages now
advance or stop only because of their recorded internal tests and delivery budget,
not because an external index happened to answer. Failed calls remain auditable
without becoming scientific evidence or permanently poisoning later work.
