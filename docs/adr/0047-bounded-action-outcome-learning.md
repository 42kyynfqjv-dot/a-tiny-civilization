# ADR 0047: Baseline learning is bounded action/outcome association

## Status

Accepted on 2026-08-07. Ruleset-thirteen implementation follows this contract.

## Context

A seeded freeform policy can explore, and physical material transfer can change bodily
state, but without retained experience the organism cannot alter later behavior from
what happened. The first learning substrate must remain deterministic and situated. It
must not inspect scientific material profiles, assign culturally privileged goals,
infer what an object is for, or let an external memory/LLM response silently alter a
canonical tick.

## Decision

- Every scheduled organism action in ruleset thirteen is followed by its exact bodily
  transition and one action-value transition before any same-tick mechanical death.
- The reward input is only the signed change in the sum of the organism's five bounded
  internal pressures. Falling total pressure is positive, rising pressure is negative,
  and no action kind is mapped to a preferred need or expected solution.
- Experience is retained per primitive action kind as an observation count and a
  cumulative value clamped to `[-128, 128]`. This deliberately supports only broad
  trial-and-error generalization. It does not yet learn an object, sensed-property,
  sequence, partner, place, explanation, belief, or skill.
- Positive experience can add bounded weight to that primitive action; negative
  experience can reduce it only to a minimum weight of one. Exploration therefore
  remains possible and no learned value becomes a hard script.
- The action policy draw advances to version two when learning is active. Its inputs
  remain canonical body/world state. Observer state, supporter activity, wall time,
  material response profiles, external memory, and model output are absent.
- Action-value updates are ordinary hash-chained events. Event schema fifteen and
  snapshot/state-hash schema sixteen isolate the boundary. Replay consumes recorded
  updates and never relearns history.
- Ruleset-thirteen actions may occur only inside the deterministic scheduled tick.
  Each must have exactly one bodily transition and exactly one matching value update;
  manual standalone action commits fail closed.
- Action values and their updates are internal mechanisms and are omitted from public
  timeline, organism, and finding projections.

## Consequences

An organism can now become more or less likely to repeat a primitive motion because
its own pressure changed afterward, while still being ignorant of why. This is not a
complete mind: target-specific association, delayed credit, perception generalization,
imitation, teaching, beliefs, forgetting, and bounded external cognition remain later
versioned checkpoints.
