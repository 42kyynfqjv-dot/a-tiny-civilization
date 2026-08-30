# ADR 0161: Cancer tissue refinement is a singleton durable worker

Status: accepted for production wiring.

## Context

ADR 0158 defines a deterministic resource-bounded tissue engine, but a pure
function is not enough for production. Durable selection must prove that a
campaign truly survived, freeze the exact inputs before execution, prevent two
heavy jobs from sharing the host, and make a crash safe to retry.

## Decision

PostgreSQL is the tissue worker's authority. It admits a protocol only when the
complete durable lineage contains exactly one current method-2 supporting root,
three to five completed and distinct independent-replication experiments, no
falsifying result, and exactly one successful synthesis directive whose computed
outcome is `survived_replication_round`. The synthesis request and result hashes
are now part of the method-1 protocol alongside every experiment hash.

The adapter loads every request in the lineage. Missing results, legacy virtual
methods, partial JSON/checksum pairs, an unfinished request, an extra synthesis,
an unexpected task, omitted virtual results, or counts that disagree with the
synthesis fail closed. It derives and appends the protocol before claiming it.
Protocol provenance is immutable.

A partial unique index and a transaction-scoped advisory lock permit one live
tissue claim across all worlds. Claim ownership consists of a bounded worker ID,
an unpredictable claim token, and a database-time lease. Expired operational
claims may be reclaimed; their protocol and claim count remain. Completion
validates the result, appends it, and releases the lease in one transaction.
Exact completion retries succeed without rewriting bytes; conflicting bytes are
corruption.

The worker is a separate runner subcommand with `--once` support and bounded
polling. Its production systemd unit permits one CPU, 1.5 GiB of memory, no swap,
32 tasks, no filesystem writes, no evidence datasets, no model or Hindsight
credentials, and no non-loopback IP access. Loopback is allowed only so the
worker can reach the host PostgreSQL listener. Both systemd and Compose cap the
worker process at 30 minutes. SIGTERM permits an ordinary bounded calculation to
commit atomically; a still-wedged process is killed 30 seconds later, leaving no
result and an expiring database lease that another process can safely reclaim.
The canonical runner never executes tissue work.

The worker polls continuously, so migration 0065 gives completed campaign
synthesis requests a narrow partial index. This is only an operational access
path: campaign survival remains reconstructed and revalidated from immutable
request, result, and virtual-experiment evidence before any protocol is
admitted. The index cannot award survival or change research history.

Tissue protocols or results are not research evidence documents. They are not
written to Hindsight, research prompts, the campaign scheduler, canonical world
events, or a clinical vocabulary. They remain observer-side uncalibrated model
projections.

## Verification

- A complete survivor is admitted, claimed, executed, completed, and exactly
  replayable after reconstructing the candidate from durable rows.
- Concurrent workers cannot both claim, even across worlds.
- Expired leases may be reclaimed; stale claim tokens cannot complete or fail a
  job.
- Partial, falsified, legacy, incomplete, duplicated, or count-mismatched
  campaign rows never create a protocol.
- Protocol/result rows reject mutation and mismatched payload columns.
- An exact completion retry does not duplicate or mutate a result.
- Boundary checks prove that the worker has no external-model, memory, or
  qualification-data capability.
