# ADR 0016: Full-Earth bundles bind exact source snapshots

## Status

Accepted on 2026-08-06. No production full-Earth bundle exists yet, so this extends
the uncommitted schema-v2 contract before any world can depend on it.

## Context

The source-snapshot contract records exactly what was acquired before normalization.
A later world-data bundle previously retained source records and normalized tile roots,
but had no structural link to those pre-normalization manifests. That gap would allow
a bundle to claim a source family while silently normalizing different raw bytes.

## Decision

- A `NormalizationRecord` now contains sorted `source_snapshots`, each with a stable
  snapshot ID and canonical source-snapshot manifest SHA-256 digest.
- Every schema-v2/full-Earth bundle must retain at least one such reference. Its
  normalized layers must each cite one or more IDs from that declared set through
  `source_snapshot_ids`.
- Schema-v1 bounded fixtures may omit snapshot references for compatibility. If they
  use them, references still have to resolve and be ordered.
- A source snapshot is still not a world bundle. This schema relationship binds
  provenance; it does not accept raw files as layer tiles or authorize genesis.

## Consequences

The upcoming relief normalizer can bind an elevation/bathymetry root to the exact
NOAA ETOPO manifest (`etopo-2022-v1-60s-bed`) rather than a mutable dataset name.
Future climate, soil, hydrography, habitat, coastline, and taxon layers must name
their own verified source snapshots. A real bundle with missing, dangling, unordered,
or zero-digest snapshot provenance fails before configuration or genesis validation.
