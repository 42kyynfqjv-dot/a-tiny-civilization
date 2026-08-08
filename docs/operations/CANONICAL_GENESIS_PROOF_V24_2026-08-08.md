# Canonical database-free genesis proof v24 — 2026-08-08

This record captures the first complete ruleset-32 construction and replay proof for canonical
input revision v24. It is mechanical tick-zero evidence, not database-backed qualification,
scientific admission, activation, or deployment authorization.

## Identity

- Source checkpoint: `6fa2c0f`
- World ID: `b3ea736d-7a5a-5161-a74b-fa8c4302d333`
- Public seed: `18111088317882099744`
- Ruleset: 32
- Composition: `full-earth-breadth-first@0.1.2`
- Composition digest: `449ecf9e2956af072eaffbef4bd31c51160d4494d109a81eb5d7c485d187868f`
- Portable genesis-manifest digest:
  `76d54b0749bd9602c625c73d9f6eac78c21ca06865ece796976e49284e06a725`

## Complete input traversal

The verifier traversed and content-checked all 147,466 composition references, totaling
10,164,215,509 bytes. It also required the genesis manifest to cover every and only regular,
non-symlink artifact under portable `./artifact.json` paths.

## Constructed genesis

- Durable organisms: 66
- Material instances: 3
- Scientific dataset commitments: 15
- Genesis event records: 139
- Sequence: 1
- Tick: 0
- Event-batch digest: `ddda1c6a6caaae6194780580acb011a2a234b3b5e1a423494d4d1e8388ccab58`
- State digest: `72573bbe2381edb2e8524cd21a3dcb2f2a9ba0856dec2d5486aff28c2e222e44`
- Snapshot schema: 32

The shared side-effect-free constructor produced the batch and snapshot used by persistent
initialization. Independent event-zero replay and snapshot-with-empty-tail replay both reproduced
the constructed state and exact state digest.

## Defect caught before persistence

The first exhaustive attempt rejected an obsolete loader assumption that one profile plan could
reference only one non-assumption body-mass source set. The plan correctly retains multiple source
compilations. Ruleset 32 now pins each distinct source-set digest under stable ordered manifest
keys; pre-32 construction preserves its singular contract. The corrected complete proof above then
passed.

## Remaining gates

This proof intentionally made no database connection and changed no external state. A launch
candidate still needs a fresh PostgreSQL genesis, bounded advancement, full replay and snapshot-tail
verification, Hindsight and cognition exercise, projection and observatory checks, immutable
qualification evidence, quality-world admission, literal activation confirmation, and production
deployment verification.
