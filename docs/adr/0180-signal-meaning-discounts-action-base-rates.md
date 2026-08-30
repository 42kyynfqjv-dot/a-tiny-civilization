# ADR 0180: signal meaning discounts action base rates

Status: accepted on 2026-08-30.

## Context

The running ruleset-42 ordinary world produced and socially retained all 32 physical
call forms, but raw association strength collapsed every shared form onto `rest`.
This was not an absence of learning. In a verified disposable forecast from the
canonical world, individuals repeatedly learned associations with swallowing,
orienting, movement, biting, chewing, reaching, and applying force. Those rarer
associations did not spread durably because `rest` was also the most frequently
observed background action.

Ruleset 45's bounded form competition could escape a synthetic saturated fixture,
but a 1,560-tick forecast from the real snapshot retained only one shared raw
meaning. Allowing contextual imitation after two matching observations briefly
raised one non-rest mapping from one to three learners, then raw-frequency learning
collapsed it again. The remaining defect was therefore the association measure,
not a missing form, a fixed vocabulary slot, or insufficient random emission.

A form is informative about an action when it occurs unusually often with that
action relative to how often the listener observes the action at all. Comparing raw
form/action strength instead measures frequency and lets the commonest action claim
every form.

## Decision

Ruleset 46 introduces deterministic base-rate-corrected signal prediction for
ordinary worlds. For each listener and physical form, candidate meanings are ranked
by:

`association value / max(1, directly observed social-action count)`

The implementation uses exact unsigned integer cross-products. Candidate `c` is
uniquely more informative than alternative `a` only when:

`value(c) * observations(a) > value(a) * observations(c)`

for every alternative meaning of the same form. Equal ratios remain unclassified.
The bounded social-action counts are already canonical private evidence learned from
direct observation; no observer label or inferred semantic category enters the
world. For a fixed form, the ratio is proportional to `P(form | action)` and hence
to the form's lift over the action base rate.

The corrected prediction is used when a heard call biases the next primitive motor
action and when the engine evaluates shared grounded conventions. A recently heard
form may also receive the existing imitation weight in another motor context after
the listener has accumulated at least two direct form/action observations in that
context. This permits learned polysemy and cultural accommodation without assigning
a form, meaning, referent, goal, word, or language objective.

Signal occurrence and form draws advance to policy version 5. New ordinary worlds
default to ruleset 46. Cancer World remains excluded because its experiment
bootstrap does not use the ordinary-world language process.

The running ruleset-42 world activates the policy only for the transition into tick
5,600. All earlier transitions retain their historical raw-strength filter and
policy draws. The change adds no canonical state field, event kind, event schema, or
snapshot schema.

## Verification evidence

Before implementation, a read-only exact-ratio calculation over the disposable
tick-6,860 canonical-world forecast found naturally learned shared mappings for at
least four inhabitants across multiple distinct actions, including orienting,
swallowing, biting, chewing, and applying force. Raw-strength evaluation exposed
only `rest` as a shared meaning.

The forecast database was cloned and rewound to its verified tick-6,860 snapshot.
With the ruleset-46 engine, the first new transition opened the human lifecycle at
tick 6,861. A further 64 transitions verified through tick 6,925 with matching event
head and state hashes; the tick-6,923 snapshot durably recorded
`human_life_cycle_opened_at = 6861`.

This is a mechanics regression proof, not scientific validation and not a promise
that any particular vocabulary or later cultural outcome will emerge.

## Consequences

- Common actions no longer win every form merely because they are common.
- Rare meanings must still arise from direct, repeated physical and social evidence.
- Multiple meanings may compete for one form; exact ties remain unresolved.
- The lifecycle gate still requires three distinct grounded motor meanings shared by
  at least four living people with the existing minimum evidence.
- Replay remains bit-for-bit scoped by ruleset and the disclosed tick-5,600 boundary.
