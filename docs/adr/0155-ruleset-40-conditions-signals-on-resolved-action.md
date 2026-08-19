# ADR 0155: Ruleset 40 conditions signals on resolved action

## Status

Accepted and implemented on 2026-08-19. It is the default for newly created
ordinary worlds. The running public ruleset-39 world activates the same stateless
correction at tick 2,845; Cancer World remains unchanged on ruleset 38.

## Context

Ruleset 39 separated a physical call from the motor action it accompanied, but it
weighted the call form using the organism's most-weighted prospective action before
the independent motor draw. The subsequently resolved action could differ. Listeners
therefore learned a noisy form/action mapping even while directly observing both.
The live population produced many calls but did not converge on conventions.

## Decision

- Select and resolve the organism's physical motor action first.
- Decide whether a call occurs and select its acoustic form only afterward.
- Weight form reuse from private associations with that exact resolved action.
- Give a directly heard form a neutral conformist imitation advantage and give a
  distinctive learned form a bounded reuse advantage. Unlearned forms remain equal.
- Add no word, referent, intent, meaning, or observer-provided vocabulary.
- Change the signal draw version at the activation boundary so replay reproduces the
exact historical policy on either side of tick 2,845.

## Consequences

Shared forms can now converge through repeated situated imitation rather than noisy
prediction. Convergence remains contingent: the world may develop, change, or lose
conventions, and the observer detector still requires independent evidence before it
reports language.

Ruleset-39 history before tick 2,845 remains byte-for-byte reproducible. Ruleset 40
uses the corrected draw from genesis. Explicit experiment worlds do not activate it.
