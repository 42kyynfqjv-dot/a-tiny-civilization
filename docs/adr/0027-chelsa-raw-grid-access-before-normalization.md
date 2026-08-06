# ADR 0027: CHELSA raw-grid access precedes climate interpretation

## Status

Accepted on 2026-08-06. This defines an auditable inspection boundary for one retained
climate source. It does not define climate units, missing-value treatment, spatial
aggregation, a normalized layer, or canonical genesis.

## Context

CHELSA-BIOCLIM+ v2.1's pinned January 1981–2010 artifact is a 20,880 × 43,200
NetCDF raster with `lat`, `lon`, `Band1`, and `crs` variables. A later normalizer must
not inherit an opaque GIS tool's interpretation of its coordinate order, floating-point
values, missing values, or physical units. Nor may a convenient cell lookup be mistaken
for an S2 aggregation policy.

## Decision

- `civilization-data inspect chelsa-january-temperature` verifies the complete pinned
  source snapshot through the portable Rust reader, requires the exact retained grid
  shape, and reports source/coordinate fingerprints.
- `civilization-data inspect chelsa-january-cell` accepts only a zero-based bounded
  `(row, column)` address in the retained axis order. It verifies the entire snapshot
  before returning source hashes, raw latitude/longitude `f64` IEEE-754 bits, and raw
  `Band1` `f32` IEEE-754 bits.
- These commands return raw bits, rather than a converted temperature or a geographic
  S2 address. The source's units, fill-value behavior, raster support, longitude seam,
  land/ocean coverage, and target-cell overlap policy remain explicit work for a
  versioned normalizer.
- No runner or genesis configuration may depend on either inspection command's output.

## Verification

The data-crate test suite rejects out-of-range source addresses. Running the cell
inspector against the retained artifact at row 10,440 and column 21,600 produces a
source-bound raw record with no host GIS dependency. The command is documented beside
the source inspection workflow.

## Consequences

The project can examine exact, real climate bytes without pretending a January normal
is a usable planetary climate model. The next climate milestone must retain enough
variables and periods, then specify fixed unit and missing-value treatment plus an S2
target-support aggregation rule before emitting any climate tile or root.
