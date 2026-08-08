# ADR 0061: One runner holds the database canonical-writer lock

## Status

Accepted on 2026-08-08.

## Context

PostgreSQL compare-and-commit checks prevent two runners from corrupting history, but
two simultaneously started runner processes can repeatedly race. The loser may load a
world cursor while the winner is committing its tail and then fail strict replay
verification. Correctness survives; availability and operator clarity do not.

## Decision

`civilization-runner serve` must acquire one fixed PostgreSQL session advisory lock
before it records a heartbeat or loads a world. It detaches and retains a dedicated connection
for its entire process lifetime. A second runner fails startup with a clear conflict.
PostgreSQL automatically releases the lock when the connection closes or the process,
container, host, or network session fails.

The lock is operational state only. It is not a world event, has no simulation-time
meaning, and cannot affect a tick's contents. Canonical compare-and-commit checks remain
the final defense against stale state.

## Consequences

- Deployments can use restart policies without overlapping canonical writers.
- A standby becomes active only after the old database session is gone.
- The runner holds one dedicated PostgreSQL connection outside its work pool while serving.
- Horizontal execution will require a later partition-ownership protocol; it may not
  weaken canonical barrier ordering or reuse this single-writer lock as world input.
