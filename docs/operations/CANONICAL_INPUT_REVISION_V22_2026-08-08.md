# Canonical input revision v22 — 2026-08-08

This is the first complete ruleset-31 input derivation. It is not yet a qualified candidate and
authorizes neither activation nor deployment.

- World ID: `b3ea736d-7a5a-5161-a74b-fa8c4302d333`
- Public seed: `18111088317882099744`
- Ruleset: 31
- Composition: `full-earth-breadth-first@0.1.2`
- Genesis directory: `/home/shmuel/codex/a-tiny-civilization-canonical-genesis/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset31-v22`
- `SHA256SUMS` digest: `8dae33db2ee11a9442ada0f70d2ce0fee190136e67f356d972128c1080918364`

All 33 organism profiles now carry body-mass-scaled metabolic power. Twenty-four are literature
approximations based on source-informed mass and nine remain engineering assumptions because their
mass is assumed. Representative committed values are 148.461427 W for Homo sapiens, 0.614764 W
for `Junco phaeonotus`, and 0.187378 W for `Selasphorus rufus`. Usable energy reserve is exactly
seven simulation days of committed power, rounded up to a joule.

Glucose and water transfer quantities are 1% of committed adult mass, rounded up to a milligram.
For example, the same neutral oral action transfers 700,000 mg for the human profile and 35 mg for
the hummingbird profile. The glucose-energy coefficient, hydration duration, reservoir quantities,
and all nine uncovered masses remain explicit engineering assumptions.

The complete input chain rederived successfully after correcting both `aves_1` and `aves_2` to the
endotherm branch. Full database qualification remains outstanding; the previous ruleset-30 quality
admission remains frozen and rejects this source/ruleset boundary.
