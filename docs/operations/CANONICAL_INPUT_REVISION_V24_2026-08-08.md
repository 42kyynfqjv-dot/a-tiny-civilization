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

The runner now also has a database-free canonical proof command:

```bash
cargo run -p civilization-runner -- verify-provisional-genesis \
  --world-id b3ea736d-7a5a-5161-a74b-fa8c4302d333 \
  --seed 18111088317882099744 \
  --genesis-directory /home/shmuel/codex/a-tiny-civilization-canonical-genesis/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset32-v24 \
  --ruleset-version 32
```

It uses the same side-effect-free genesis constructor as persistent initialization, verifies the
portable manifest and full content-addressed Earth tree, then requires event-zero replay and
snapshot replay to reproduce the constructed state. This closes the mechanics-verification gap
caused by local PostgreSQL socket restrictions; it does not replace database-backed bounded-tick
qualification or launch admission.

The complete database-free proof passed after correcting the ruleset-32 multi-source provenance
boundary:

- portable genesis-manifest digest:
  `76d54b0749bd9602c625c73d9f6eac78c21ca06865ece796976e49284e06a725`;
- verified composition artifacts: 147,466 totaling 10,164,215,509 bytes;
- organisms: 66;
- material instances: 3;
- scientific dataset commitments: 15;
- genesis events: 139;
- event batch hash: `ddda1c6a6caaae6194780580acb011a2a234b3b5e1a423494d4d1e8388ccab58`;
- state and snapshot hash:
  `72573bbe2381edb2e8524cd21a3dcb2f2a9ba0856dec2d5486aff28c2e222e44`; and
- event-zero replay equals schema-32 snapshot replay: true.

This is a complete tick-zero mechanics and replay proof for v24. The absence of a PostgreSQL write
is intentional; persistence, advancement, projection, memory/cognition, admission, activation, and
deployment remain distinct gates.
