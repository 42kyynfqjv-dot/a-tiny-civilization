# Canonical input revision v23 — 2026-08-08

Ruleset 32 retains the exact adult-body-mass commitment in canonical organism events, state hashes,
and snapshots. This input rebuild is not a qualified candidate and authorizes neither activation
nor deployment.

- World ID: `b3ea736d-7a5a-5161-a74b-fa8c4302d333`
- Public seed: `18111088317882099744`
- Ruleset: 32
- Composition: `full-earth-breadth-first@0.1.2`
- Genesis directory: `/home/shmuel/codex/a-tiny-civilization-canonical-genesis/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset32-v23`
- `SHA256SUMS` digest: `9e5660afca617670ffcbd516bbaf4599aff5a32216406493beb012de9eb31edf`

The complete chain was independently rederived from the pinned global sources and retained local
occurrence evidence. All 14 JSON artifacts are byte-identical to v22, demonstrating that ruleset 32
changes only the event/state retention boundary and does not select a different world, population,
species set, environment, or physiological value.

The rebuild exposed that the checksum manifest generator retained absolute development-host paths.
Although every digest is correct, that manifest is not portable to a production directory. V23 is
therefore preserved as derivation evidence but cannot become a launch candidate. The generator is
corrected before the next immutable input revision.
