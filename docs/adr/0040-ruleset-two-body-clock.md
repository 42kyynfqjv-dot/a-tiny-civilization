# ADR 0040: Ruleset two executes a durable internal body clock

## Status

Accepted on 2026-08-07 for new provisional worlds. Ruleset one remains permanently
supported for replay.

## Context

The original full-Earth scheduler was deliberately empty. It proved queue ordering and
barrier safety but did not execute any life-owned transition. That was the right
ruleset-one boundary; retroactively populating its queue would change archived history.

The next execution layer must be real enough to prove per-organism partition work,
event ordering, storage, snapshots, and replay, without pretending that unreviewed
metabolism, locomotion, reproduction, weather, or ecology is scientifically admitted.

## Decision

- New worlds use ruleset version 2. Ruleset one preserves its empty full-Earth queue,
  event schema, snapshot schema, and hash behavior.
- Every living full-Earth organism in ruleset two owns exactly one scheduled body-clock
  work item for the next tick, routed by its durable embodied S2 patch.
- A completed partition barrier emits exactly one `organism_age_advanced` event per
  scheduled organism. It advances a checked unsigned tick count by one; overflow,
  missing age state, unknown work subjects, incomplete outputs, and budget overflow
  fail the complete transition.
- The event uses schema v6; ruleset-two state hashes and snapshots use schema v6.
  Genesis, stored history, replay from genesis, and snapshot-plus-tail replay must all
  agree on the resulting bytes.
- The event is internal causal bookkeeping. Public timeline, organism, and finding
  projections ignore it. It does not expose an organism's sensitive life details or
  create a public age feed.
- Ruleset-two age state is optional in the serialized organism wire shape solely so
  ruleset-one snapshots retain their published representation. It is mandatory for
  every ruleset-two scheduled organism.

## Deliberate non-decisions

- One age tick is not a biological age model. It creates no energy demand, fatigue,
  mortality, fertility, learning, movement, weather, material change, or ecological
  effect.
- It does not establish a simulation epoch or invoke the retained celestial data.
- It does not make the provisional composition scientific evidence, authorize a
  canonical public world, or select a canonical seed.

## Consequences

The system now proves that all living embodied organisms have deterministic future work
and that that work produces durable, replayable causal state. Physiology, environment,
locomotion, reproduction, fauna behavior, and cognition can add effects to this same
barrier without weakening its partition or observer-separation guarantees.
