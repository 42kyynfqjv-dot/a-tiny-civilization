# ADR 0150: Active ordinary-world memory delivery is prioritized

Status: Accepted on 2026-08-11.

## Context

Every subjective memory is durably inserted into the local append-only outbox before Hindsight
delivery. Cancer World can produce a large perception backlog, while a newly created ordinary world
needs recent accepted memories before its scheduled cognition turns can recall prior experience. A
single global oldest-first claim therefore allowed an experimental backlog to make the public world
memory-poor even though both worlds used correctly isolated banks.

## Decision

The delivery worker first claims the oldest eligible row belonging to the newest running world whose
manifest has no experiment commitment. If that world has no eligible row, it falls back to the
global oldest-first queue. A partial world-scoped index keeps that lookup bounded.

This is delivery scheduling, not simulation input. No retained memory is deleted, rewritten, or
made visible across worlds. Hindsight bank IDs remain world-and-organism scoped, and every recall
outcome is still recorded before it can affect canonical history.

## Consequences

- A retired or experimental backlog cannot starve the live observatory world.
- Experimental and archived deliveries continue whenever the preferred queue is empty.
- Sustained production above adapter throughput remains visible as an outbox backlog; prioritization
  is not represented as proof that the adapter can support unbounded population scale.
- A PostgreSQL integration test proves that a successor memory is claimed before an older retired-
  world memory.
