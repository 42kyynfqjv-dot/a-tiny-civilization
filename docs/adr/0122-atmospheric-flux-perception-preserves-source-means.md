# ADR 0122: Atmospheric-flux perception preserves source means

## Status

Accepted for provisional ruleset 28. This is an engineering boundary, not scientific admission.

## Context

Configuration schema 5 already binds exact fixed-point monthly ERA5 normals for total
precipitation and eastward and northward near-surface wind components to the committed origin.
Ruleset 27 deliberately made only temperature causal. A first physical coupling for the remaining
measurements must remain deterministic, avoid privileged weather vocabulary, and must not invent a
meteorological distribution that the source summaries do not contain.

## Decision

- Ruleset 28 retains the ruleset-27 temperature driver and adds two direct touch-channel scalars:
  `water_flux` and `air_motion`. Agents receive only quantized values. They receive no rain, wind,
  direction, month, season, forecast, shelter, farming, or use label.
- `water_flux` is constant during each simulated day. For each adjacent pair of days within a
  30-day normal phase, a seed- and source-digest-bound draw chooses the first value from the closed
  interval zero through twice the monthly mean. The second value is its exact complement. Every
  pair therefore preserves the source monthly mean exactly; all normal phases contain 15 complete
  pairs.
- `air_motion` is the checked sum of the absolute monthly eastward and northward component means.
  It is an intentionally provisional L1 component-magnitude proxy, not wind speed and not a sampled
  wind distribution. This avoids inventing direction, covariance, variance, or square-root rounding.
- The source fixed-point units and decimal places remain in the configuration and provenance
  contract. The organism sees only its bodily scalar.
- Event, snapshot, and state-hash schema boundaries advance to 28. Rulesets through 27 remain
  byte-compatible and cannot call the atmospheric-flux driver.
- Canonical qualification requires schema-28 history plus at least one perception of each new
  scalar. Replay remains model-free and recomputes the same values from committed inputs.

## Consequences

This creates changing physical wetness input and a source-bound air-motion input without exposing
their interpretation or claiming realistic storms. It does not yet model cloud state, evaporation,
runoff, soil moisture, wind gusts, direction, covariance, or effects on bodies and objects. Those
require separately versioned mechanics and evidence. The paired-day construction is deliberately
minimal and must be replaced, not silently tuned, if a scientifically admitted weather process is
later adopted.

## Verification

Unit tests prove identical seed/source inputs reproduce identical values, every adjacent day pair
sums to exactly twice the monthly precipitation mean, the component proxy is exact, ruleset 27
rejects the new driver, and snapshot integrity selects schema 28. Canonical qualification inspects
the append-only event payloads for both label-free readings.
