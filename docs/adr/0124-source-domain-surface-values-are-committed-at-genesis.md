# ADR 0124: Source-domain surface values are committed at genesis

## Status

Accepted for provisional configuration schema 6. This is a private causal-input boundary, not an
agent perception or scientific admission.

## Context

Origin-environment schema 2 joins exact terrain, surface-water, and topsoil source evidence at the
seed-selected L10 patch. Keeping those values only in an external artifact would require a future
runner to reopen that artifact or choose a mapping during replay. Configuration is the immutable
tick-zero boundary from which the engine must obtain every causal input.

The sources have different epistemic status. ETOPO supplies physical millimetres at coarse support;
JRC currently supplies an occurrence palette code; SoilGrids supplies ordered source-domain
prediction quantiles. None should silently become an organism concept.

## Decision

- Configuration schema 6 adds a required private `local_surface_baseline` beside the existing
  environment and weather baselines.
- The baseline commits the origin-environment digest, exact L10 and embodied patches, terrain
  min/mean/max millimetres, the bounded JRC occurrence source code, and nine ordered SoilGrids
  Q0.05/Q0.5/Q0.95 triples.
- All soil triples must be present and monotonically ordered. The signed upstream no-data sentinel
  is rejected for a causal candidate rather than filled, averaged, or inferred.
- Schema 6 requires environment, weather, and surface inputs to bind the same evidence and active
  patches at the configured embodied level. Schemas 1 through 5 retain their exact shapes and
  explicitly reject a surface field.
- The runner constructs schema 6 only when an origin-environment schema-2 surface closure and the
  complete ERA5 weather baseline are both supplied. Archived schema-1 origin artifacts continue to
  construct schema 5.
- No rule reads the surface baseline yet. A later ruleset must separately define label-free physical
  effects and advance event, snapshot, and state-hash schemas before inhabitants can sense them.

## Consequences

The next world will commit its exact local Earth surface inputs at genesis, so later replay never
contacts raster storage and never depends on a new interpretation. This intentionally changes the
tick-zero configuration and state hash for a fresh candidate. It does not expose altitude, water
codes, SoilGrids property order, habitat classes, fertility, edibility, or affordance labels to an
inhabitant.

## Verification

Domain tests prove schema-6 round trips, rejects missing soil values and invalid ranges, preserves
the schema-5 wire contract, and enforces common spatial binding. The production qualification query
accepts both archived schema-5 and new schema-6 weather-bearing histories while continuing to
require the same physical weather perceptions.
