# ADR 0100: Selectable movement is a motor coordinate

Accepted on 2026-08-08 as ruleset 23.

## Context

Earlier rulesets let an organism choose `move`, but the engine selected the adjacent direction only
afterward from a seed draw. No organism could intentionally repeat or alter a direction through
experience or bounded cognition. That prevents navigation in principle. Supplying maps, named
destinations, routes, or goals would cross the observer boundary and script spatial knowledge.

## Decision

Ruleset 23 gives `move` four otherwise-equivalent motor candidates numbered zero through three.
Each index selects one member of the existing deterministic S2 edge-neighbor ordering. It is a
body-owned immediate direction, not north/east/south/west, a geographic coordinate, place, path,
or destination.

A bounded cognition result may optionally select `movement_direction` from zero through three only
with `move`. The usual small weight bonus applies only to that candidate. Baseline exploration
remains nonzero and no result can create a move or choose a nonadjacent patch.

## Version and enforcement

Ruleset 23 uses event schema 24 and policy draw version 8. State and snapshot schema remain 24
because the resulting embodied patch was already canonical state. Ruleset 22 and earlier histories
retain their prior seeded direction draw and exact bytes.

Planning and commit independently require every ruleset-23 move action to carry one valid direction
and exactly one later relocation from the organism's current patch to that indexed adjacent patch.
Missing, duplicate, reordered, nonadjacent, wrong-direction, or fabricated relocations fail before
commit. The model adapter advances to `openai-compatible-bounded-cognition-v4`; historical receipts
omit the optional field unchanged.

During this change, testing exposed that the final policy step was overwriting ruleset-21 selected
signal intensity with the older generic intensity draw. Ruleset 21+ now preserves the selected
acoustic candidate, while ruleset 20 and earlier retain their historical generic draw.

## Consequences

Agents can now exert directional control without possessing a map. Direction-specific outcome
learning, landmarks, trails, route memory, and return behavior remain separate emergent substrates;
none is inferred merely from movement.
