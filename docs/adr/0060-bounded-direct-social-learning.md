# ADR 0060: Social learning begins with bounded direct action observation

## Status

Accepted on 2026-08-08 as ruleset 18.

## Context

Ruleset 13 lets an organism associate its own primitive action with a change in its
own bodily pressure. Neutral signals can propagate, but no organism can yet retain
anything another organism did. Without a social-learning substrate, behavior cannot
spread between lives and culture cannot emerge even in principle.

Adding words, purposes, inventions, or observer-authored meanings would violate the
causal-openness contract. Letting every organism attend to every co-located action
would also make dense gatherings produce quadratic events.

## Decision

- At most once per simulated tick, a living organism may directly observe one other
  living organism that began the transition on the same exact embodied patch.
- Attention is selected from the complete stable-ID-ordered patch group by a canonical
  digest of the world seed, next tick, observer identity, and group digest. Host order,
  wall time, observer activity, and model output have no role.
- The retained state contains only the witnessed closed-grammar primitive action kind,
  an observation count, and a bounded positive tendency. It contains no purpose,
  success claim, material label, word, invention, relationship, or observer category.
- Social tendency modifies the next action draw through the same bounded nonzero-
  exploration weighting used for individual outcome learning. It never makes an
  action mandatory or creates a new action.
- Active organisms are grouped by S2 patch once per tick. One event per eligible
  observer bounds canonical event growth linearly; patch-group hashing and selection
  do not scan the whole population per observer.
- The event and retained state are private. Public timeline, organism, and finding
  projections discard them.

## Version boundary

Ruleset 18 uses event schema 20 and snapshot/state-hash schema 21. Ruleset 17 histories
remain byte-for-byte replayable. A missing, duplicated, self-directed, nonlocal,
wrong-action, reordered, or arithmetically altered social transition fails before
commit. Newborns begin with empty social learning state; it is neither genetic nor
inherited.

## Consequences

This is a substrate for behavioral diffusion, not language or teaching. Exact-patch
co-presence is the current perception abstraction; occlusion, directed attention,
relationships, signal-action association, demonstration outcomes, forgetting, and
multi-step imitation remain later mechanics. None may retroactively change a ruleset-18
world.
