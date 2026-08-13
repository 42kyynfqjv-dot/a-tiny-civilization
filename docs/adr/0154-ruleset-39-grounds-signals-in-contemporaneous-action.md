# ADR 0154: Ruleset 39 grounds signals in contemporaneous action

## Status

Accepted and implemented on 2026-08-13. It is the default for newly created ordinary
worlds. Existing worlds retain the ruleset committed at genesis; Cancer World remains
the explicit ruleset-38 experiment.

## Context

Ruleset 36 gave every one of 32 acoustic forms an independent action candidate. The
family therefore dominated motor selection. Echoed calls could also reinforce a
private association from one signal to another signal even when no external behavior
was shared. The observer correctly refused to call that language, but the canonical
mechanics produced a large volume of circular calls and weak behavioral grounding.

Changing those mechanics inside an existing world would make identical historical
inputs produce different state. A replay-safe correction requires a new ruleset and
event schema.

## Decision

- Schema 38 adds `OrganismSignalEmitted`, a physical acoustic occurrence independent
  of the organism's primitive motor action.
- Ruleset 39 chooses a real motor action first. It then treats all 32 forms as one
  normalized signal family for occurrence probability and selects a form inside that
  family only after a signal occurs.
- A signal is emitted alongside the producer's same-tick motor action. A listener may
  update a signal-action association only from a directly heard human source and that
  contemporaneous action; signal-to-signal echo is not admitted as a semantic target.
- Equal-distance recipients are rotated by a deterministic seed/source/tick digest so
  the bounded acoustic fan-out does not permanently favor low entity IDs.
- Explicit experiment worlds do not activate this driver. Cancer World keeps its
  committed ruleset-38 behavior and replay contract.
- New ordinary-world tooling defaults to ruleset 39. Older histories remain unchanged.

## Consequences

Ruleset 39 makes language possible rather than guaranteed. Calls can still fail to
stabilize, conventions can decay, and no label or meaning is supplied to inhabitants.
The public detector remains an observer-only inference over committed evidence.

Replay verifies adjunct emission, same-tick grounding, bounded recipient rotation,
and exact isolation from legacy and Cancer World drivers.
