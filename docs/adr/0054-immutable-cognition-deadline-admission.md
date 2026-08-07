# ADR 0054: Cognition enters history only through an immutable deadline latch

## Status

Accepted and implemented on 2026-08-07 for ruleset 16.

## Context

Recording a model response is insufficient if queue latency decides the tick where it
enters history. A response can also race the simulation runner, arrive after a timeout,
or return after a paid request has already incurred cost. Replay must not reproduce any
of those wall-clock races.

## Decision

- A canonical selection fixes its deadline at 60 simulated ticks. Infrastructure may
  work only inside that window; it cannot extend or pause simulation time.
- Before the deadline transition is planned, PostgreSQL locks the world and every due
  request and creates one immutable `CognitionDeadlineInput`. It contains either an
  exact validated result with provenance hashes or an explicit unavailable reason.
- Repeating latch creation after a crash returns byte-identical input. The world commit
  must consume every exact latch for that sequence atomically; omitted, added, or
  altered inputs fail.
- The simulation applies model output only as a fixed bonus to an already available,
  use-neutral primitive action. Local deterministic policy remains authoritative when
  the result is absent or invalid.
- Provider and Hindsight calls never occur during replay. Replay reads only the
  recorded `CognitionInputRecorded` event.
- Network dispatch is durable before the HTTP call. A response that arrives after the
  latch may complete the operational attempt and settle an already authorized charge,
  but cannot create a cognition result or replace the latched local fallback.
- If a worker crosses the deadline with an unresolved paid dispatch, its reservation
  becomes billing-indeterminate rather than being falsely released. A reserved call
  that was never dispatched is released.
- Runner subject selection is derived from the world seed, tick, and sorted living
  identities. Queue order, observer traffic, and provider availability cannot choose
  who receives the next opportunity.

## Consequences

Live history is bit-for-bit replayable given its recorded external-input log. Provider
latency affects only whether a bounded suggestion is available by the published
simulated-time deadline; it cannot shift later history, hold a birth, or silently alter
the clock. Operational attempt and billing records remain auditable without becoming
canonical world state.
