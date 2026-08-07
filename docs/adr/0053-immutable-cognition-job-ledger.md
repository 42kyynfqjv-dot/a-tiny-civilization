# ADR 0053: External cognition uses an immutable PostgreSQL job ledger

## Status

Accepted on 2026-08-07. Atomic request insertion, exclusive request leases, and the
complete storage schema are implemented. Stepwise attempt/result/cost/latch methods
and the worker remain the next checkpoint.

## Context

Calling a provider and writing its result afterward leaves a crash window: the call
may be charged or succeed while the durable history says it never happened. Creating a
job as a runner-side effect can likewise diverge from the canonical selection event.
Retries, free-tier quotas, paid authorization, and deadline decisions need one durable
ordering boundary without turning wall-clock worker state into simulation state.

## Decision

- PostgreSQL migration 0011 adds request, normalized recall, route-attempt, result,
  monthly cost-account, paid-reservation, deadline-latch, and latch-consumption tables.
- `WorldStore::commit_transition` inserts each `cognition_requests` row directly from
  its `CognitionRequestSelected` event in the same transaction as the event batch,
  snapshot, cursor, and projection outbox.
- Request provenance and payload are immutable. Only wall-clock lease, retry time,
  claim count, and bounded diagnostic text may change.
- Workers claim requests with `FOR UPDATE SKIP LOCKED`. A lease can change throughput,
  but cannot change the selected agent, inputs, request identity, or simulation
  deadline.
- Route-attempt rows form a durable completed prefix. A network route must be inserted
  as dispatched before its HTTP call. Database triggers cap actual dispatches at 16,
  reject gaps, and forbid attempts after success or the attempt-limit terminator.
- Normalized recall outcomes, final ladder results, deadline latches, and latch
  consumptions are append-only. Terminal route attempts are immutable.
- Paid capacity uses integer micro-USD. A monthly account enforces
  `reserved + spent <= hard stop`; one request can own at most one reservation, whose
  state moves once from reserved to settled, released, or indeterminate.
- Deadline latches are immutable and separate from their eventual canonical event
  consumption. A crash after latching therefore returns the same bytes on restart.

## Consequences

Canonical selection and durable work creation can no longer split. The schema can
represent every skipped route, dispatched call, response, charge, absence, and replay
input without raw credentials or provider prose. No provider is invoked yet: the
stepwise store methods, worker, canonical result event, and deadline consumption tests
must land before credentials are used.
