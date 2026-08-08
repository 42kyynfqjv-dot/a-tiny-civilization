# ADR 0128: Deployment revalidates the staged Earth inputs

## Status

Accepted on 2026-08-08.

## Context

The root-only staging command validates every content-addressed provisional reference and the two
DE441 segments before publishing a fresh runtime directory. A later deployment previously trusted
that old result. Disk damage, an operator mistake, or a changed path could therefore be discovered
only when the canonical runner attempted to load a world.

## Decision

- Immediately after validating the protected production environment and before building or
  starting containers, deployment traverses the complete staged provisional composition with the
  release `civilization-data` validator.
- It separately checks the exact byte lengths and SHA-256 digests of both pinned DE441 segments.
- The staged root and every descendant must be free of symbolic links and group/world writes.
- Any failure stops deployment before Compose mutates service state. This check is read-only and
  does not replace the same validation performed at staging and canonical initialization.

## Consequences

The production runner never starts merely because a stale runtime directory exists. The cost is one
additional streaming verification pass over the launch inputs, paid only during deliberate
deployment rather than on every simulation tick.
