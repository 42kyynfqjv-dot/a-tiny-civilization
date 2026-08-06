# ADR 0011: Population scale may slow or pause history, never alter it

## Status

Accepted on 2026-08-06 as a canonical-genesis gate. The pure single-worker ordering and
barrier kernel is implemented under [ADR 0012](0012-deterministic-partition-barrier.md),
but durable scheduled state and embodied integration are not. The current engine still
rejects schema-v2 full-Earth genesis instead of overstating that partial foundation.

## Context

The world has no authored population target. A successful civilization could grow far
beyond the population that one server can update densely. Leaving this unspecified
would make infrastructure pressure an eventual back door for arbitrary fertility caps,
reduced cognition, discarded people, skipped events, or observer-driven simplification.

No implementation can honestly promise detailed simulation of twenty billion people
at a fixed wall-clock rate. The project can promise what resource pressure is and is
not allowed to change.

## Decision

- Every person is a durable individual for their entire life. A person is never merged
  into an anonymous cohort because they are distant, unfollowed, or numerous.
- Every supporter-eligible animal species must likewise use its declared individual
  identity tier before naming is enabled. Plants, microbes, insects, fish schools, and
  suitable fauna or life stages may use documented scientific cohorts.
- The canonical scheduler is a deterministic event queue. An unchanged, sleeping, or
  otherwise inactive person need not execute an artificial full brain step every global
  tick; their next due causal transition is part of state and is reproducible.
- Work is partitioned by stable S2 cells. Entity ordering, same-tick ordering,
  cross-partition messages, and barrier rules are versioned canonical semantics.
  Which host owns a partition and how many workers are running are operational facts
  only and cannot affect results.
- Event limits apply to one partition transition, not to the population of the whole
  Earth. Exceeding a limit fails before commit; it does not truncate a batch.
- Load may reduce the rate at which simulation time becomes visible. It may not change
  reproduction, mortality, perception, cognition allocation, ecology, event detail, or
  any random stream.
- When available capacity cannot safely execute the next boundary, advancement stops
  after the last fully committed and hash-verified transition. It resumes from exactly
  that boundary when capacity is restored. No pause event is inserted into agent time.
- There is no hidden biological population cap. The observatory publishes measured
  capacity envelopes, projection lag, event volume, and incidents. “Unlimited scale”
  is not a project claim.
- Append-only events remain authoritative. Snapshots, aggregate read models, and cold
  archive tiers may reduce replay and storage cost without deleting individual history.

## Required tests before canonical genesis

- One logical history produces identical ordered events and state hashes under one
  worker and multiple partition workers.
- Randomized worker delay, retry, reassignment, and process restart cannot change a
  cross-partition result.
- A capacity stop and resume matches an uninterrupted run byte-for-byte.
- Observer requests cannot activate canonical work or change partition scheduling.
- Event-queue execution matches dense reference execution over a bounded test history.
- Published load tests state the population, active fraction, event rate, storage
  growth, replay rate, hardware, and wall-clock throughput.

## Consequences

- Very large populations may make the public world advance slowly until more capacity
  is deployed. That is preferable to secretly changing the experiment.
- Horizontal partition execution becomes necessary before the real full-Earth world,
  even though the initial population will be small.
- Twenty billion detailed lives remain a major operational and financial problem, not
  something this ADR hand-waves away. The architecture preserves correctness and a
  resumable history; it does not promise affordable real-time throughput at that scale.
