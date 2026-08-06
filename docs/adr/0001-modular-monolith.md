# ADR 0001: Begin as a PostgreSQL-backed modular monolith

- Status: accepted
- Date: 2026-08-06

## Context

The simulation needs strict domain boundaries and several independently runnable
roles, but starts on one server with a small population. Distributed infrastructure
would increase failure modes before it solves a measured problem.

## Decision

Use one Rust workspace with pure domain/engine crates and adapter crates. Deploy thin
API, runner, and migration processes built from that workspace. Use PostgreSQL for
events, snapshots, leases, outbox records, and read models.

## Consequences

- Domain boundaries are enforceable without network boundaries.
- Transactions can preserve event/cursor/outbox consistency.
- Individual processes can be split later without rewriting the core.
- PostgreSQL capacity and projection lag must be measured as population grows.
