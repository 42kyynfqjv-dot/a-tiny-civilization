# Canonical qualification v18 evidence — 2026-08-08

This record proves ruleset 30 and the observer finding-v2 rebuild against the exact source intended
for activation. It is mechanical qualification evidence, not permission to deploy and not
scientific admission. No public or production world was changed while producing it.

## Immutable identity

- World: `b3ea736d-7a5a-5161-a74b-fa8c4302d333`
- Seed: `18111088317882099744`
- drand round: `31125752`
- Ruleset and event schema: `30`
- Source commit: `0d9d60619b7d6d762aa55bab86beb97f3eec3d6a`
- Genesis checksum-manifest digest:
  `36f92754e0e50c7bfc018c303f57b670f0320ba01452d013a5b9820afb27d4d9`

The fresh disposable PostgreSQL database `canonical_candidate_v18` independently traversed all
147,466 references totaling 10,164,215,509 bytes. Genesis reproduced sequence `1`, tick `0`, event
head `046f3aebc6352e94ab75e9d0baecdeca5005034319245bc2c4d033ce96fafc34`, and state hash
`c77534f6431c0d0e6a88290f4beeae84e86a4d71c81988b8508a5f7c58d23911`.

## Tick-1,000 proof

The candidate advanced through exactly 1,000 ticks with Hindsight and Ollama on loopback, remote
export unapproved, paid dispatch disabled, and two zero-cost local-model receipts recorded as
nondeterministic inputs before their deterministic deadlines.

- final sequence / tick: `1018 / 1000`
- event head:
  `cbb54e8e77d9264d66d73ee19e04c0d9c4a00a5faf2ba6d48d22ed0dd0542a9d`
- state hash:
  `2045f73ef3a5601b257de06d1862c9515a1ff2e4540b32d7762a7288ea7e06ff`
- Hindsight memories: `4000` delivered, `0` pending, `0` errors
- cognition: `17` requests, `16` due and completely latched/consumed, `2` model receipts,
  `0` non-person requests
- observer projections: all `5` current at sequence `1018`
- observer content: `66` organisms, `67` timeline items, `8` deterministic findings, and `23`
  region-bound artifact traces
- finding-v2 rebuild: the prior six facts plus factual tick-100 and tick-1,000 human-presence
  streaks, each sourced to a committed tick event
- replay and snapshot-tail state verification: passed
- observer candidate smoke: passed with `1` artifact, `8` findings, `66` organisms, and `9` wiki
  entries; all public privacy checks passed

The final state hash exactly matches v17. The event head intentionally differs because the event log
contains the newly recorded cognition input; replay consumes that record rather than calling a
model. The finding-v2 projection is observer-only and has no path into this state.

## Retained evidence and launch freeze

The immutable bundle is retained outside the repository at
`a-tiny-civilization-qualification-evidence/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset30-v18-tick1000`.
Every file passes its strict `SHA256SUMS`; that manifest's digest is
`9d6b4736c648fbbb5cb2b37fbd5d79ec23e276fe56cb78fc31c24ddd401e2675`. The bundle's
`qualification-status.json` digest is
`6a1bf3d515dcc3809327c8ab53b8b568f57dfbae5c2576bfbf96c11f3d453227`, and the bundle declares
that it contains no canonical event payloads.

Quality-admission schema 2 now binds activation to source commit
`0d9d60619b7d6d762aa55bab86beb97f3eec3d6a` and the enumerated qualified paths. Any difference in
that boundary requires a fresh qualification and admission.
