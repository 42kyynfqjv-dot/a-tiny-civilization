# ADR 0121: qualification cannot race the first cognition deadline

## Status

Accepted on 2026-08-08.

## Context

The bounded qualification runner can advance simulation ticks much faster than the pinned CPU model
can prepare a result. That behavior is correct: infrastructure cannot hold a canonical tick or move
the fixed response deadline. It is nevertheless an operational trap when the qualification gate
explicitly requires one exercised local-model receipt. A single accelerated invocation can cross
tick 60 before the worker finishes and produce only immutable local fallbacks.

## Decision

- `advance-qualification-world.sh` remains the exact general bounded primitive and never waits for
  infrastructure.
- `advance-cognition-qualified-world.sh` is the reproducible pre-genesis orchestration wrapper. It
  requires an initialized world at exactly tick 0 / sequence 1 and already-running memory and
  cognition workers.
- The wrapper advances exactly one tick, leaving the tick-zero request's deadline in the future,
  then polls only the operational cognition tables without advancing simulation time.
- It proceeds only after exactly one durable model receipt exists while the world remains at tick 1.
  A bounded wall-clock timeout fails the attempt; it never shifts the canonical deadline.
- After readiness, it invokes the unchanged bounded primitive for the exact remaining tick count.
  Database credentials are converted to libpq environment fields and are not exposed in `psql`
  process arguments.
- The wrapper refuses an absent world, a changed cursor, malformed inputs, or a database URL with
  unsupported parameters.

## Consequences

Future mechanical candidates no longer depend on an operator remembering a manual pause, while the
simulation retains its strict infrastructure-independent clock. Provider or Hindsight failure still
cannot block a real world; it blocks only this deliberately stronger pre-genesis evidence workflow.

## Verification

Candidate v9 demonstrated both sides of the boundary. A deliberately accelerated disposable run
crossed all deadlines and correctly retained only fallbacks, so it was not promoted. The passing
run advanced one tick, durably prepared one zero-cost loopback Qwen result, then completed through
tick 1,680 with exact replay and consumption at the fixed deadline. The wrapper also fail-closed
against that already-advanced database at cursor 1,680 / 1,709.
