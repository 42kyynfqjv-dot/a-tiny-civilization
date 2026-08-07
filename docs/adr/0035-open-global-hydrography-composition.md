# ADR 0035: Initial hydrography is terrain-routed and open-evidence constrained

## Status

Accepted on 2026-08-07 for source selection and algorithm design. The public JRC v1.5
1984–2024 tile contract is now implemented as a deterministic, resumable acquisition
tool. A pinned source snapshot, routing specification, normalized release, and final
integrated scientific validation remain required before this decision can authorize
genesis.

## Context

Canonical hydrography needs more than lines labelled as rivers. The simulation needs
directed drainage connectivity, upstream contributing area, freshwater storage,
observed permanent/seasonal surface water, and climate-driven state. Several polished
global hydrography products are scientifically useful but currently carry
noncommercial or product-specific restrictions incompatible with an Apache-2.0 public
project that may accept supporter payments. Hydrography90m is CC-BY-NC-4.0, and the
HydroSHEDS site requires product-by-product review while its general site permission
is noncommercial.

The project already retains global ETOPO elevation, will retain Copernicus land/water
classification, and can acquire JRC Global Surface Water evidence under Copernicus
data terms. USGS HYDRO1K provides public-domain global streams, drainage basins, flow
direction, and ancillary terrain-derived layers at approximately one kilometre for an
independent coarse comparison.

## Decision

- Derive the canonical initial L10 drainage graph from the verified ETOPO terrain
  layer. The future routing ADR must specify depression handling, flats, endorheic
  basins, ocean outlets, neighbour ordering, integer slope comparison, accumulation,
  and every tie break. Runtime simulation and replay never invoke a GIS package.
- Acquire JRC Global Surface Water v1.5 occurrence, seasonality, and transition
  evidence from its official 2024 10-degree tile release, then pin every retained byte to
  a source snapshot. The long-term occurrence and transition layers cover 1984–2024;
  the v1.5 seasonality download describes 2024 rather than the complete history. Pin it to
  constrain observed freshwater storage and intermittent/permanent surface-water
  support. Reservoirs and other human-altered water bodies follow the project's
  explicit reconstruction policy; they are never silently relabelled natural.
- Use the pinned Copernicus land-cover water class and derived coastline as a second
  observed boundary constraint. Disagreement between evidence sources is retained as
  uncertainty rather than resolved by an undocumented preference.
- Use ERA5-Land runoff, soil-water, snow, and evaporation normal-period evidence to
  parameterize hydrological state and forcing after their temporal semantics are
  separately specified. They do not define river topology.
- Pin USGS HYDRO1K as public-domain validation evidence at the initial L10 scale. It
  cannot replace the ETOPO-derived graph: it was derived from older GTOPO30 terrain
  and is retained as an independent coarse cross-check, not privileged truth.
- Do not admit Hydrography90m, MERIT Hydro, or a HydroSHEDS product unless the exact
  product release later proves commercial use and public derived-data redistribution
  are compatible with the project.

## Consequences

Hydrography no longer requires a new owner account or commercial data licence. The
network-free acquisition inventory covers 1,512 official files across occurrence,
seasonality, and transitions; `scripts/acquire-jrc-surface-water.py --download` retains
them without replacing completed artifacts. The remaining work is engineering and
evidence acquisition: download and pin JRC and HYDRO1K artifacts,
define the deterministic routing algorithm, generate the global graph and water tiles,
compare them against independent evidence, and couple conserved water state to the
durable ecology model.

References: [JRC Global Surface Water user guide](https://storage.googleapis.com/water-world/downloads_ancillary/DataUsersGuidev2024_v.5.pdf),
[USGS HYDRO1K archive](https://www.usgs.gov/centers/eros/science/usgs-eros-archive-digital-elevation-hydro1k),
[USGS HYDRO1K public-domain notice](https://www.usgs.gov/media/files/hydro-1k-readme),
and [Hydrography90m licence](https://hydrography.org/hydrography90m/hydrography90m_layers/).
