# ADR 0021: Geographic sources require a pinned route into S2 before tiling

## Status

Accepted on 2026-08-06. This is a prerequisite contract for a real S2 elevation
normalizer; it does not itself create a normalized tile, tile-tree root, or world.

## Context

The repository now verifies NOAA ETOPO source bytes and can derive a portable global
regular grid which retains exact selected source `f32` bits. The full-Earth data model,
however, stores canonical layers in S2 cells. Turning a latitude/longitude source grid
into an S2 layer needs more than a file-format conversion: it must specify which point
or area is represented, the Earth model and vertical datum, longitude wrapping, polar
behavior, boundary ownership, resampling rule, and the exact route into CellIds.

Calling ordinary platform `sin`, `cos`, or a GIS library during normalization without
pinning those details would make the same source potentially produce a different tile
tree on another machine. Using the private ECEF-to-S2 routing proof directly is also
not enough: ETOPO coordinates are geographic, and the required geographic-to-ECEF
conversion is not yet an exported, verified deterministic boundary.

## Decision

- A canonical global layer normalizer MUST NOT emit an S2 tile, index, root, or bundle
  until it names a versioned geographic-source-to-S2 route.
- That route MUST define all of the following in canonical, testable form:
  - source sample interpretation (node versus cell area), coordinate ordering, and
    antimeridian/polar conventions;
  - horizontal frame and ellipsoid, vertical datum, and any datum transformation;
  - fixed input-coordinate representation and geographic-to-ECEF conversion;
  - the exact ECEF-to-S2 version and tie behavior;
  - tile sampling/support geometry, resampling/aggregation kernel, missing-value rule,
    and output unit/quantization policy; and
  - cross-language golden vectors covering each face, seams, poles, antimeridian, and
    cell boundaries, plus byte-identical rebuild verification.
- The grid derivation remains a valid provenance-bound intermediate. Its header binds
  source snapshot and artifact hashes, selected stride, dimensions, and raw selected
  `f32` bits; no downstream consumer may call it a canonical S2 layer without the
  route above.
- The existing fixed-point ECEF-to-S2 proof remains the required target bridge, but
  its private status is intentional until the geographic input conversion is specified
  and verified at the same rigor.

## Current implementation state

`world-domain` now exposes the first fixed-point reference for exact E7-degree WGS 84
coordinates: runtime CORDIC uses checked integer arithmetic and retained Q62 angle
constants, derives a WGS 84 ellipsoidal ECEF ray from the exact flattening rational,
and then uses the existing exact S2 bridge. The data CLI can inspect that route through
`inspect geographic-route`. This is intentionally still short of an elevation layer:
the source-grid sampling/support geometry and independent cross-language golden suite
remain required before the route is eligible for canonical normalizer output.

## Consequences

The project does not gain a deceptively plausible elevation tile tree just because an
upstream raster is available. The next data implementation task is a separately
versioned geographic-to-ECEF reference with cross-language golden vectors, followed by
a source-grid-to-S2 sampling ADR and normalizer. This preserves the central claim:
the public can reproduce an emitted layer root from stated evidence and rules, rather
than trust a host-specific GIS pipeline.
