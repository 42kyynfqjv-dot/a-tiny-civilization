# ADR 0023: ETOPO centre attribution is a reproducible intermediate, not area aggregation

## Status

Accepted on 2026-08-06. This creates an auditable input to a later S2 normalizer; it
does not create a tile, tile-tree root, world-data bundle, or world.

## Context

The verified ETOPO source supplies raw 60-arc-second area-cell values. Its exact
source support and centre route are now specified, but a later normalizer should not
need to rediscover the source coordinate lattice or reroute a raw grid differently.
At the same time, assigning a cell's entire area to its centre's S2 CellId would be a
false area-overlap algorithm.

## Decision

- `civilization-data derive etopo-centre-index` writes a new, no-replacement binary
  intermediate containing a regular source sampling in source row order.
- Every record is exactly twelve bytes: the source centre's S2 CellId as eight
  big-endian bytes, followed by the selected raw ETOPO `f32` IEEE-754 bits as four
  little-endian bytes.
- Its 88-byte header binds the schema, selected source stride, S2 level, exact source
  snapshot digest, exact source-artifact digest, and output dimensions. Reserved bytes
  are zero and are part of the canonical intermediate bytes.
- Centre routing uses the shared fixed-point WGS 84-to-S2 bridge. Coordinates originate
  only from the validated ETOPO source-cell lattice in ADR 0022.
- The command validates input shape and all coordinate axes, refuses invalid S2 levels,
  streams only from verified local source bytes, and refuses to replace an existing
  output.
- This is an attribution index, not a source-area ownership map. A canonical
  elevation/bathymetry normalizer MUST apply an explicitly versioned target-support
  aggregation kernel; it cannot call this index a tile or use centre attribution as
  evidence of whole-cell containment.

## Verification

The encoder test builds an actual one-degree global sampling, checks the bound digests,
dimensions, first independently routed CellId, and raw `f32` bits. A run against the
pinned source produced a 64,800-record index with digest
`a6c057eea1d2f7dcc339f80354c81630c3be941e16d308931a4c52f5dd5cbea8`.

## Consequences

Source routing becomes a concrete, reproducible artifact rather than an implicit loop
inside a future normalizer. The project remains honest about the unsolved work: target
cell coverage and area-weighted aggregation are still required before any canonical
relief root can exist.
