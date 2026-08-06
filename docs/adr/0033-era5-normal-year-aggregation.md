# ADR 0033: ERA5 normal-year aggregation preserves annual source support

## Status

Accepted on 2026-08-06 for the pinned 1981–2010 ERA5 source snapshot. This
specifies temporal interpretation and provenance for a future climate normalizer; it
does not yet define source-grid support geometry, S2 aggregation, a released climate
tile, or full-Earth genesis.

## Context

The retained source consists of thirty annual ZIP containers. Each contains twelve
calendar-month samples for instantaneous fields in `avgua` and accumulated
precipitation in `avgad`. A normal-year tile must retain twelve phases, but cannot
truthfully claim there were only twelve upstream artifacts.

ERA5 documentation describes its `moda` accumulated fields as monthly means of daily
means with effective daily processing periods. In particular, `tp` is metres of water
equivalent per day, not an already-integrated monthly total. The source is a
reanalysis at grid-box scale, rather than direct point measurements or historical
weather replay.

## Decision

- Before parsing any member, a normalizer verifies the complete source snapshot and
  the exact two-member ZIP contract from [ADR 0032](0032-era5-archive-evidence.md).
  It uses only private scratch files for member extraction and never changes retained
  source bytes.
- The normal-year cycle has exactly twelve January-through-December phases. For a
  field and calendar month, the mean is the integer-defined arithmetic mean of the
  thirty corresponding annual monthly values. The retained minimum and maximum are
  the extrema over those same thirty values; they are not daily or sub-grid extremes.
- Every one of the thirty data-artifact digests is recorded with the all-month phase
  mask in a seasonal tile. The source snapshot digest remains the compact commitment
  to the complete evidence set. Schema v2 rejects missing phase support, duplicate or
  unordered source artifacts, and a claim that one artifact supports an undeclared
  phase.
- Field conversion happens before aggregation through checked, declared rational
  conversion and quantization—not host floating-point formatting. The future release
  must state a conversion record for every emitted field. `t2m` and `sst` retain Kelvin
  unless a separately declared fixed conversion is selected; `u10` and `v10` retain
  signed metres per second; `siconc` retains its source fraction; `tp` is converted
  from metres water equivalent per day to millimetres water equivalent per day by the
  exact factor 1000. No field is silently relabeled as a monthly total.
- NaN, fill, infinite, out-of-declared-domain, missing-time, duplicate-time, and
  coordinate-schema conditions fail closed. There is no imputation in this source
  normalization. A field with fewer than thirty valid annual values for any declared
  phase produces no tile.
- A future geographic-source-to-S2 contract must establish whether the ERA5 grid
  coordinates represent nodes or cell centres; its source support, polar and seam
  ownership, and conservative target-cell aggregation must be explicit before a
  normalizer emits S2 tiles. This ADR intentionally does not infer those semantics
  merely from a 0.25-degree coordinate sequence.

## Consequences

The project can now construct climate evidence that is honest about its thirty-year
normal period and precipitation units. It cannot yet claim a global climate layer:
the remaining work is portable NetCDF-member inspection, exact value decoding,
source-grid support semantics, deterministic geographic routing, spatial aggregation,
output quantization, and an independently rebuilt release root.

References: [ERA5 monthly data documentation](https://confluence.ecmwf.int/pages/viewpage.action?pageId=216495456),
[ERA5 accumulation conversion table](https://confluence.ecmwf.int/pages/viewpage.action?pageId=272324919),
and the pinned [ERA5 dataset DOI](https://doi.org/10.24381/cds.f17050d7).
