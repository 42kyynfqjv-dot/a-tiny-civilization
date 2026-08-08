# ADR 0083: The ruleset-18 genesis path pins provisional composition 0.1.1

## Status

Accepted on 2026-08-08.

## Decision

Every active provisional-genesis entry point uses the immutable
`full-earth-breadth-first-0.1.1` composition. This includes the runner CLI default, seed-bound input
preparation, atomic PostgreSQL initialization, and root-owned container artifact staging.

Version 0.1.1 retains the seven global Earth layers and three earlier world-component releases but
replaces the fauna physiology v1 inspection summary with the normalized, source-pinned v2 physiology
catalog. Staging verifies the exact 9,842-byte composition and 1,047-byte catalog by SHA-256 before
copying either. The offline validator exhaustively verifies all eleven referenced artifacts.

Version 0.1.0 remains checked in and replayable; no existing world's manifest is rewritten.

## Consequences

- Ruleset-18 preparation cannot derive body commitments against one composition and initialize a
  world against another.
- The service-readable artifact set contains the actual normalized physiology input referenced by
  genesis, rather than its earlier inventory-only predecessor.
- Any future composition revision must update all four entry points atomically and preserve its
  predecessor for replay.
