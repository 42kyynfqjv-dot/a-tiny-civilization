# ADR 0024: ETOPO centre summaries are fixed-point source quadrature, not terrain tiles

## Status

Accepted on 2026-08-06. This adds a reproducible, provenance-preserving summary over
a verified ETOPO centre-attribution index. It does not create a canonical relief
layer, an S2 tile tree, a bundle, or a world.

## Context

The ETOPO centre index from [ADR 0023](0023-etopo-s2-centre-index.md) is intentionally
only an attribution stream: a selected source value and its exact S2-routed centre.
Later data work needs to consume that stream without trusting an unvalidated binary,
host floating-point arithmetic, or an accidental change to its source lattice.

It is useful to report reproducible coarse evidence during data preparation, but it
would be misleading to call a centre-binned mean the mean elevation of an S2 cell.
Source rectangles can cross target boundaries.

## Decision

- `civilization-data derive centre-summary` reads one centre-index artifact and
  refuses malformed magic, schema, reserved bytes, dimensions, levels, record length,
  non-finite values, invalid CellIds, or any record whose CellId disagrees with a
  fresh route from the declared ETOPO lattice.
- It only summarizes to an ancestor S2 level. Each output record contains the target
  CellId, source sample count, and minimum, nearest-even mean, and maximum values in
  signed integer millimetres. Input `f32` IEEE-754 bits are converted with integer
  arithmetic; no host floating-point calculation contributes to output bytes.
- The 124-byte `ATCECS1` header binds schema, source stride/level, input-index SHA-256,
  original source-snapshot digest, original raw-artifact digest, output cell count, and
  source sample count. Records are ordered by CellId and the output is no-replacement.
- The output is named a **source-centre quadrature summary**. It makes no claim about
  target-cell coverage, target-cell area weighting, coastline, vertical datum handling,
  or area overlap. It MUST NOT be used as a canonical terrain tile or layer root.

## Verification

Tests build a full one-degree global index, re-route all 64,800 source centres, verify
the output header/counts and fixed-point values, and reject a tampered CellId. The
summary command independently checks the same contracts when it runs outside tests.

## Consequences

The source-attribution intermediate is now safely consumable by deterministic tooling.
The remaining canonical normalizer still needs an explicit target-support and
area-overlap (or separately justified aggregate-support) policy, vertical treatment,
quantization policy, and byte-identical rebuild proof.
