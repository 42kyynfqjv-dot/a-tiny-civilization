# ADR 0022: ETOPO source cells retain exact area support before S2 aggregation

## Status

Accepted on 2026-08-06. This fixes the support geometry of one pinned relief source;
it does not emit an S2 tile, root, bundle, or world.

## Context

NOAA ETOPO 2022 v1 bedrock relief is a global 60-arc-second `Area` raster. Its `z`
values describe source cells, not point observations. Replacing each cell with a
rounded decimal-degree point, or treating its centre address as the cell's complete
S2 support, would lose the very geometry a later elevation/bathymetry aggregation must
account for.

The pinned NetCDF has 10,800 rows south-to-north and 21,600 columns west-to-east.
Its first centre is latitude `-647940`, longitude `-1295940` in half-arcseconds; each
axis advances by 120 half-arcseconds. The source boundaries are therefore exactly
representable without host floating point.

## Decision

- An ETOPO source cell is the geographic rectangle with boundaries:
  - `south = -648000 + 120 * row`;
  - `north = south + 120`;
  - `west = -1296000 + 120 * column`; and
  - `east = west + 120`,
  all in half-arcseconds.
- Rows are south-to-north; columns are west-to-east. Bounds are south/west inclusive
  and north/east exclusive. The north-pole boundary is the terminal zero-measure edge;
  the `+180` longitude edge wraps to `-180`.
- The centre is derived only from those bounds: `(south + 60, west + 60)`. The
  half-arcsecond source centre is the one routed through the fixed-point WGS 84 ECEF
  to S2 contract for diagnostic attribution.
- A centre's S2 address is **not** a declaration that the full area cell belongs to
  that S2 cell. A canonical normalizer must still implement a separately versioned
  area-overlap or declared aggregate-support kernel, missing-value rule, vertical-unit
  treatment, output quantization, and byte-identical rebuild proof.
- `civilization-data inspect etopo-cell-route` exposes the source centre, all four
  exact support bounds, and diagnostic S2 address. It performs no source-value
  aggregation and must not be used as a canonical tile writer.

## Verification

Before inspection or derivation, the data CLI reads all pinned `lat` and `lon` values
through the portable NetCDF reader and requires each one to quantize to its expected
half-arcsecond lattice position. The CLI and tests also prove the first and last cells,
including polar and antimeridian edges, have the declared centres and bounds.
Out-of-range row/column inputs fail closed. The existing fixed-point
geographic-to-S2 verifier remains the independent proof of centre routing.

## Consequences

The project now has a precise, reproducible answer to what one ETOPO value spatially
means. It deliberately does not shortcut the difficult next step: choosing and proving
how a set of differently shaped source areas contributes to a curved S2 target cell.
