# ADR 0141: public life numbers are not in-world names

Date: 2026-08-09

Status: Accepted

## Context

Observers need stable handles to choose, follow, cite, and discuss individual lives. Assigning those
handles inside canonical simulation state would leak the human concepts of names, writing, or
observer identity into a population that must discover any such practices for itself.

## Decision

The observatory assigns stable finding-aid labels such as `Person 01` and `Animal 01`. Numbering is
separate per participation tier and ordered by first committed appearance, with canonical organism
ID as the deterministic tie-breaker. The organism ID remains the durable URL identity; the label is
a read-side projection and never enters events, cognition, memory, perception, or action selection.

The labels remain available for the entire archive. If inhabitants later establish a repeatable
name-like practice through their own signal and social-learning mechanics, the public wiki may
document that interpretation with provenance and make an inhabitant-created designation the
primary public display. The numerical ID remains visible as its permanent audit reference. The
observatory must not infer names from observer preferences.

## Consequences

- People can follow an individual immediately without granting it a name inside the world.
- Public citations remain stable before and after any possible emergence of naming, even when an
  emergent name becomes the primary display.
- A future naming interpretation is a provenance-bearing observer/wiki feature, not a canonical
  rename operation.
- Supporter reservations remain observer-side and cannot become inhabitant knowledge.
