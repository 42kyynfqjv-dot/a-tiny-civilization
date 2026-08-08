# ADR 0089: Pre-genesis qualification is one fail-closed report

## Status

Accepted.

## Decision

`scripts/qualification-status.sh WORLD_ID` is the bounded-world evidence gate before a public
genesis is authorized. It first performs exact canonical replay with the runner and then emits one
JSON object derived directly from PostgreSQL. A successful report requires:

- a running ruleset-18 world with at least 1,000 ticks of history;
- contiguous canonical batches through the durable cursor and at least one valid snapshot;
- all four public projections exactly current;
- a nonempty, fully delivered Hindsight memory outbox with no recorded delivery errors;
- at least one due cognition request, no missing due latch or consumption, and at least one real
  recall/result path exercised;
- nonempty organism, timeline, and deterministic-finding projections.

Future cognition requests are reported but are not failures. Their deadlines have not occurred in
simulation time, so treating them as overdue would make the gate depend on wall-clock timing.

The script exits nonzero when replay fails, the world is absent, or any reported check is false. It
does not advance the world, run projections, deliver memory, or contact a model provider; those are
separate qualification actions whose durable results it inspects.
The database URI remains in the protected parent environment: the script decomposes it into libpq
environment settings and removes `DATABASE_URL` from the `psql` child rather than exposing the URI
in a process argument. One optional `sslmode` query parameter is supported; other connection
parameters fail closed.

## Consequences

Pre-genesis readiness is no longer inferred from several successful terminal commands or prose
notes. The complete report can be retained with launch evidence and independently recomputed from
the disposable qualification database. It is deliberately stricter than production liveness,
which checks continuously running services rather than historical coverage.
