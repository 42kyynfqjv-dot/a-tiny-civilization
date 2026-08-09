# ADR 0143: the live habitat is a bounded observer projection

Date: 2026-08-09

Status: Accepted

## Context

The observatory exposed population records and significant events but discarded committed movement
and primitive-action facts. The result was auditable but did not let a visitor watch life unfold.
Sending every organism to every browser would fail as populations grow and would turn observer load
into pressure on canonical history.

## Decision

Add a disposable, one-way habitat projection over committed events. It retains each organism's
current and previous geographic position, latest use-neutral primitive action, and a bounded ring of
recent public activity. It excludes bodily needs, cognition inputs, reproductive mechanisms, death
mechanisms, private memory, parentage, and birth category.

The public API has three explicit levels of detail:

- planet and region views return at most 1,024 server-generated population clusters;
- local views return at most 2,000 individual drawable entities inside explicit geographic bounds;
- recent activity returns at most 64 entries and the durable projection retains only 512.

The browser renders the result on one Canvas, interpolates only between committed positions, and
receives small polling deltas rather than canonical state. A truncated response becomes clusters or
a visibly capped view; it never expands until the browser or database is exhausted.

## Consequences

- One inhabitant and billions of inhabitants use the same interface contract.
- Observer traffic cannot alter tick rate, action selection, memory, or canonical storage.
- The habitat may be dropped and rebuilt from the event log without losing history.
- Terrain styling may make positions legible but must not claim unprojected visual detail as a world
  fact.
