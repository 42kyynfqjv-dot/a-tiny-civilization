# ADR 0123: Origin surface evidence is joined before causal interpretation

## Status

Accepted on 2026-08-08 for provisional origin-environment schema 2. This is an evidence boundary,
not scientific admission or an agent-facing mechanic.

## Context

The provisional full-Earth composition already pins complete global L10 releases for ETOPO 2022
bedrock relief, JRC Global Surface Water occurrence source codes, and nine SoilGrids 0–5 cm
properties with three prediction quantiles. The canonical origin artifact previously joined only
Copernicus land cover and CHELSA temperature. Later mechanics could therefore reach the global
roots but could not prove which exact terrain, water, and soil cells were selected at genesis.

Interpreting a JRC palette code as drinkable water, a SoilGrids property as an agent concept, or an
ETOPO mean as embodied microterrain would add scientific and behavioral assumptions. The source
join must precede those choices.

## Decision

- Origin-environment schema 2 requires one `local_surface` object bound to the same selected L10
  patch as land cover and temperature.
- The object retains the exact terrain min/mean/max and support count, JRC scalar source code and
  declared source unit, and all nine ordered SoilGrids Q0.05/Q0.5/Q0.95 values and support count.
- Every local value is bound to its independently verified global root and exact containing tile
  digest. Soil evidence additionally retains the ordered property-source set digest, depth, and
  sampling/reprojection method.
- Derivation resolves all five releases from the committed composition and fails closed on a
  missing layer, changed root, unexpected layer identity or level, absent origin cell, invalid
  source-code range, or noncanonical soil source set.
- Schema 1 remains byte-readable for archived worlds. It cannot masquerade as schema 2: schema 1
  forbids `local_surface`, while schema 2 requires it.
- No surface value is yet placed in perception, action selection, energetics, hydrology, ecology,
  or observer interpretation. Those require separately versioned causal mappings.

## Consequences

The canonical origin now has one provenance-complete local Earth evidence closure rather than
three unjoined global roots. This enables later terrain, surface-water, and soil mechanics without
rerunning or silently choosing source cells. It does not claim that L10 relief is local slope, that
a zero JRC occurrence code means no accessible water, that topsoil quantiles are mutually coupled,
or that any value implies habitat suitability, edibility, fertility, or use.

## Verification

Domain tests cover legacy byte compatibility, current-schema completeness, spatial binding,
ordered SoilGrids provenance, scalar bounds, and canonical round trips. The data tool successfully
derived schema 2 for the immutable canonical origin `8683550000000000`: 496 ETOPO support samples
produce a 2,296,048 mm mean bedrock relief value, the retained JRC source code is 0, and every one
of the nine SoilGrids property vectors has one retained source sample.
