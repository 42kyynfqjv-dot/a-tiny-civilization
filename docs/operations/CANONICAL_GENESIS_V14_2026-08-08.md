# Canonical genesis v14: schema-6 surface-input proof

Date: 2026-08-08  
Source commit: `b44cd55`  
World: `b3ea736d-7a5a-5161-a74b-fa8c4302d333`  
Public seed: `18111088317882099744`  
Ruleset: 28

## Artifact closure

The same public seed and immutable origin were regenerated into
`b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset28-v14`. No seed, location, fauna ranking, or
founder limit changed. The complete 14-artifact JSON chain passed strict `SHA256SUMS` verification;
the checksum manifest digest is
`db9afd695ddfa051837901534dbf4c5a1634b235b964da68093b7b254cf1663d`.

The new origin-environment digest is
`b665567530191969284845d341a067934998a0b0820ce6ed8635abd9afb33dfd`. Its change is the expected
schema-2 local surface closure, and every downstream plan was rederived rather than edited.

## Disposable genesis proof

Database `canonical_candidate_v13` on the isolated qualification PostgreSQL instance was created
empty, migrated, and initialized through `scripts/initialize-provisional-world.sh`. Initialization
independently rederived the local occurrence intersection and ERA5 evidence, then traversed all
147,466 pinned provisional references totaling 10,164,215,509 bytes before committing genesis.

- sequence: 1
- tick: 0
- event head: `bce02666c38c750f18bc6cbc70c651833232dae410bd84389106871c2356c9d9`
- state hash: `7181efbf68e214cab390c995fa2419b5ba023473f657e6fe3874b359b9e41f03`
- genesis replay = snapshot plus tail = committed cursor
- configuration schema: 6
- committed terrain mean: 2,296,048 mm
- committed JRC occurrence source code: 0
- committed SoilGrids source vectors: 9

This is a tick-zero configuration proof, not the next full-duration mechanical/cognition
qualification. The candidate is disposable and remains isolated from the public service.

## Host note

During this proof, direct Docker access was unavailable and `sudo` reported invalid ownership for
its own executable. Already-running loopback PostgreSQL remained healthy, so candidate work
continued through the service interface without Docker. Host privilege-file ownership must be
repaired before any later operation that truly requires container lifecycle control; it did not
affect canonical bytes or this database proof.
