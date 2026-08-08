# ADR 0111: Canonical initialization migrates an empty store first

Date: 2026-08-08

Status: Accepted

## Context

The canonical initializer correctly refuses a PostgreSQL store containing another world. On a
genuinely new database, however, that guard queried `worlds` before the migration service had
created it and failed with `relation "worlds" does not exist`. Requiring an undocumented manual
migration step made the first production genesis procedure internally incomplete.

## Decision

`initialize-provisional-world.sh` invokes the API binary's embedded, idempotent migration command
after validating the immutable genesis checksums and before the runner enforces store exclusivity.
The migration executable is overridable for packaging and tests, but must exist and be executable.
The runner remains unable to evolve schemas during ordinary service operation.

## Consequences

A fresh canonical database follows one complete command from checksummed artifacts through schema
creation, exclusivity enforcement, atomic genesis, and replay verification. Retrying the same
command is safe. A database containing another world is still refused after migrations are current.
