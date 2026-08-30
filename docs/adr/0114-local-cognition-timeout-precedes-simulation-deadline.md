# ADR 0114: Local cognition timeout precedes the simulation deadline

Date: 2026-08-08

Status: Accepted; amended by ADR 0171 on 2026-08-30

## Context

Canonical candidate v4 showed that the prior 15-second cognition-worker timeout can expire while
the pinned CPU model is still prefilling a bounded prompt. The adapter correctly records that
outcome as unavailable, but an unnecessarily short circuit breaker prevents the local free route
from contributing even when ample deterministic response-window time remains.

## Decision

The original default local/remote cognition request timeout was 45 seconds. ADR 0171 raises it to
180 seconds after the public world moved to one tick per wall minute and live CPU prefill exceeded
the old circuit breaker. The response window remains exactly 60 simulation ticks. Production
preflight now requires one recall plus all sixteen permitted network attempts to fit strictly
inside that window and requires the claim lease to outlive the same worst-case bound. Invalid
combinations fail before services start.

The timeout is an infrastructure circuit breaker, not canonical world state. Request selection,
the simulation deadline, route ordering, every route attempt, the prepared result or absence, and
the exact deadline latch remain durable and replayable. A late or timed-out response cannot replace
the immutable outcome. Paid routing remains disabled unless separately authorized.

## Consequences

The pinned Qwen CPU route has enough time to process the observed bounded prompt on this host while
still failing closed before the simulation deadline. Operators cannot silently shorten the tick
cadence or lengthen the provider timeout into a deadline race. Slower inference remains a recorded
unavailable input rather than delaying or rewriting canonical history.
