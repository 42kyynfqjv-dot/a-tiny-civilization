# Full-Earth scientific data plan

## Scope

The canonical world contains the whole globe from genesis. “Full Earth” does not mean
that every square metre is stored or updated at maximum detail. It means every location
has an address, physical-geography baseline, canonical ecological state at a declared
resolution, and a deterministic path to finer causal state.

The baseline preserves present land masses, oceans, terrain, climate statistics, real
materials, and real taxa through pinned source releases. It excludes direct modern
infrastructure and information that would hand technology or modern culture to the
agents. This is a present-geography counterfactual biosphere assembled from sources
with different observation dates—not “Earth on one exact day.”

## Coordinate and resolution contract

- S2 quadratic cells provide stable 64-bit hierarchical addresses. The implementation
  revision and definition digest are immutable world inputs.
- WGS 84 ECEF (EPSG:4978) and deterministic local ENU frames drive distance and
  physics. EPSG:4979 is the source/catalog coordinate frame and EGM2008 (EPSG:3855)
  supplies the declared orthometric-height frame.
- Initial roles are S2 L10 planet aggregates, L14 regional ecology, L18 active
  landscape, and L23 embodied patches. S2's published cell sizes vary by location;
  these are roles rather than square-metre promises.
- Measured source resolution, simulation resolution, uncertainty, and inference status
  remain separate fields.

References: [S2 hierarchy](https://s2geometry.io/devguide/s2cell_hierarchy),
[S2 cell statistics](https://s2geometry.io/resources/s2cell_statistics.html), and
[EPSG:4978](https://epsg.org/crs_4978/WGS-84.html).

## Candidate authoritative source families

No row below is a committed world input until an exact artifact, version, retrieval
date, terms snapshot, byte length, and SHA-256 digest enter a validated bundle.

| Domain | Candidate source | Intended role and caution |
| --- | --- | --- |
| Global relief | [NOAA ETOPO 2022](https://www.ncei.noaa.gov/products/etopo-global-relief-model) | Global land/bathymetry baseline; retain source-type and vertical-reference metadata. |
| Generalized land reference | [Natural Earth 1:10m land](https://www.naturalearthdata.com/downloads/10m-physical-vectors/10m-land/) | Public-domain global land geometry for acquisition and coarse cross-checks; generalized cartography is not a measurement-resolution canonical coastline. |
| Bathymetry | [GEBCO gridded bathymetry](https://www.gebco.net/data_and_products/gridded_bathymetry_data/) | Ocean refinement and type-identifier evidence; much seabed detail is interpolated and releases change annually. |
| Land terrain | [NASADEM](https://www.earthdata.nasa.gov/data/catalog/lpcloud-nasadem-hgt-001) plus compatible national bare-earth models | Finer land elevation where coverage and terms permit; a digital surface model must not be mislabeled as bare terrain. |
| Polar terrain | [ArcticDEM and REMA](https://www.pgc.umn.edu/guides/stereo-derived-elevation-models/pgc-dem-products-arcticdem-rema-and-earthdem/) | Higher-resolution polar terrain with its uncertainty and attribution retained. |
| Land cover | [ESA WorldCover 2021](https://esa-worldcover.org/en/data-access) | Evidence for observed cover; built-up/cropland classes are anthropogenic unknowns, not ready-made wilderness. |
| Surface water | [JRC Global Surface Water](https://global-surface-water.appspot.com/download) | Observed water history and seasonality; not an instruction to expose dams or modern labels. |
| Watersheds | [HydroSHEDS products](https://www.hydrosheds.org/products) | Globally consistent routing/basins/rivers/lakes; exact product-specific redistribution and commercial terms must pass review. |
| Climate | [ERA5-Land](https://cds.climate.copernicus.eu/datasets/reanalysis-era5-land) | Freeze a dated release and derive a declared climate-normal period; never consume a moving upstream series during a world. |
| Soils | [SoilGrids](https://docs.isric.org/globaldata/soilgrids/index.html) | Global soil-property estimates and prediction intervals; not local ground truth. |
| Taxonomy and occurrences | [GBIF](https://www.gbif.org/publishing-data) plus compatible taxonomic authorities | DOI-pinned downloads and stable identities; retain only records whose individual licenses permit the release. Occurrences are evidence, not species ranges. |

Datasets with noncommercial, no-redistribution, registration-only, or unclear derived-
data terms cannot enter a public Apache-2.0 repository or supporter-funded deployment
without a documented legal path. Raw scientific archives remain outside Git; the repo
stores acquisition recipes, manifests, citations, hashes, and validators.

## Removing inherited modern information

The ingestion pipeline never imports roads, buildings, utilities, parcels,
administrative borders, census data, written labels, or place names into world state.
It does not pretend to reverse history:

- built-up and cultivated surfaces become `anthropogenic_surface_unknown` in the
  scientific pipeline, never an agent-facing label;
- replacement ecology is inferred deterministically from compatible intact cells using
  climate, soil, elevation, slope, and regional habitat evidence;
- every donor distribution, method version, inference flag, and uncertainty is retained;
- reservoirs, channelization, mines, reclaimed coast, and similar physical legacies are
  kept, excluded, or reconstructed only under a specific cited decision;
- introduced species and altered climate are not silently relabeled “natural.”

This policy prevents a city, farm, road sign, or map from gifting technology while
remaining honest about what current data can and cannot reconstruct.

## Conserved causal refinement

Each planet-level cell carries conserved extensive quantities such as water, nutrients,
living/dead biomass, and declared abundance cohorts. Refinement allocates those totals
to child cells using only pinned evidence and a stream key derived from policy version,
world seed, exact normalized bundle content digest, parent cell, process, retained
refinement generation, quantity, and child cell. Exact Hamilton residual allocation
makes child totals equal the parent. The synthesis is retained and causal changes are
applied as deltas; it is never recalculated when a parent total changes because
Hamilton allocation is not population monotone.

Crossing organisms and physical flows are ordered boundary events. Coarsening exactly
re-aggregates children and retains the causal delta. A browser request may read a
projection but cannot materialize a canonical child. The current private proof covers
one caller-supplied scalar-evidence vector at a time; it does not yet prove that the
weights came from the claimed bundle or retain its synthesis context. Verified evidence
binding, coupled ecological vector constraints, and durable retained state remain
required before genesis. See
[ADR 0014](../adr/0014-conserved-ecology-refinement.md).

## Unpreviewed genesis placement

Before choosing a seed, publish and hash:

1. the complete source/data manifest and ruleset revision;
2. a global eligibility predicate, including land/ice, freshwater, source coverage,
   and preregistered physiological climate constraints;
3. area weighting and exact cell-ordering logic;
4. the seed commitment and recorded public-randomness procedure.

The first eligible cell in the resulting deterministic permutation wins. The initial
state hash is committed before rendering or naming the location. There are no rerolls.
Lower Buffalo participates under exactly the same rule as every other eligible cell.

## Delivery order

1. ~~Implement canonical tile-tree index traversal and hash verification.~~ Complete.
2. ~~Implement the pure L10 ordering/barrier kernel and synthetic dense-versus-queued
   equivalence proof.~~ Complete.
3. ~~Implement the private fixed-point ECEF-to-S2 address reference and cross-language
   golden verification.~~ Complete.
4. Freeze license-compatible global source artifacts and construct L10 layer roots.
In progress: exact snapshots pin Natural Earth generalized global land polygons and
CC0 NOAA ETOPO 2022 v1 global bedrock relief. Full-Earth layer contracts now bind
source-snapshot manifest digests before a normalizer can claim a root. A deterministic
Natural Earth binary inspector now validates and parses the pinned polygon stream into
an auditable raw framing summary; ETOPO can derive a hash-bound regular global
elevation intermediate, and its exact area-cell centres route through the fixed-point
geographic-to-S2 contract with source-area geometry pinned in
[ADR 0022](../adr/0022-etopo-area-cell-support.md). Neither claims a normalized root.
Source-centre routing can now be retained as a hash-bound intermediate under
[ADR 0023](../adr/0023-etopo-s2-centre-index.md), but this remains attribution rather
than area ownership. The remaining prerequisite is a source-grid aggregation kernel; see
[ADR 0021](../adr/0021-geographic-source-to-s2-normalization.md).
5. ~~Implement a private conserved L10↔L14 scalar-refinement and reaggregation proof.~~
   Complete. Couple sourced ecological quantities and retain refinements/deltas after
   step 4 establishes their real bundle semantics.
6. Normalize the Lower Buffalo L18/L23 reference window and verify local physics.
7. Add global viability enumeration and the unpreviewed placement dry run.
8. Run a multi-year disposable world twice to identical hashes before any public seed.

No source bundle, start location, or canonical world currently exists; schema fixtures
in tests are explicitly non-scientific.
