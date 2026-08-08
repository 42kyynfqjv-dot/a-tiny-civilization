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

No public or production world was changed. Full-duration Hindsight, cognition, mechanics,
projection, replay, and observer-smoke qualification remains required before this candidate can
supersede the retained ruleset-29 launch evidence.
