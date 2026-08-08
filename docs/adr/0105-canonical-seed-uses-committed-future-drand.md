# ADR 0105: canonical seed uses committed future drand

## Status

Accepted.

## Context

Previewing several seeds and keeping an appealing world would curate history before tick zero. A
host RNG, operator-chosen number, commit hash, or wall clock can also be influenced by the operator.
The project needs one public procedure that is committed before its input exists and can be
independently verified afterward.

## Decision

`civilization-data seed commit` pins the League of Entropy quicknet chain hash, BLS public key,
RFC9380 unchained scheme, three-second period, and genesis time. It fetches the latest beacon from
Protocol Labs, fetches that exact round independently from Protocol Labs and Cloudflare, requires
byte-identical responses, verifies the BLS signature, and verifies that randomness is SHA-256 of the
signature. The selected target must be at least 200 rounds—ten minutes—later. The new commitment
artifact is then published in Git before that target round.

`civilization-data seed resolve` refuses to run before the committed round time. It fetches the exact
round from both relays, requires equality, repeats signature and randomness verification, and never
replaces an output. It derives:

```text
SHA-256(
  "a-tiny-civilization/world-seed/v1" || 0x00 ||
  quicknet_chain_hash || round_u64_be || randomness
)
```

The first eight digest bytes, interpreted as an unsigned big-endian integer, are the world seed. A
UUIDv5 under the standard URL namespace, named by
`https://atinycivilization.com/worlds/{derivation_digest}`, is the world ID. The complete digest,
beacon, commitment digest, derived identity, and relay identities remain in the resolution artifact.

## Consequences

The target world is accepted without preview or reroll. An inconvenient origin, ecology, or early
history is still history. Relay disagreement, invalid cryptography, premature resolution, changed
constants, malformed JSON, redirects, oversized responses, or an existing output fail closed. Relay
unavailability delays resolution; it does not authorize a substitute seed. Scientific or mechanical
failure after genesis follows the published world lifecycle rather than changing this seed.
