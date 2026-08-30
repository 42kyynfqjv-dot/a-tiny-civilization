# ADR 0179: Ordinary cognition quarantines the CPU timeout

Date: 2026-08-30

Status: Accepted

## Context

Ordinary cognition route policy three kept the local `qwen2.5:1.5b` route active
because it had once returned a usable receipt. Production telemetry now shows one
historical success followed by 85 consecutive unavailable outcomes. Each recent
attempt consumed the complete 180-second circuit breaker. The model is not a
requested production dependency and the repeated CPU work delays every ordinary
cognition result without contributing an action.

Removing or reordering the route would break the durable route-index contract, and
changing policy three would make its recorded canonical hash unreconstructible.

## Decision

- Ordinary route policy version four retains the exact ordered route list and adds
  `local_openai/qwen2.5:1.5b` to its canonical quarantine.
- The worker records that route as `skipped_disabled` without dispatching it. The
  existing dynamic OpenRouter route and every later configured fallback keep their
  original indices.
- Version-three reconstruction retains its original three-route quarantine; version
  two retains its empty quarantine. Validation fixes the exact quarantine for each
  supported policy version.
- Cancer research, Hindsight, simulation rules, cognition deadlines, and replay are
  unchanged. This policy change does not alter any already-recorded receipt.

## Consequences

New ordinary cognition jobs no longer spend three minutes on a route with 85
consecutive timeouts. Historical policies and their registry hashes remain
reconstructible, and the route can only return in a future explicit policy version
after a bounded health probe demonstrates that it is useful again.
