# Canonical input revision v24 — 2026-08-08

This is the first portable complete ruleset-32 input derivation. It is not yet a qualified
candidate and authorizes neither activation nor deployment.

- World ID: `b3ea736d-7a5a-5161-a74b-fa8c4302d333`
- Public seed: `18111088317882099744`
- Ruleset: 32
- Composition: `full-earth-breadth-first@0.1.2`
- Genesis directory: `/home/shmuel/codex/a-tiny-civilization-canonical-genesis/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset32-v24`
- `SHA256SUMS` digest: `76d54b0749bd9602c625c73d9f6eac78c21ca06865ece796976e49284e06a725`

The complete chain was rederived after correcting checksum-manifest path handling. Every manifest
entry is relative (`./artifact.json`), all 14 entries verify in place, and an independent copy to a
different temporary directory also verifies without rewriting the manifest. All JSON artifacts are
byte-identical to v22 and v23; only the portable checksum-manifest bytes differ.

Ruleset 32 will emit one private adult-body-mass commitment after every initialization and birth,
and will retain that exact fixed-point value in snapshots and state hashes. Database qualification,
fresh evidence, experimental-quality admission, observatory admission, activation, and deployment
remain separate fail-closed steps. V24 itself is input evidence only.
