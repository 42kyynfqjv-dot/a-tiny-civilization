# ADR 0159: Indeterminate Fireworks costs require provider-export proof

Status: accepted for Cancer World.

## Context

The research worker records a paid dispatch before making the external request.
If the process then loses the response, the reservation correctly becomes
`indeterminate`: releasing it would risk exceeding the Cancer World hard stop,
while retaining the entire conservative reservation can eventually strand money
that the provider did not charge.

Fireworks exposes authoritative per-request timestamps, models, prompt tokens,
and completion tokens through `firectl billing export-metrics`. The export does
not contain A Tiny Civilization's request UUID, so reconciliation cannot rely on
an asserted operator mapping.

## Decision

An operator may supply one unmodified monthly Fireworks billing CSV to the
offline runner command. The importer:

- hashes the complete export bytes and each exact raw matched CSV record;
- recognizes only the documented names or a small versioned alias set for the
  five security-relevant fields, requiring exactly one timestamp, usage-type,
  model, prompt-token, and completion-token field; unrelated provider metadata
  columns remain ignored and are never retained;
- considers only `TEXT_COMPLETION_INFERENCE_USAGE` for the pinned
  `accounts/fireworks/models/gpt-oss-20b` route;
- requires exactly one row within five seconds of every unreconciled, immutable
  paid Fireworks dispatch and refuses missing, ambiguous, or reused rows;
- recomputes cost at the same pinned tariff as the runtime receipt adapter:
  $0.07 per million input tokens and $0.30 per million output tokens, rounded up
  to a micro-dollar; and
- defaults to verification without writes. A separate explicit confirmation
  flag appends evidence.

The original dispatch and `indeterminate` reservation are never changed or
deleted. The reconciliation is a separate append-only record containing the
request and route, whole-export hash/byte length, exact-row hash/byte range, both timestamps, token
counts, actual cost, original reservation, and released difference. Within the
same database transaction, the mutable monthly aggregate moves the original
reservation out of `reserved_micro_usd`, moves verified actual cost into
`spent_micro_usd`, and thereby restores exactly `reserved - actual` capacity.

An account-level transaction lock serializes importers. An exact retry is a
read-only success; a different retry conflicts. Database triggers repeat the
route, timestamp-cardinality, tariff, reservation, and aggregate checks rather
than trusting the CLI.

## Consequences

- Provider evidence can recover conservatively stranded capacity without
  rewriting history.
- No email or other account identity from the CSV is retained in PostgreSQL;
  the whole-file length and exact row byte range make its hashes independently
  reproducible from retained operator evidence.
- A missing export row remains reserved. An ambiguous timestamp window requires
  a future stronger provider identifier; it is never guessed.
- The tariff is intentionally duplicated in a migration constraint and the
  shared application function. Any price change requires a new versioned method
  rather than silently repricing historical calls.

## Verification

- Unit tests cover the documented CSV, observed header aliases, quoted fields,
  missing/unknown/duplicate headers, ambiguous rows, exact byte hashes, pricing,
  and the five-second boundary.
- PostgreSQL integration tests prove an exact retry does not adjust aggregates
  twice, conflicting retries fail, the original reservation remains immutable,
  and the reconciliation cannot be updated or deleted.
- The operator first runs the command without confirmation and reviews only
  counts, micro-dollar totals, and the whole-export hash before appending.

Fireworks export format reference:
<https://docs.fireworks.ai/accounts/exporting-billing-metrics>.
