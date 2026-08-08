# ADR 0114: Local cognition timeout precedes the simulation deadline

Date: 2026-08-08

Status: Accepted

## Context

Canonical candidate v4 showed that the prior 15-second cognition-worker timeout can expire while
the pinned CPU model is still prefilling a bounded prompt. The adapter correctly records that
outcome as unavailable, but an unnecessarily short circuit breaker prevents the local free route
from contributing even when ample deterministic response-window time remains.

## Decision

The default local/remote cognition request timeout is 45 seconds. The ruleset-26 response window
remains exactly 60 simulation ticks and the default runner target remains one tick per wall second.
Production preflight requires the configured wall timeout to be strictly shorter than the 60-tick
window at the configured target cadence. Invalid combinations fail before services start.

The timeout is an infrastructure circuit breaker, not canonical world state. Request selection,
the simulation deadline, route ordering, every route attempt, the prepared result or absence, and
the exact deadline latch remain durable and replayable. A late or timed-out response cannot replace
the immutable outcome. Paid routing remains disabled unless separately authorized.

## Consequences

The pinned Qwen CPU route has enough time to process the observed bounded prompt on this host while
still failing closed before the simulation deadline. Operators cannot silently shorten the tick
cadence or lengthen the provider timeout into a deadline race. Slower inference remains a recorded
unavailable input rather than delaying or rewriting canonical history.
