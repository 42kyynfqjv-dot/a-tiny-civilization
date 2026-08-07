# ADR 0036: Copernicus land cover uses fixed L10 target-support quadrature

## Status

Accepted on 2026-08-07 for the first observed-surface evidence root. This does not
turn observed 2022 classes into prehistoric habitat, a surveyed coastline, or a
genesis-eligible world-data bundle.

## Context

The retained Copernicus v2.1.1 product is a 64,800 × 129,600 geographic area raster at
1/360 degree spacing. A simulation L10 S2 cell covers many source cells and its curved
support crosses geographic rows and columns. Assigning source pixels by their centres
would require routing 8,398,080,000 areas and would still misstate centre ownership as
exact overlap. Sampling only the S2-cell centre would erase mixed coast, forest,
grassland, agricultural, and built-up evidence.

The source also carries four quality rasters. A categorical normalizer that keeps only
the modal LCCS value would discard both the observed mixture and the evidence needed to
judge it. Conversely, importing the source's modern urban and cultivated classes as a
ready-made ancient landscape would script a reconstruction the observations do not
contain.

## Decision

- The first layer is named `observed-land-cover`, not `habitat`. It preserves the
  pinned 2022 surface classification before any ecological or counterfactual inference.
- Every S2 L10 target cell is sampled on a fixed 32 × 32 lattice in its native S2-face
  `(u,v)` rectangle. Each coordinate is the odd midpoint of one of 1,024 equal face-UV
  subrectangles. Rational interpolation, face-ray construction, and WGS 84 inverse
  routing use checked integer arithmetic; runtime normalization calls no host
  trigonometry or GIS library.
- The resulting exact-E7 coordinate selects the containing Copernicus area cell with
  integer arithmetic. Columns are west-to-east, west-inclusive/east-exclusive, with
  `+180°` owned by the wrapped `-180°` column. Source rows are north-to-south,
  north-inclusive/south-exclusive; the terminal `-90°` point is owned by the last row.
- Each of the 1,024 target samples has equal weight. This is deterministic target-
  support quadrature, not exact spherical area intersection and not equal physical
  area. Thirty-two points per axis approximately match the nominal 300 m source
  spacing across an average L10 cell; polar distortion and boundary approximation are
  disclosed rather than hidden.
- A packed L6 container retains all 256 L10 descendants. Every L10 record stores sparse
  counts for all sampled LCCS classes, exact sampled counts for `processed_flag` and
  `current_pixel_state`, and minimum/sum/maximum for `observation_count` and
  `change_count`. Counts must conserve all 1,024 samples and remain within the pinned
  source domains.
- The tile binds the exact source snapshot and archive digests, sample-policy identity,
  quadrature resolution, container, and target level. Publication is staged and atomic;
  every root reference and canonical tile is reread independently. A second full build
  must produce the same root hash before the root is retained as evidence.
- LCCS values 10–40 and 190 remain explicit modern anthropogenic evidence. They are not
  erased, relabeled as wilderness, or exposed to agents as cultural concepts. A later
  reconstruction layer must cite its inputs, keep those observations recoverable, and
  state every inference.

## Consequences

The project gains a reproducible mixed-class global evidence format instead of a modal
map or 38 unrelated Boolean layers. Small or narrow 300 m features can still be missed
by target quadrature, and face-UV samples are not spherical-area weights. Those are
known v1 approximation limits. Habitat inference still requires climate, terrain,
soil, and real species evidence; coastline estimation still requires terrain and water
cross-checks under ADR 0034 and ADR 0035.
