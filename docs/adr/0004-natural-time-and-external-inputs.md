# ADR 0004: Natural history uses canonical ticks and recorded external inputs

- Status: accepted
- Date: 2026-08-06

## Context

Running faster than real time is sometimes described as time compression. That phrase
can also imply skipping uneventful history or accelerating progress for observers.
Wall-clock model budgets and variable service latency can accidentally make the amount
of cognition per simulated year depend on infrastructure.

## Decision

- Simulation time is an ordered integer tick domain. Every accepted tick runs the same
  complete transition; no era, life interval, or causal step is skipped.
- A world may target faster-than-real-time execution, but never changes rate because
  observers are bored or a desired discovery has not occurred.
- Public history begins at genesis. No undisclosed prehistory or hand-selected seed is
  used to make launch more interesting.
- Target and actual delivery rates are disclosed. Ordinary infrastructure lag delays
  observers but does not become a fictional world event.
- Cognition and memory requests are selected deterministically, allocated per
  simulated time, and assigned an acceptance deadline tick.
- A validated result received by the deadline, or an explicit unavailable/late result,
  is committed as an input event. Late results are discarded from canonical history.
- A separate hard wall-currency circuit breaker can force the unavailable path; its
  effect is recorded.
- Observer summaries are deterministic projections. No observer LLM is required.

## Consequences

- Faster observation never means a shallower simulation.
- Replay is independent of provider latency, queue order, and billing-month length.
- Pauses and slow hosts change delivery time but not already defined tick semantics.
- External scheduling needs durable request identities and deadline tests.
