# ADR 0106: Canonical first genesis requires an exclusive store

Date: 2026-08-08

Status: Accepted

## Context

The PostgreSQL schema deliberately supports multiple worlds and immutable extinct-world archives.
That does not mean the first public observatory may reuse a development database containing proof
or qualification histories. If it did, those worlds would become visible through ordinary observer
projections and the meaning of “first public history” would be ambiguous.

## Decision

- `init-provisional-full-earth` accepts an explicit `--refuse-other-worlds` guard.
- The guard permits an empty store or a byte-identical retry of the requested world ID, but rejects
  every store containing a different world before loading or verifying genesis artifacts.
- `initialize-canonical-world.sh` always enables the guard. The lower-level provisional initializer
  enables it only when `ATINY_REFUSE_OTHER_WORLDS=1`, preserving disposable multi-world test paths.
- Successor-world creation remains a distinct, explicitly authorized operation. It is not smuggled
  through the first-genesis wrapper.

## Consequences

The canonical candidate and eventual production database cannot accidentally publish stale local
history. Operators retain retry safety after a partial command failure because the same derived
world ID is allowed. Multi-world archive support is unchanged and can be activated later through
the documented successor path.
