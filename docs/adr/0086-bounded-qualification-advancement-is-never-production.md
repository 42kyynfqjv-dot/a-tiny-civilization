# ADR 0086: Bounded qualification advancement is never production

## Status

Accepted on 2026-08-08.

## Decision

The runner exposes `advance-qualification --world-id UUID --ticks N` for exact, finite,
non-production evidence runs. It acquires the same database-wide canonical-writer lock as the live
runner, verifies snapshot-plus-tail state before advancing, uses the same cognition scheduling and
DE441-backed transition path, advances exactly the requested number of simulation ticks, and prints
the final committed cursor and hashes.

The command rejects zero ticks, more than one million ticks per invocation, a non-running world,
and any environment where `APP_ENV` is `production` (case-insensitive). It never changes wall-clock
pacing for the public world because it cannot run there.

## Context

The first real-input qualification used the continuous service loop. That proved live operation but
made an exact evidence boundary depend on process-signal timing and log backpressure. A bounded
command makes stop/resume, deadline, projection, replay, and capacity probes reproducible without
adding a production speed-control channel.

## Verification

Against the disposable ruleset-18 world, a ten-tick invocation advanced tick 2,337 to 2,347 and
sequence 2,377 to 2,388; the extra sequence was a deterministic cognition-scheduling boundary.
Replay matched the committed event and state hashes. Repeating the invocation with
`APP_ENV=production` failed before advancement and preserved sequence 2,388.

## Consequences

- Qualification scripts no longer need to interrupt a continuous runner near an approximate tick.
- The canonical code path is shared rather than reimplemented in a test-only simulator.
- CI and operators can record exact before/after boundaries while production retains only its fixed
  long-running pace.
