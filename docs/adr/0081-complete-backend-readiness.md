# ADR 0081: Deployment readiness covers the complete backend service set

## Status

Accepted on 2026-08-08.

## Decision

Every required long-running backend process records a durable PostgreSQL heartbeat under a stable
service name. The simulation runner retains its existing heartbeat; the observer projector,
Hindsight delivery worker, and cognition worker now record their own instance-scoped heartbeats.
Worker metadata identifies mode and version but contains no provider credentials or canonical
cognition payload.

The public status response reports the latest timestamp for each service. Production deployment
does not declare success until the observer API and web process answer, Hindsight's own health
endpoint answers, and PostgreSQL has a heartbeat no older than sixty seconds for all four Rust
services. The check is bounded to sixty seconds and fails the deployment command visibly.

## Consequences

- An API-only deployment can no longer be mistaken for an operating civilization backend.
- A projector or cognition/memory worker that cannot complete its normal database loop withholds
  its heartbeat and prevents a false-green deployment.
- These are liveness signals, not proof that an external cognition provider will accept a future
  request; provider-path qualification remains a separate pre-genesis gate.
