# ADR 0082: Complete backend readiness is continuously monitored

## Status

Accepted on 2026-08-08.

## Decision

One checked-in `backend-status.sh` command defines the operational liveness gate used both after a
deployment and during normal operation. It validates the exact protected production environment,
checks the web, observer API, and Hindsight endpoints from their containers, and requires fresh
PostgreSQL heartbeats for the simulation runner, observer projector, memory worker, and cognition
worker. Heartbeat age is bounded to a configurable 15–300 seconds and defaults to 60 seconds.

The deployment helper waits up to sixty seconds on this command. A hardened systemd oneshot and
two-minute timer run the same check after deployment, leaving a failed unit for host alerting.

## Consequences

- The deploy-time definition of “ready” cannot drift from the ongoing monitor.
- A restarted or silently wedged asynchronous service becomes visible without exposing an admin
  endpoint publicly.
- Installing the timer and routing failed-unit notifications remain host operations, but their
  implementation is checked into the repository.
