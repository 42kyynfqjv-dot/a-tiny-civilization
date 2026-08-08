# ADR 0126: Topsoil coarse fragments affect private movement load

## Status

Accepted for provisional ruleset 30. The quantitative mapping is an explicit engineering
assumption pending the integrated scientific-validation pass.

## Context

Configuration schema 6 commits nine ordered SoilGrids 0–5 cm property vectors, each retaining the
source Q0.05, Q0.5, and Q0.95 values. Ruleset 29 deliberately left every soil property causally
unread. A first soil effect must use a physical source dimension, preserve replay, avoid turning a
database property name into inhabitant knowledge, and avoid claiming that one L10 regional value is
a measured L23 footpath.

The schema-pinned third property is SoilGrids coarse-fragment volumetric content (`cfvo`) in cubic
centimetres per cubic decimetre. Its source domain therefore spans 0 through 1,000. The committed
canonical cell has Q0.05/Q0.5/Q0.95 values 0/30/574.

## Decision

- Ruleset 30 requires the same validated configuration-schema-6 surface baseline as ruleset 29.
- A move first receives the existing ETOPO relief adjustment. It then receives an additional
  `terrain_adjusted_load × cfvo_Q0.5 / 1,000`, with integer floor rounding and checked arithmetic.
- The median must be within 0 through 1,000. Negative values, impossible volume fractions, missing
  surface configuration, and arithmetic overflow fail before a world can advance.
- For a one-million-unit pre-terrain exposure at the canonical origin, terrain produces 1,136,086
  units and the retained median adds 34,082, producing exactly 1,170,168 units.
- The effect changes only the private `OrganismNeedsChanged` outcome. No soil, stone, roughness,
  difficulty, route, direction, property order, or source-unit reading is emitted.
- Event, snapshot, and state-hash schemas advance to 30 even though event payload shapes remain
  unchanged. Ruleset-29 and older histories preserve their exact bytes and mechanics.
- JRC surface-water codes and the other eight SoilGrids properties remain committed but causally
  unread. This rule does not infer water access, drainage, vegetation, fertility, food, or habitat.

## Consequences

Two independent real-Earth surface domains now affect history through private bodily consequences.
The factor is intentionally regional and provisional: it does not simulate gait, substrate moisture,
particle geometry, footwear, animal-specific locomotion, or a directional path. Validation may
replace the mapping only in a successor ruleset/world; it may not tune a running history.

## Verification

Tests pin the source-unit scale, the canonical median calculation, invalid medians, stacked bodily
movement cost, ruleset/event/snapshot/state-hash version 30, snapshot integrity, replay schema
selection, default provisional initialization, and qualification-schema enforcement.
