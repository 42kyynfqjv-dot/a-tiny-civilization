# ADR 0145: neutral signal imitation and contextual reuse

Date: 2026-08-09

Status: Accepted

## Context

Ruleset 33 supplies 32 physically distinct signal forms and lets an organism privately associate a
heard form with a directly witnessed next motor action. The live world demonstrated abundant sound
production without a qualifying shared convention. The association could bias a listener's later
action, but the policy provided only weak routes for copying the physical form or producing a
learned form again under a similar embodied action pressure.

Adding words, referents, intentions, communicative goals, or scheduled convergence would manufacture
language. Leaving forms almost entirely disconnected from vocal production would instead make the
documented possibility of convention mechanically implausible.

## Decision

Ruleset 34 adds two stateless deterministic policy weights over existing canonical state:

- an organism is more likely, but never required, to emit the exact physical form it directly heard
  during the current perception interval; and
- an organism is more likely, but never required, to emit a form it has privately associated with
  the non-signal motor action most strongly weighted by its present bodily context.

The weights use only direct sound perception, existing private signal-action associations, current
use-neutral action candidates, and fixed integer arithmetic. They add no new state or events and no
form begins with a meaning. All 32 forms remain available, ordinary action selection can override
both weights, and the public detector in ADR 0144 remains unchanged.

The running public ruleset-33 world activates this stateless driver when its committed state reaches
tick 65,000. Ruleset-33 states through tick 64,999 retain the previous policy version and therefore
replay byte-for-byte. The boundary is source-pinned and public; ruleset 34 uses the driver from
genesis. Event, state-hash, and snapshot schemas remain at version 32 because no canonical shape
changes.

## Consequences

- Vocal imitation and conventional reuse become physically plausible without guaranteeing language.
- Repetition alone still cannot satisfy the public language detector.
- The live world continues without replacement, hidden intervention, or retroactive mutation.
- Replay requires source that knows the disclosed tick-65,000 compatibility boundary.
- Observer-facing activity calls the raw event a “call”; exact form numbers remain available in the
  research archive if evidence eventually qualifies.
