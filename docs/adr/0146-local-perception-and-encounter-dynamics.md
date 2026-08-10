# ADR 0146: local perception and encounter dynamics span more than one body cell

Date: 2026-08-10

Status: Accepted

## Context

The public ruleset-33 world placed every body in an S2 level-23 patch and originally reused exact
patch equality for hearing, social attention, and reproductive contact. After about 69,000 ticks,
the 24 living people occupied 24 distinct body patches but remained within a roughly 200-metre
area: the closest pair was about 10 metres apart and the median nearest-neighbour distance was about
20 metres. Exact-patch hearing therefore treated a loose local group as mutually isolated. It also
prevented learned signal associations and removed any movement tendency that could produce later
physical encounters.

Making language or reproduction happen would script history. Leaving direct perception confined to
one metre-scale address would instead make both outcomes depend on an unrealistic discretization
artifact.

## Decision

Ruleset 35 adds a stateless deterministic local-interaction driver:

- hearing and social attention treat the source organism's S2 level-18 active-landscape patch and
  its four edge-neighbour landscape patches as one bounded vicinity;
- each signal reaches at most the eight nearest living recipients in that vicinity, ordered by
  exact level-23 IJ grid distance and stable organism identity, bounding event growth;
- a deterministic per-tick index buckets living organisms by level-18 landscape cell, so local
  candidate discovery examines only the source cell and its four edge neighbours rather than the
  global population;
- each organism socially attends to at most one acting organism in the vicinity per tick through a
  seed- and tick-derived deterministic draw;
- movement receives a bounded, non-mandatory weight increase only for a primitive direction that
  reduces grid distance to the nearest directly heard living signal source; and
- reproductive eligibility remains exact-patch, species/category-bound private physiology. The
  driver creates possible encounters; it never schedules a reproductive outcome.

The running ruleset-33 world activates this driver after reaching tick 75,000. Ruleset-33 states
through tick 75,000 retain their prior behavior; the next planned transition uses the new driver.
Ruleset 35 uses it from genesis. Existing event types, state, hashes, and snapshot schemas are
unchanged.

The level-18 vicinity is a causal address approximation, not a claim that every call is audible at
one fixed metric radius. Signal attenuation, terrain occlusion, wind coupling, species-specific
hearing, and source-bound social-range parameters remain scientific-validation work.

## Consequences

- Nearby organisms can hear and observe one another without occupying the same metre-scale cell.
- Local social evidence can continue accumulating while language remains optional and
  evidence-thresholded.
- Neutral movement toward a directly heard source makes genuine close encounters plausible without
  introducing species labels, courtship, partnership, kinship concepts, or observer steering.
- Signal event volume remains linear in emitters with a fixed recipient cap rather than quadratic
  in local population.
- Candidate lookup scales with local density rather than total world population. Extremely dense
  cells still require later hierarchical subdivision and aggregate crowd-pressure/acoustic
  mechanics; the fixed attention and recording caps remain in force.
- Replay remains byte-for-byte stable across the disclosed live activation boundary.
