# ADR 0130: Live genesis requires an observed first tick

## Status

Accepted on 2026-08-08.

## Context

Canonical activation atomically and replayably commits tick zero. Service health can also pass
before any world exists. Neither fact proves that the long-running production runner discovered the
new world, loaded its exact inputs, committed a natural transition, delivered initial subjective
memories, or produced current public projections.

## Decision

After activation, a read-only live-genesis verifier waits at most five minutes for the exclusive
canonical world to remain running beyond tick zero, with no pending or errored Hindsight deliveries.
At that exact committed sequence it requires all observer smoke/privacy checks to pass. It then runs
independent genesis replay plus snapshot-tail verification inside the runner container and the full
backend status check.

The verifier cannot initialize or advance a world, build or start containers, restart a service, or
deploy the site. A timeout reports the last observed cursor and memory state.

## Consequences

Tick-zero activation and healthy live genesis are now distinct, evidenced operations. A startup
failure leaves the immutable tick-zero world available for ordinary runner restart and exact resume;
operators never reinitialize or reroll it to recover.
