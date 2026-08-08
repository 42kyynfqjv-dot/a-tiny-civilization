# ADR 0120: seeded local temperature precedes complete weather

## Status

Accepted on 2026-08-08.

## Context

ADR 0119 established exact fixed-point ERA5 monthly normals but deliberately kept them noncausal.
The engine needs changing physical conditions without supplying seasons, weather concepts, habitat
labels, or authored behavior to organisms. Monthly summaries alone do not justify a complete
multivariate weather model: in particular, they do not retain daily covariance or wind and
precipitation distributions.

## Decision

- Ruleset 27 requires configuration schema 5. Genesis embeds both the existing CHELSA local
  environment baseline and a weather-input baseline derived from the pinned ERA5 normals.
- Initialization independently validates the evidence-to-normals digest, the selected L10 evidence
  cell, the active embodied patch, units, decimal precision, all 12 months, and complete values for
  air temperature, precipitation, and both wind components. Missing causal input fails closed.
- Air temperature is the first causal weather dimension. One deterministic anchor is selected for
  every simulation day from that normal phase's observed ERA5 minimum-to-maximum interval. The draw
  binds driver version, world seed, normals digest, and simulation-day index.
- Consecutive daily anchors are linearly interpolated in integer source units. Division truncates
  toward zero. No wall clock, host random source, service, model, or observer input participates.
- The resulting physical temperature replaces the old monthly mean in bodily thermal regulation
  and direct touch readings. Organisms receive only a quantized physical value; they receive no
  month, season, climate, weather, comfort, or survival label.
- Precipitation and wind normals are carried in the immutable configuration but remain noncausal
  until a later ADR defines defensible day-scale distributions and cross-variable dependence.
- Event, snapshot, state-hash, and configuration schema boundaries advance to 27, 27, 27, and 5.
  Ruleset-26 histories retain their exact monthly-mean behavior and existing schemas.

## Consequences

The world now experiences replay-stable local temperature variation sourced from actual Earth
measurements without pretending that an invented independent-noise process is complete weather.
This is explicitly provisional and not scientifically admitted. It does not yet create rain,
wind, clouds, surface water, storms, or ecological responses.

## Verification

Tests prove identical values for identical seed/input/tick, enforce the monthly source envelope,
bound interpolation between adjacent five-minute ticks, verify schema-27 snapshot integrity, retain
ruleset-26 monthly-mean behavior, and reject incomplete or spatially mismatched genesis inputs. The
canonical qualification gate additionally requires exactly one schema-5 weather configuration and
schema-27 event batches before a candidate can pass.
