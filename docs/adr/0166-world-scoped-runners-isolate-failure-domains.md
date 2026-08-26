# ADR 0166: world-scoped runners isolate canonical failure domains

Date: 2026-08-26

Status: Accepted

## Context

One runner process iterated every running world under a database-wide writer lease.
That guaranteed one canonical writer, but it also coupled availability: a fail-closed
transition in Cancer World exited the process before the ordinary world received its
turn. No invalid history committed, yet both worlds stopped advancing.

## Decision

Production runs one process per world using the same runner binary:

- `runner` is pinned to the current ordinary-world ID;
- `cancer-runner` is pinned to `CANCER_WORLD_ID`;
- each process loads, verifies, schedules, and advances only its configured world;
- PostgreSQL grants a shared all-runner guard plus an exclusive deterministic
  advisory lock derived from the world ID;
- two scoped runners may hold different world locks concurrently, while duplicate
  writers for one world and a simultaneous legacy all-world writer fail closed;
- production refuses to start an unscoped runner;
- ordinary and Cancer runners publish distinct heartbeat identities.

PostgreSQL, canonical event tables, replay code, and the application image remain
shared. This is process and lease isolation, not a fork of simulation semantics.

## Consequences

- An integrity failure, replay cost spike, or crash in one world no longer stops the
  other world.
- Each world can later receive a different wall-clock cadence or resource limit
  without changing its simulation-time rules.
- Successor cutover must update the ordinary runner's pinned ID.
- Operators can diagnose and restart a failed world independently.
- Database failure remains shared infrastructure and is not addressed by this ADR.
