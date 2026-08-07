# ADR 0034: Copernicus land evidence is composed with SoilGrids properties

## Status

Accepted on 2026-08-06 for full-Earth source acquisition planning. Exact artifacts
and release roots remain required before this decision can authorize genesis.

## Context

The full-Earth schema requires separate climate, coastline, habitat, hydrography, and
soil semantics. That does not require a different data provider for every layer.
The authenticated Climate Data Store catalogue currently exposes the global
`satellite-land-cover` process with version `v2_1_1`, annual products through 2022,
22 UN FAO LCCS surface classes, and four classification/change quality flags. It also
exposes ERA5-Land soil temperature and volumetric water through four model depths.

Land cover is not habitat by itself, a classified water edge is not an exact surveyed
shoreline, and modeled soil water is not soil composition. Treating any of these as
interchangeable would hide assumptions rather than reduce work.

## Decision

- Pin the complete global 2022 `satellite-land-cover` `v2_1_1` response, including all
  quality variables, as the primary observed surface-class evidence. Its acquisition
  request is fixed and may not silently advance to a newer year or version.
- Derive initial L10 habitat evidence from the land-cover classes and quality flags,
  coupled with climate and terrain. Built-up and cultivated classes remain explicit
  reconstruction inputs and are never agent-visible habitat names or technologies.
- Derive the initial L10 land/water boundary from the same classified surface evidence,
  constrained by ETOPO elevation/bathymetry and checked against the pinned Natural
  Earth land reference. The emitted layer remains a coastline estimate with source
  uncertainty; no input is mislabeled as a surveyed shoreline.
- Use ERA5-Land only for time-varying or normal-period soil state, including temperature
  and volumetric water by model depth. Its model layers are not pedological horizons.
- Use SoilGrids for physical and chemical soil properties: texture fractions, bulk
  density, coarse fragments, pH, organic carbon, cation-exchange capacity, and total
  nitrogen, retaining published prediction intervals. SoilGrids and ERA5-Land remain
  separately cited evidence even when combined into one simulation soil vector.
- Hydrography remains separate. A land-cover water class cannot establish flow
  direction, drainage topology, seasonal connectivity, or freshwater storage.

## Consequences

The initial habitat and coastline evidence no longer require a new external account or
an unresolved provider search. The existing CDS credential can acquire land cover and
ERA5-Land after any dataset-specific terms are accepted; SoilGrids is public CC BY 4.0.
Substantial deterministic normalization and ecological inference still remain, and no
source response becomes a canonical layer merely because it was downloaded.

The observed-class target-support and packed quality schema are fixed separately in
[ADR 0036](0036-copernicus-land-cover-l10-target-support.md).

For the breadth-first integration pass, the repository can now acquire the official
SoilGrids global BigTIFF overview pyramids for nine 0–5 cm properties at Q0.05, Q0.5,
and Q0.95. Their first image is approximately one-kilometre support (39,811 × 14,509
in the Homolosine grid), appropriate for plumbing a provisional L10 soil vector. This
does not replace the final six-depth, native-250 m evidence pass or authorize genesis.

References: [CDS satellite land cover](https://cds.climate.copernicus.eu/datasets/satellite-land-cover?tab=documentation),
[ERA5-Land monthly means](https://cds.climate.copernicus.eu/datasets/reanalysis-era5-land-monthly-means?tab=overview),
and [SoilGrids](https://docs.isric.org/globaldata/soilgrids/index.html).
