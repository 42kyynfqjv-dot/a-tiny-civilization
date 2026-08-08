# Canonical genesis v15 evidence — 2026-08-08

This record proves the ruleset-29 tick-zero candidate for the already committed canonical world. It
is qualification evidence, not permission to deploy and not scientific admission.

## Immutable identity

- World: `b3ea736d-7a5a-5161-a74b-fa8c4302d333`
- Seed: `18111088317882099744`
- drand round: `31125752`
- L10 origin: `8683550000000000`
- L23 embodied origin: `868354e9dff6c000`
- Ruleset and event schema: `29`
- World-configuration schema: `6`

The candidate artifacts are retained outside the repository at
`a-tiny-civilization-canonical-genesis/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset29-v15`.
All 14 JSON artifacts pass the directory's strict `SHA256SUMS`; the checksum manifest digest is
`19ef651493db22c4616352da7a65d774e8be11d15b43a4b7e4aabc1fec3039cf`.

## Source-bound surface commitment

- Origin-environment digest:
  `b665567530191969284845d341a067934998a0b0820ce6ed8635abd9afb33dfd`
- ETOPO terrain minimum / mean / maximum:
  `2228633 / 2296048 / 2364719 mm`
- JRC surface-water occurrence source code: `0`
- SoilGrids topsoil vectors: all nine required properties, three retained quantiles each

These inputs remain explicitly provisional. Ruleset 29 gives only the ETOPO relief range a causal
effect under ADR 0125. It does not infer a route, slope, habitat, resource, or organism concept from
the JRC or SoilGrids fields.

## Database proof

The fresh disposable PostgreSQL database `canonical_candidate_v14` independently traversed
147,466 composition references totaling 10,164,215,509 bytes and committed exactly one genesis
batch:

- sequence / tick: `1 / 0`
- event head:
  `d2a6ac49351b82283334a2c483b6f3e4b3760de71004a2198802c8941cb8377e`
- state hash:
  `ee0b304549c8591c725efe7043901cb2fdff475c260c1f821c888836ca16d23c`
- stored configuration proof: schema `6`, terrain mean `2296048 mm`
- verification: genesis replay equals snapshot plus tail equals the committed cursor

The first stored replay exposed a duplicated schema selector that still expected schema 28. Commit
`413ae34` fixes that boundary, adds regression coverage, passes the complete project gate, and
successfully verifies the same already-committed candidate rather than replacing it.

## Bounded qualification proof

The same isolated candidate was then advanced through exactly 1,000 ticks. The runner paused after
tick 1 until a real loopback Qwen result was durable before its fixed simulated-time deadline.
Ollama v0.32.6 was installed in a workspace-local directory from the official Linux asset whose
SHA-256 is `dec2fa50d24e6868ca3c4c977d69d059399372105f951a9acc320a5a79aadcfc`.
The retained `qwen2.5:1.5b` model digest is the project-pinned
`65ec06548149b04c096a120e4a6da9d4017ea809c91734ea5631e89f96ddc57b`.
Inference and Hindsight stayed on loopback; paid cognition was disabled.

- final sequence / tick: `1018 / 1000`
- event head:
  `1081077d11f58e5fa5360c414abe3f3cbca73a06faf99fc334feb437d29abbe7`
- state hash:
  `8a9bb4b0d55425405eba688d8199f35cf4618c47e1eb730f600de37e64649193`
- event batches / snapshots: `1018 / 17`
- Hindsight memories: `4095` delivered, `0` pending, `0` errors
- cognition: `17` requests, `16` due and completely latched/consumed, `2` actual model
  receipts, `0` non-person requests
- observer projections: all `5` current at sequence `1018`
- observer content: `66` organisms, `67` timeline items, `6` deterministic findings, `37`
  region-bound artifact traces
- exercised mechanics include 10,421 directed moves, 20,279 varied signals, 3,701 learned
  signal/action associations, 684 signal/motor associations, and 66,000 each of water-flux and
  air-motion perceptions
- machine-readable qualification: `passed: true`

The immutable retained bundle is
`a-tiny-civilization-qualification-evidence/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset29-v15-tick1000`.
Its `SHA256SUMS` digest is
`60d719d9bf2225e283da0d45147edb7d93a2c26d44757a886dabf5097bd89882`; every covered file
verifies. The bundle binds source commit `0dcc482f324d3eb8f2c77cf05a78167071b437ce`, the genesis
manifest digest, and the passing qualification-report digest, and declares that it contains no
canonical event payloads.

No public or production world was changed by this procedure. Passing this bounded candidate gate
does not itself authorize launch; the quality review, retained evidence bundle, production
configuration, and deliberate genesis activation remain separate gates.
