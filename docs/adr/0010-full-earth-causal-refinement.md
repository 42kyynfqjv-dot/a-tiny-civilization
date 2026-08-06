# ADR 0010: The canonical world covers the full Earth at causal resolution

## Status

Accepted on 2026-08-06. This supersedes ADR 0009's bounded-raster geometry for a
canonical public world. Schema-v1 bounded configurations remain decodable only for
proofs, development fixtures, and regression tests.

## Context

A small river valley is useful for testing a scientific ingestion pipeline, but it is
the wrong boundary for an open-ended civilization. An invisible edge would eventually
become a scripted constraint, and selecting one attractive region by hand would curate
history before it began.

Representing the whole planet as a uniform 32-metre raster would also be misleading and
impractical. It would allocate nearly five hundred billion surface cells before a
single organism existed, while implying uniform source precision that the underlying
measurements do not have.

## Decision

- The canonical spatial domain is the entire Earth, including all present-geography
  land masses and oceans. Lower Buffalo–Ozark is a fixed development and regression
  tile, never the world boundary, preferred genesis site, or ecological template.
- S2 quadratic cells provide hierarchical global addresses. The exact S2 definition,
  implementation revision, and digest are tick-zero facts. S2 is not the physical
  shape of the world.
- Physical positions and calculations use WGS 84 Earth-Centered, Earth-Fixed
  coordinates (EPSG:4978), WGS 84 three-dimensional catalog coordinates (EPSG:4979),
  EGM2008 orthometric height (EPSG:3855), and deterministic local east/north/up frames.
- Four resolution roles are initially pinned:
  - S2 level 10 for planet-wide conserved ecological aggregates;
  - level 14 for regional ecology and migration/hydrology context;
  - level 18 for active landscapes around causal activity;
  - level 23 for synthesized embodied patches and object interaction.
- A finer level means finer simulation state, not finer scientific evidence. Every
  value retains its source resolution, transformation, uncertainty, and whether any
  finer structure was inferred.
- Climate, elevation, bathymetry, coastline, habitat, hydrography, and soil are required
  full-Earth layer families. Their content-addressed tile-tree roots are committed at
  genesis. Traversed child indexes and leaves must match the hashes reached from those
  roots.
- Off-region ecology remains canonical at its declared aggregate level. Refinement is
  triggered only by organisms or physical effects entering a causal neighborhood.
  Observer traffic, follows, supporter activity, and camera position cannot load or
  refine canonical state.
- Refinement uses stream-scoped seed material derived from the world seed, data-manifest
  hash, cell identity, process identity, and simulation epoch. Child allocations use a
  deterministic residual rule so conserved child totals exactly equal their parent.
  Coarsening aggregates exactly and retains the content-addressed causal delta.
- Genesis placement uses a preregistered viability predicate over global cells. The
  data release, predicate, code revision, and seed procedure are published before the
  unpreviewed seed is combined with a recorded public randomness input. There are no
  aesthetic previews or rerolls.

## Present geography without inherited technology

“Present-day Earth” means a pinned, multi-epoch scientific representation of current
physical geography and climate—not a synchronized photograph and not a prehistoric
reconstruction.

Roads, buildings, utilities, parcels, borders, writing, place labels, and other direct
modern human information are never ingested into the agent environment. Built and
cultivated surfaces are marked as anthropogenic unknowns and replaced only through a
versioned, cited ecological inference with uncertainty. Terrain sources that include
structures remain identified as surface models unless a real bare-earth source exists.

Dams, reservoirs, channelization, mines, reclaimed coast, introduced species, and
other physical legacies cannot be “undone” honestly from current data. Each is retained,
excluded, or reconstructed according to an explicit per-layer decision and assumption;
none is silently cleaned away. The result is described as a **present-geography
counterfactual biosphere**, not pristine, empty, prehistoric, or culturally unowned
Earth.

Sensitive species, caves, nests, and archaeological locations are omitted or
generalized in public and simulation releases.

## Consequences

- The full Earth exists from tick zero even though most of it is held at a coarse,
  conserved causal resolution.
- Storage and compute grow with causal activity rather than observer interest or total
  theoretical surface resolution.
- Canonical genesis is blocked until global layer roots, causal refinement, conserved
  aggregation, and unpreviewed placement pass deterministic tests.
- The Lower Buffalo fixture can be rebuilt repeatedly without biasing the real seed.
- Schema-v1 bounded proofs keep their exact serialized shape and verification hashes.

Reference specifications: [OGC DGGS](https://www.ogc.org/standards/dggs/),
[S2 cell hierarchy](https://s2geometry.io/devguide/s2cell_hierarchy), and
[EPSG:4978](https://epsg.org/crs_4978/WGS-84.html).
