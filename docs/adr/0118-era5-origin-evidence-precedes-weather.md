# ADR 0118: Exact ERA5 origin evidence precedes weather mechanics

## Status

Accepted on 2026-08-08.

## Context

The retained ERA5 normal-period snapshot contains 30 complete annual archives and six monthly-mean
variables, but the executable boundary previously inspected only one archive at a time. The
canonical origin therefore had no single source-bound artifact carrying precipitation, wind,
sea-surface temperature, or sea-ice evidence. Converting those fields directly into weather would
mix evidence retention with unresolved temporal downscaling and coupled-climate decisions.

## Decision

- A provisional origin-climate evidence artifact binds the canonical origin-selection digest and
  selected S2 patch to the exact ERA5 source-snapshot digest.
- The selected L10 centre maps deterministically to the nearest 0.25-degree ERA5 grid point.
  Latitude is descending, source longitude is 0–360 degrees, and half-step ownership is explicit.
- Every 1981–2010 archive is required in ascending year order. Its path, byte length, and SHA-256
  remain in the artifact.
- The six fixed source series are retained in canonical order: `siconc`, `sst`, `t2m`, `tp`, `u10`,
  and `v10`. Source unit and GRIB step type must match the pinned contract.
- All 2,160 monthly values remain their exact NetCDF binary32 bits. NaN is retained because a
  variable can legitimately be absent over a land or ocean cell; infinity is rejected.
- The artifact status is `provisional-noncausal-not-scientifically-admitted`. It cannot drive
  temperature, precipitation, wind, hydrology, ecology, action selection, or public claims.
- The ERA5 inspector now exposes exact variable types and attributes so changes in units, step
  semantics, missing-value representation, or grid metadata are visible before derivation.

## Consequences

The canonical origin can now retain its complete downloaded ERA5 normal-period evidence without
pretending monthly climatology is weather. A later ruleset must separately specify fixed-point unit
conversion, seasonal interpolation, seeded sub-month variation, cross-variable dependence,
land/ocean composition, and causal admission. Reprocessing or scientific corrections belong to a
successor world once this evidence digest is committed.

## Verification

Tests pin quarter-degree routing including negative-longitude and dateline handling, exact series
order and length, canonical round-trip bytes, and rejection of reordered or infinite inputs. The
real committed-origin derivation verified all 30 retained archives and produced six 360-value
series at source row 265, column 1026.
