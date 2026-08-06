# ADR 0028: ETOPO overlap approximation begins with a fixed interior quadrature

## Status

Accepted on 2026-08-06 as the v1 terrain-support policy. It does not yet emit a
normalized terrain tile, tile-tree root, world-data bundle, or canonical world.

## Context

ETOPO 2022 values describe 60-arc-second geographic area cells, while the simulation's
full-Earth contract uses S2 cells. A source-cell centre is useful attribution evidence
but loses boundary-crossing support. Exact spherical intersection remains expensive and
must not be quietly replaced by a host GIS operation.

## Decision

- The data CLI exposes `inspect etopo-cell-quadrature`. For an ETOPO source row and
  column, it places `n × n` equal interior sample points on ETOPO's exact
  half-arcsecond lattice, where `n` is a nonzero divisor of 60.
- Point coordinates are `(2i + 1) * 120 / (2n)` half-arcseconds from the source
  cell's south/west boundary. They are strictly inside the source cell, route through
  the pinned fixed-point WGS 84-to-S2 bridge, and aggregate in canonical CellId order.
- Every emitted target count is an equal-weight sample count. It is explicitly an
  approximation to source-cell overlap, not exact spherical clipping, equal physical
  area, or a replacement for a vertical/horizontal datum policy.
- V1 canonical relief normalization will use this fixed, deterministic interior
  quadrature rather than exact spherical source-cell clipping. Its selected sampling
  profile and error disclosure are part of the published layer manifest. Exact
  clipping is deferred as a possible future, versioned policy revision; it is not a
  prerequisite for the first canonical world.
- A one-point quadrature must equal the existing exact source-centre route. Tests also
  require exact lattice divisibility and conservation of the declared point count.

## Consequences

The project has a portable, auditable overlap-approximation primitive with no platform
trigonometry or GIS dependency. A later terrain normalizer must still select a
quadrature resolution, source stride, target level, physical-area weighting or its
alternative, missing-value rule, quantization, global-coverage policy, and error
disclosure before it may claim a canonical terrain layer. Exact clipping remains a
possible future policy revision rather than a hidden promise.
