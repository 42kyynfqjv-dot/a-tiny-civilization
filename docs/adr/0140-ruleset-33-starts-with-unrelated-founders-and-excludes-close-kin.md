# ADR 0140: ruleset 33 starts with unrelated founders and excludes close kin

Date: 2026-08-09

Status: Accepted

## Context

The ruleset-32 technical world began with two human founders. That is enough to exercise genesis,
replay, and the public observatory, but it is a poor population basis: every descendant would share
the same two founders and the simulator had no private physiological exclusion for close relatives.
Human observers must not need to intervene in pair selection, and the inhabitants must not receive
privileged concepts such as kinship, marriage, or reproduction.

## Decision

Ruleset 33 begins each new provisional world with 24 deterministic adult human founders: 12 in each
of the two source-bound reproductive categories. Founders have no parent IDs and are therefore
unrelated in canonical genealogy. Their IDs derive only from the committed world ID and fixed
founder ordinals; observer demand cannot select or alter them.

Before a reproductive development can begin, the engine privately compares the two organisms'
canonical parent graphs. Ruleset 33 excludes direct ancestors and descendants, siblings,
avuncular relations, and first cousins. This is a physiological eligibility constraint, not a
learned norm, action label, social role, or fact available to cognition.

The rule adds no event fields. Parent IDs were already append-only canonical history, so ruleset 33
retains the ruleset-32 event, state-hash, and snapshot schema versions. The changed pairing rule is
still pinned by the world manifest's ruleset version and remains replayable.

## Consequences

- The existing two-founder ruleset-32 world remains an immutable technical-world archive.
- A ruleset-33 successor needs fresh genesis proof and qualification before public cutover.
- Twenty-four founders reduce initial relatedness but do not replace the deterministic genealogy
  guard as the population develops.
- Reproduction remains private and non-explicit in public projections under the existing
  presentation policy.

