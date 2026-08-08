# ADR 0125: Terrain relief affects movement without becoming knowledge

## Status

Accepted for provisional ruleset 29. The quantitative mapping is an explicit engineering
assumption pending the later scientific-validation pass.

## Context

Configuration schema 6 commits the exact ETOPO minimum, mean, and maximum bedrock-relief values at
the canonical L10 cell. A first causal use must not tell an organism its altitude, inject a map,
invent a slope for an unmeasured embodied path, or turn a scientific source field into a concept.
The existing bodily regulator already supplies the natural discovery channel: actions can have
different private fatigue outcomes, which the existing label-free learning system may experience.

ETOPO's L10 range is coarse regional evidence, not a measured L23 route. The rule therefore uses
only range magnitude as a bounded provisional load factor and makes no directional claim.

## Decision

- Ruleset 29 requires configuration schema 6 and its validated local surface baseline.
- Non-rest actions retain their existing fatigue exposure. A move additionally receives
  `baseline × relief_range_mm / 1,000,000`, with integer floor rounding and checked arithmetic.
  Equivalently, one kilometre of regional relief doubles the pre-existing movement fatigue
  exposure; flat source support adds nothing.
- The canonical origin range is 136,086 mm, so a move receives exactly 1.136086 times the baseline
  exposure whenever the baseline is a multiple of one million. No cap or hidden random draw exists;
  the existing bodily capacity remains the mechanical upper bound.
- The effect changes only the private `OrganismNeedsChanged` outcome. No terrain, elevation, slope,
  difficulty, route, direction, or use reading is emitted. Existing action and movement-direction
  learning can discover bodily consequences without receiving an answer key.
- Event, snapshot, and state-hash schemas advance to 29 even though the event payload shape is
  unchanged. Rulesets through 28 retain their exact fatigue behavior.
- JRC and SoilGrids inputs remain committed but causally unread. They require independent mappings;
  ruleset 29 does not interpret water occurrence, soil properties, habitat, fertility, or food.

## Consequences

Actual retained terrain now affects history through an auditable, replay-safe physical cost rather
than an observer label. The one-kilometre reference is not asserted as a scientific locomotion
model; it is versioned and visible so the validation pass can replace it in a successor world
without tuning a live world. Since the effect feeds existing bodily learning, inhabitants may adapt
their actions through experience but are not born knowing terrain mechanics.

## Verification

Tests prove zero relief preserves baseline exposure, the canonical 136,086 mm range maps
1,000,000 exposure units to exactly 1,136,086, inverted ranges fail, movement is costlier than an
otherwise non-rest action, a surface-bound schema-6 configuration is used, and ruleset-29 event,
snapshot, state-hash, and replay boundaries are distinct. Qualification additionally requires one
schema-6 surface configuration and schema-29 history.
