# ADR 0167: situated call reuse breaks the circular language gate

Date: 2026-08-30

Status: Accepted

## Context

The ruleset-42 ordinary world reached tick 4,759 without a public convention.
The language projection was working as designed: within its current 1,152-tick
window, all 32 atomic call forms remained close to the action base rates. The
strongest apparent ordered pattern had only nine observations and every leading
pattern denoted the population's most common action, rest.

The production policy contained a circular gate. An inhabitant reused a learned
call only when that call was already the unique strongest mapping both from form
to action and from action to form. Random physical calls therefore had to become
a convention before repeated production could create the convention. Long-run
positive association values also saturated, erasing the margins used by that
gate. Existing tests proved propagation, learning, composition, replay, and a
manually populated lifecycle threshold, but did not prove unassisted convergence.

## Decision

Ruleset 43 adds situated frequency-biased call reuse:

- for the action an inhabitant is actually performing, a call becomes reusable
  after that inhabitant has directly observed it with the same action twice;
- the most frequently observed form for that situated action receives a bounded
  leader bias, while repeated evidence supplies a bounded graded bias;
- direct imitation, competing hypotheses, physical signal production, and
  compositional calls remain in place;
- no call form, meaning, word, vocabulary size, language objective, or discovery
  is assigned by the observer;
- a deterministic 24-person test must form three shared grounded meanings without
  injected cognition or a supplied lexicon;
- the internal lifecycle gate requires repeated evidence for each mapping under
  the repaired driver rather than treating one novel coincidence as language.

The running ruleset-42 world activates the stateless production repair at tick
5,000. Ticks before that boundary retain their original policy draw and hashes.
New ordinary worlds default to ruleset 43. The event, state-hash, and snapshot
schemas do not change because the repair reads existing durable association
observation counts and changes only future deterministic selection weights.

## Consequences

- Familiar calls can become conventions through ordinary reuse instead of needing
  to be conventions first.
- Which forms win remains contingent on the seed, embodied behavior, encounters,
  and accumulated social evidence.
- The public detector remains independent and conservative; it still requires
  persistence, multiple learners and sources, dominance over the action baseline,
  and three distinct meanings before displaying a language candidate.
- Ruleset-42 replay changes only at the disclosed tick-5,000 activation boundary.
- Cancer World is unaffected because experiment worlds do not use the ordinary
  contemporaneous-language driver.
