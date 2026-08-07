# ADR 0044: Bodily regulation is canonical, source-addressed, and non-explicit

## Status

Accepted on 2026-08-07. Ruleset-ten implementation is the first provisional execution
of this contract; its parameter profiles remain subject to the scientific quality gate.

## Context

Age, movement, perception, material handling, and local signals do not yet make an
organism physically alive. A useful world needs accumulating bodily pressures,
rest-dependent fatigue, temperature exposure, and mechanical death when regulation
fails. Those processes must not tell an organism what will relieve a pressure, and the
observer must never receive graphic or mechanism-level presentation.

Implementing the causal machinery before scientific validation is useful only if the
engine cannot disguise convenient game constants as measurements. The parameter set
that drives each body therefore needs to be an immutable part of world history, with
an explicit evidence basis and digest. Under ADR 0049, engineering assumptions may
shape an explicitly labelled experimental genesis but cannot be presented as measured
or scientifically admitted.

## Decision

- A canonical body retains five bounded, dimensionless pressure intensities: energy
  deficit, hydration deficit, thermal discomfort, pain, and fatigue. Zero means no
  pressure and `u16::MAX` is the mechanical regulation limit.
- Each ruleset-ten organism carries a species-matching physiological-regulation
  commitment. It pins a profile identifier, digest, evidence basis, usable energy
  reserve, hydration and fatigue durations, thermoneutral bounds, and a thermal
  exposure/recovery budget. Ruleset ten also requires the existing exact measured
  metabolic-power commitment.
- Energy pressure integrates the committed measured power over immutable simulated
  tick duration. Hydration and fatigue use committed time constants. Rest reduces
  fatigue; other primitive actions increase it. Temperature pressure integrates only
  the physical exposure outside committed thermoneutral bounds.
- Every pressure transition is an append-only event containing its exact prior and
  next state. Replays never recalculate historical pressure from a changed profile.
- Reaching the energy, hydration, or thermal limit emits a neutral versioned mortality
  mechanism. Fatigue and pain affect future policy but do not independently kill in
  this ruleset.
- A need names only a bodily pressure. The engine does not label any material as food,
  water, shelter, or medicine, and this ruleset does not yet invent ingestion effects.
- Raw pressure values, committed physiological parameters, and mortality mechanisms
  are withheld from public projections. The observer may receive only restrained
  population, injury, birth, and death outcomes under the existing presentation
  policy.

## Consequences

The engine has a replayable survival substrate without claiming that provisional
profiles are scientifically admitted. Later material ingestion, exposure, injury,
reproduction, and deterministic policy rules can change these same neutral pressures.
Every participating real taxon needs an immutable profile and disclosed evidence
class. Scientific review may replace provisional assumptions only in a successor
world under ADR 0049.
