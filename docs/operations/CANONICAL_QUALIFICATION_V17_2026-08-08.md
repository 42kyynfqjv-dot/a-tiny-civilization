# Canonical qualification v17 evidence — 2026-08-08

This record proves the ruleset-30 candidate against the exact source intended for activation. It is
mechanical qualification evidence, not permission to deploy and not scientific admission. No
public or production world was changed while producing it.

## Immutable identity

- World: `b3ea736d-7a5a-5161-a74b-fa8c4302d333`
- Seed: `18111088317882099744`
- drand round: `31125752`
- Ruleset and event schema: `30`
- Source commit: `9c773be13d372bfe06710b2eb50b68c0b21cc791`
- Genesis checksum-manifest digest:
  `36f92754e0e50c7bfc018c303f57b670f0320ba01452d013a5b9820afb27d4d9`

The exact-current-code run used a fresh disposable PostgreSQL database named
`canonical_candidate_v16`. Genesis reproduced sequence `1`, tick `0`, event head
`046f3aebc6352e94ab75e9d0baecdeca5005034319245bc2c4d033ce96fafc34`, and state hash
`c77534f6431c0d0e6a88290f4beeae84e86a4d71c81988b8508a5f7c58d23911`.

## Tick-1,000 proof

The candidate advanced through exactly 1,000 ticks with Hindsight and Ollama on loopback, remote
export unapproved, paid dispatch disabled, and two zero-cost local-model receipts recorded as
nondeterministic inputs before their deterministic deadlines.

- final sequence / tick: `1018 / 1000`
- event head:
  `5ced614fbab8545b72ee8b7e59aa96c563d890ed8e9e54d734b8135d520166a1`
- state hash:
  `2045f73ef3a5601b257de06d1862c9515a1ff2e4540b32d7762a7288ea7e06ff`
- Hindsight memories: `4000` delivered, `0` pending, `0` errors
- cognition: `17` requests, `16` due and completely latched/consumed, `2` model receipts,
  `0` non-person requests
- observer projections: all `5` current at sequence `1018`
- observer content: `66` organisms, `67` timeline items, `6` deterministic findings, and `23`
  region-bound artifact traces
- replay and snapshot-tail state verification: passed
- observer candidate smoke: passed with nonempty artifacts, findings, organisms, and wiki

The state hash exactly matches the earlier ruleset-30 qualification. The event head intentionally
differs because the event log contains the newly recorded cognition input; replay consumes that
record rather than calling a model.

## Retained evidence and launch freeze

The immutable bundle is retained outside the repository at
`a-tiny-civilization-qualification-evidence/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset30-v17-tick1000`.
Every file passes its strict `SHA256SUMS`; that manifest's digest is
`e5ca7ba30dad45f07bd651a9510f24315cfb45ef73278e632bd9e08d6d5f5855`. The bundle's
`qualification-status.json` digest is
`dc62d7e09d1edd828defdc28975142bb63b9bd3754551cc5b5f38e1471651322`, and the bundle declares
that it contains no canonical event payloads.

Quality-admission schema 2 binds activation to this exact source commit and to the enumerated
simulation, schema, canonical-data, and qualification paths. Any committed, staged, unstaged, or
untracked change inside that boundary fails launch verification until a fresh qualification bundle
and admission replace this one. Operations and documentation can continue independently, but they
cannot silently change the exercised world mechanics.
