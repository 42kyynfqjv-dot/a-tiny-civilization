# ADR 0029: Full-Earth terrain packs L10 values into verified L6 tiles

## Status

Accepted on 2026-08-06. This defines the canonical terrain-tile payload and storage
shape; it does not claim that a global ETOPO-derived layer root exists yet.

## Context

The full-Earth scheduler begins at S2 L10, which has 6,291,456 global cells. Storing
one filesystem artifact per L10 scalar produces millions of tiny files, slow release
validation, and an operationally fragile deployment. Collapsing terrain to L6 would
instead discard the planetary-resolution values the scheduler needs.

## Decision

- A packed scalar terrain tile is one content-addressed artifact rooted at a coarse S2
  container and contains every target-level descendant in canonical CellId order.
- V1 terrain storage uses L6 containers and L10 target values: every payload contains
  exactly 256 L10 cells. The tile tree therefore has 24,576 leaf artifacts rather than
  6,291,456 one-cell files.
- Each target value retains its support sample count and minimum, rounded mean, and
  maximum in integer millimetres. It also binds the source snapshot/artifact digests
  and selected quadrature resolution.
- The parser rejects missing, reordered, duplicate-by-substitution, unsourced, out of
  range, malformed, or non-canonical payloads. The release verifier additionally
  rejects a payload whose declared layer or container differs from its tile-tree entry.
- The media type is
  `application/vnd.atinycivilization.packed-scalar-terrain-tile+json` and schema
  version 1. It is a storage format, not a claim that all scalar fields are measured
  at L10 precision.

## Consequences

The global normalizer can emit and verify realistic release artifacts without changing
the L10 causal boundary. A terrain profile still must publish its source stride,
quadrature resolution, missing-value behavior, physical-area weighting policy, and
error disclosure. ETOPO-based tiles remain an approximation under ADR 0028, not exact
spherical clipping.
