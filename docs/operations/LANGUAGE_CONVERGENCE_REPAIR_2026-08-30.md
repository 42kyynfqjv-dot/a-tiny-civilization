# Ordinary-world language convergence repair — 2026-08-30

## Diagnosis

At sequence 4,840 / tick 4,759, the ruleset-42 ordinary world still reported
`undetected`. The detector was current and behaving correctly. In its 1,152-tick
window, every atomic form remained near the action base rate: rest accounted for
39.4% of eligible evidence, while the strongest atomic forms denoted rest only
24.6%–55.3% of the time. No form reached the required 60% dominance plus baseline
margin and lift.

The production policy required an already mutually distinctive form/action mapping
before an inhabitant could preferentially reuse a familiar call in that situation.
Long-run positive association values had also saturated. The combination made
convergence circular and left production effectively fragmented across 32 forms.

## Repair

Source checkpoint `9ca14af` implements ADR 0167. Ruleset 43 uses bounded situated
frequency-biased reuse of directly observed calls, and the running ruleset-42 world
activates the same stateless repair at tick 5,000. No word, form, meaning, lexicon,
language objective, cognition response, or observer projection is fed into the
world. Existing event, state-hash, and snapshot schemas remain unchanged.

The regression suite includes an unassisted 24-person simulation that must form
three shared grounded meanings within 512 ticks. The full 98-test engine suite,
21-test runner suite, strict Clippy checks, formatting, and diff validation passed.

## Deployment and live evidence

- Replacement image: `sha256:87cc0c3bc9f34b49b310eba9a126dbbc1eb81679b7c203c50be28a0cecfd37ca`
- Runner history verification before resume: sequence 4,857 / tick 4,776
- First replacement-image commit: sequence 4,858 / tick 4,777
- Second replacement-image commit: sequence 4,859 / tick 4,778
- Runner restart count: zero
- A post-deployment full replay verifier completed successfully.

Before the tick-5,000 repair boundary, accumulated history independently crossed
the first public threshold. The detector reported `proto_lexicon` with ordered call
`[27, 5]` associated with rest: 12 evidence events, nine learners, five sources,
100% dominance, and evidence spanning ticks 3,671–4,772. This is one convention,
not yet the three-distinct-meaning `rudimentary_language_candidate` threshold.
The repaired policy is intended to let the population extend beyond that
rest-dominated convention through its own situated reuse after tick 5,000.
