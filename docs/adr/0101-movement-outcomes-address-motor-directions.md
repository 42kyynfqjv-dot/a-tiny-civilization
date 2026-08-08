# ADR 0101: Movement outcomes address private motor directions

Accepted on 2026-08-08 as ruleset 24.

## Context

Ruleset 23 makes four adjacent movement directions selectable, but the existing action-learning
state addresses only the generic `move` primitive. Consequently, experience from one direction
changes the weight of every direction equally. The organism can select a direction but cannot learn
which local motor coordinate previously reduced or increased its total bodily pressure.

## Decision

Each organism may retain at most four private movement-direction values, one for each adjacent motor
coordinate. A move updates exactly one value using the same bounded bodily-pressure delta as generic
action learning. The value weakly adjusts only the matching future move candidate. Planning and
commit independently require one correctly ordered value update for every ruleset-24 move.

The state is life-local and non-heritable. Public projections explicitly discard it. Direction
indices remain label-free bodily coordinates: no compass heading, destination, route, place name,
map, or scientific interpretation enters canonical cognition.

## Consequences

Ruleset-24 event schema 25 and snapshot schema 25 preserve exact replay while older histories remain
byte-identical. Fresh genesis and qualification require evidence that the direction-specific learning
path executed. This provides the minimal feedback loop needed for repeated local navigation without
supplying any navigational concept.
