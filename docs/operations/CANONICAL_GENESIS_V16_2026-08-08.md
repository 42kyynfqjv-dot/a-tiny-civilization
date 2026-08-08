# Canonical genesis v16 evidence — 2026-08-08

This record proves the ruleset-30 tick-zero candidate for the already committed canonical world. It
is qualification evidence, not permission to deploy and not scientific admission.

## Immutable identity

- World: `b3ea736d-7a5a-5161-a74b-fa8c4302d333`
- Seed: `18111088317882099744`
- drand round: `31125752`
- L10 origin: `8683550000000000`
- L23 embodied origin: `868354e9dff6c000`
- Ruleset and event schema: `30`
- World-configuration schema: `6`

The candidate artifacts are retained outside the repository at
`a-tiny-civilization-canonical-genesis/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset30-v16`.
All 14 JSON artifacts pass the directory's strict `SHA256SUMS`; the checksum-manifest digest is
`36f92754e0e50c7bfc018c303f57b670f0320ba01452d013a5b9820afb27d4d9`.

## Source-bound surface commitment

- Origin-environment digest:
  `b665567530191969284845d341a067934998a0b0820ce6ed8635abd9afb33dfd`
- ETOPO terrain minimum / mean / maximum:
  `2228633 / 2296048 / 2364719 mm`
- JRC surface-water occurrence source code: `0`
- SoilGrids coarse-fragment Q0.05 / Q0.5 / Q0.95 source values: `0 / 30 / 574`
- SoilGrids topsoil closure: all nine required properties, three retained quantiles each

Ruleset 30 preserves ruleset 29's provisional ETOPO movement factor and adds the exact SoilGrids
coarse-fragment median as a second private movement-load factor under ADR 0126. It emits no soil,
terrain, route, difficulty, property, or use label. JRC and the other eight soil properties remain
causally unread.

## Database proof

The fresh disposable PostgreSQL database `canonical_candidate_v15` independently traversed
147,466 composition references totaling 10,164,215,509 bytes and committed exactly one genesis
batch from source commit `0f6e417c4e6056214fc121bea44c75c3b062a4cc`:

- sequence / tick: `1 / 0`
- event head:
  `046f3aebc6352e94ab75e9d0baecdeca5005034319245bc2c4d033ce96fafc34`
- state hash:
  `c77534f6431c0d0e6a88290f4beeae84e86a4d71c81988b8508a5f7c58d23911`
- verification: genesis replay equals snapshot plus tail equals the committed cursor

## Bounded qualification proof

The same isolated candidate advanced through exactly 1,000 ticks only after a real zero-cost
loopback Qwen result was durable before its fixed simulated-time deadline. Hindsight and Ollama
remained on loopback, remote export was unapproved, and paid dispatch was disabled.

- final sequence / tick: `1018 / 1000`
- event head:
  `7e556a3becb3149051206c569ef93c9e2dc7c3a00c1fa9ec526fcd84c53cce64`
- state hash:
  `2045f73ef3a5601b257de06d1862c9515a1ff2e4540b32d7762a7288ea7e06ff`
- event batches / snapshots: `1018 / 17`
- Hindsight memories: `4000` delivered, `0` pending, `0` errors
- cognition: `17` requests, `16` due and completely latched/consumed, `2` actual model receipts,
  `0` non-person requests
- observer projections: all `5` current at sequence `1018`
- observer content: `66` organisms, `67` timeline items, `6` deterministic findings, `23`
  region-bound artifact traces
- exercised mechanics include 10,161 directed moves, 20,293 varied signals, 3,768 learned
  signal/action associations, 707 signal/motor associations, and 66,000 each of water-flux and
  air-motion perceptions
- machine-readable qualification: `passed: true`
- observer candidate smoke: passed with nonempty timeline, findings, organisms, artifact archive,
  and wiki; all five projection lags were zero and public commitments contained no event payloads

The immutable retained bundle is
`a-tiny-civilization-qualification-evidence/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset30-v16-tick1000`.
Its `SHA256SUMS` digest is
`b31d82abf6fd73c646e755cdfb289130d02cf2ad6ceddbc315a38eea6d23c444`; every covered file
verifies. The bundle binds source commit `c66b1ba267a0995b96e435255fbdd5a9d7f36944`, the genesis
manifest digest, and qualification-report digest
`5341528eaf249ead702b7221d79d01b100b6ba542675982c81d846c8c3117cd7`, and declares that it
contains no canonical event payloads.

No public or production world was changed. This candidate supersedes the retained ruleset-29
mechanical evidence for launch review but does not itself authorize deployment or claim scientific
admission.
