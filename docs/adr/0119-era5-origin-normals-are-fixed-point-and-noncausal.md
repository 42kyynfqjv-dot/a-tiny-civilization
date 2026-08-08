# ADR 0119: ERA5 origin normals are fixed-point and noncausal

## Status

Accepted on 2026-08-08.

## Context

ADR 0118 retains the exact binary32 values from six ERA5 monthly series at the canonical origin.
Those source bits are durable evidence, but simulation mechanics require an integer boundary whose
result does not depend on host floating-point behavior. Monthly climatological summaries are also
not weather: they contain no day-scale sequence, cross-variable covariance, or causal land/ocean
model.

## Decision

- A canonical provisional normals artifact binds the SHA-256 digest of the exact origin-climate
  evidence artifact.
- Every finite binary32 observation is converted using integer arithmetic and round-to-nearest,
  ties-to-even. Temperature converts from kelvin to integer millidegrees Celsius; sea-ice fraction
  and precipitation use six decimal places; wind uses integer millimetres per second.
- Each calendar month is summarized across 1981–2010 only after per-observation conversion. The
  minimum, maximum, observed-year count, and ties-to-even integer mean are retained.
- NaN is explicit missing data. A completely absent series/month has count zero and null summary
  values. Partial coverage is retained with its exact count. Infinity and malformed partial
  summaries fail closed.
- Series order, units, decimal precision, month order, and conversion-policy identifier are part of
  the canonical schema.
- The status is `provisional-weather-input-noncausal-not-scientifically-admitted`. The normals do
  not yet affect weather, organisms, perception, action selection, ecology, or public scientific
  claims.

## Consequences

The project now has a replay-stable integer input contract for later weather work without silently
turning climatology into a weather generator. At the committed terrestrial origin, sea-surface
temperature and sea-ice are explicitly absent in every month; air temperature, precipitation, and
both wind components each have 30 observations per month. A later causal ADR must define temporal
downscaling, season boundaries, seeded variation, cross-variable dependence, and how the selected
land cell interacts with surrounding ocean and terrain.

## Verification

Tests cover canonical round trips, fixed series/month structure, partial-summary rejection,
round-to-nearest-even ties, kelvin conversion, NaN-as-missing, and infinity rejection. Deriving from
the candidate-v7 evidence produced content hash
`4ad51196a8850aa29d52d54dce10a06b8340065749f07a80688fa8ae87aafe64`, bound to exact evidence
digest `3e16aa5093aafc4178bbdd2c2cd7a9e94f54552583bfac330d180dc3ec256fb3`.
