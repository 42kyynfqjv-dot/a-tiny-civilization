# Canonical database-free genesis proof — ruleset 33 v1 — 2026-08-09

This record captures the exact construction and replay proof for the first ruleset-33 successor
candidate. It is tick-zero mechanical evidence, not database qualification, scientific admission,
activation, or deployment authorization.

## Identity

- Source checkpoint: `695902bab040a6ac3eff8c4ee9ba621e580e3f22`
- World ID: `286f5398-7903-5fe4-a108-51ada3e87754`
- Public seed: `4608635557644240168`
- Ruleset: 33
- Composition: `full-earth-breadth-first@0.1.2`
- Composition digest: `449ecf9e2956af072eaffbef4bd31c51160d4494d109a81eb5d7c485d187868f`
- Portable genesis-manifest digest:
  `d2da84e526f335430bd71947a50215d3c0670a34ddb060e1cd30d5ff5dfa58fd`

## Complete input traversal

The verifier content-checked 147,466 composition references totaling 10,164,215,509 bytes and
required the portable manifest to cover every regular, non-symlink genesis artifact.

## Constructed genesis

- Human founders: 24
- Individually tracked fauna founders: 64 across 32 real species
- Durable organisms: 88
- Material instances: 3
- Scientific dataset commitments: 15
- Genesis event records: 183
- Sequence: 1
- Tick: 0
- Event-batch digest: `b92a502521ec99d2913da3a3e6c0b3172414eb4a9e08e5053007c1d0217090ea`
- State and snapshot digest: `40e3137e9f0f458e09acf632397bd7a19203f1f9bda8c3a39910ce0759db19ac`
- Snapshot schema: 32

Snapshot schema 32 is intentionally reused because ruleset 33 changes founder construction and
kinship policy without changing the serialized state shape. Independent event-zero replay and
snapshot-with-empty-tail replay both reproduced the constructed state and exact digest.

The proof command was:

```bash
target/release/civilization-runner verify-provisional-genesis \
  --world-id 286f5398-7903-5fe4-a108-51ada3e87754 \
  --seed 4608635557644240168 \
  --genesis-directory /home/shmuel/codex/a-tiny-civilization-canonical-genesis/286f5398-7903-5fe4-a108-51ada3e87754-ruleset33-v1 \
  --ruleset-version 33
```

## Remaining gates

No database or public-world state changed during this proof. The candidate still requires isolated
PostgreSQL initialization, bounded advancement, full and snapshot-tail replay, cognition and memory
exercise, projection checks, immutable qualification evidence, fresh admission, scientific review,
explicit activation, and production cutover verification.

