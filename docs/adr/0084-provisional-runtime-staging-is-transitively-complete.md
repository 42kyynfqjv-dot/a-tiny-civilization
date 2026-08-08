# ADR 0084: Provisional runtime staging is transitively complete

## Status

Accepted on 2026-08-08.

## Decision

A provisional release whose top-level artifact is a tile index is not verified by hashing that
index alone. The filesystem verifier now resolves the index within its release namespace and walks
every child index and leaf. It verifies canonical index encoding, content hashes and lengths,
duplicate/cycle freedom, parent containment, layer/container binding for known tile formats, and a
ten-million-artifact safety bound. Identical top-level releases, such as the shared ETOPO
elevation/bathymetry tree, are verified once and conflicting aliases fail.

The active 0.1.1 composition has been exhaustively verified as 147,466 unique referenced artifacts
and 10,164,215,509 bytes. Runtime staging derives that verified closure instead of maintaining a
root-only hand list. It adds the two separately pinned DE441 BSP files required by live celestial
evaluation, producing 147,469 paths including the composition itself. Files are copied through one
archive stream into a fresh destination, assigned root:GID-10001 ownership and restrictive modes,
then the staged composition is exhaustively verified again.

## Consequences

- A missing or changed transitive tile prevents both initialization and staging.
- The runner no longer receives index roots whose leaves are absent from its read-only mount.
- Normal CI uses miniature recursive fixtures and immutable top-level pin checks. The real 10 GB
  closure is rechecked explicitly before genesis and automatically on both sides of staging.
