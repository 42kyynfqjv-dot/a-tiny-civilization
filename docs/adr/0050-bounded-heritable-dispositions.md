# ADR 0050: Heredity is a bounded innate action disposition, not inherited knowledge

## Status

Accepted on 2026-08-07. Ruleset-fifteen implementation follows this contract.

## Context

Ruleset fourteen makes births causal but gives every individual of one species the
same initial action policy. Consequential individual variation should cross
generations without pretending that the simulation contains a molecular genome or
allowing learned knowledge, observer language, or privileged action meanings to enter
at birth.

The next world is an experimental genesis under ADR 0049. Its heritable profile may
therefore be an openly declared engineering assumption, but its derivation and replay
boundaries must still be exact.

## Decision

- Every ruleset-fifteen organism has one immutable disposition weight for each of the
  eleven existing use-neutral primitive actions. The weights are a bounded innate
  action prior, not genes, personality, intelligence, goals, or action meanings.
- One canonical full profile applies to each exact real species in a world. Mixed
  profiles for the same species fail genesis and state validation rather than forming
  accidental non-mating subpopulations.
- Founder weights derive only from the world seed, organism identity, action kind,
  and the canonical full-profile fingerprint. Supplied founder weights are recomputed
  and rejected if they differ.
- For each primitive action, an offspring deterministically selects one parent's
  inherited weight and may receive a small bounded variation. The full profile limits
  weight ratios, founder spread, variation probability, and variation magnitude so a
  profile cannot silently script behavior or eliminate exploration.
- Parent identities are canonical and stable. Individual weights never participate in
  reproductive grouping. Parent memories, perceptions, learned action values,
  beliefs, categories, observer state, model output, wall time, and infrastructure
  timing never enter a heredity draw.
- Inherited weight scales the baseline action candidate first. Life-local action
  learning applies afterward and is never copied to an offspring.
- The offspring disposition is committed in the private reproductive-development
  start event. A birth copies that exact pending state and initializes perception
  memory, learned values, bodily loads, and update timestamps empty.
- Apply, snapshot, and replay validation recompute founders, pending offspring, and
  born descendants exactly. Generation and derivation tick are therefore verified
  facts rather than trusted metadata.
- Public projections omit profile identifiers, weights, generation, variation,
  parentage, and all reproductive mechanism detail.
- Event schema seventeen and snapshot/state-hash schema eighteen isolate the boundary.
  Body-profile plan schema two carries the species profile; schema one remains
  byte-compatible for ruleset-fourteen worlds.

## Consequences

Individuals can begin with different bounded tendencies and descendants can resemble
either parent without inheriting discoveries. The implementation remains constant
work per primitive action and per birth. Repeating a full species profile per
individual is acceptable for this correctness checkpoint but is explicit scale debt;
before very large populations, canonical profiles should be interned in a per-species
registry while retaining identical replay semantics.
