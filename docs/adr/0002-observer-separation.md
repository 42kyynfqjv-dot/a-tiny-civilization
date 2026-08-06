# ADR 0002: Observer knowledge is a one-way projection

- Status: accepted
- Date: 2026-08-06

## Context

The public wiki has access to objective history and retrospective classifications
that no simulated person should know. Accidentally reusing observer content in
cognition would introduce an omniscient narrator and predetermined concepts.

## Decision

Observer pages and summaries are derived projections in a separate application
boundary. Simulation cognition inputs may originate only from situated perceptions,
subjective memories, bodily state, relationships, and culturally transmitted claims.

## Consequences

- Observer projections can be rebuilt without affecting a world.
- Public explanations must label provenance and uncertainty.
- Application APIs and database roles should eventually enforce one-way access.
