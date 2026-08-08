# Provisional capacity report — 2026-08-08

This is an honest development measurement, not a promise for the public world. It
records the first long ruleset-17 full-Earth integration history before scientific
admission and before release-build population sweeps.

## Environment

- Linux 5.10, eight single-thread AMD EPYC (Rome) CPU cores, 15 GiB RAM, no swap;
- Rust 1.97.1 development profile with debug information;
- PostgreSQL 17 in a local Docker container;
- one runner process and external DE441 ephemeris evaluation;
- five-minute simulation ticks and a ruleset-17 provisional full-Earth configuration.

## History measured

- 66 founders: two people and 64 individually modeled real-species fauna;
- 68 organisms after two births, with no deaths;
- 1,184 ticks and 1,205 committed batches;
- 509,563 canonical events, including 9,394 shared-reservoir transfers;
- 2,835 accepted Hindsight memory-outbox deliveries and 20 cognition jobs;
- 100% of the current small population had scheduled work on each tick;
- full replay from genesis and snapshot-plus-tail resume produced the committed state
  hash at the measured boundary.

## Throughput and storage

- observed debug runner throughput was approximately 1.5–1.7 ticks per wall second;
- event production averaged about 430 events per tick and 6.32 events per active
  organism-tick;
- event batches occupied 48,963,584 bytes, approximately 11.9 MB per simulated day at
  this population and activity level;
- the database occupied 443,283,123 bytes. Of that, 380,190,720 bytes were legacy
  every-transition snapshots created before sparse retention was implemented;
- new snapshot retention keeps genesis, every 64th sequence, and terminal boundaries,
  while still validating every in-memory transition. The measured legacy snapshot
  total must not be extrapolated to new worlds;
- resuming the long history from an anchored sparse snapshot took roughly 0.5 seconds;
- a clean rebuild of all public projections took 82.945 seconds. The three projection
  cursors reached sequence 1,205 and deterministic row digests matched the reference
  single-batch rebuild exactly.

At the intended initial pace of one tick per wall second, this debug measurement has
headroom for the measured 68-organism workload. It does not establish a production
population ceiling: behavior, ecology, communication, cognition frequency, database
latency, and the active fraction will all change that boundary.

## Scale boundary

Twenty billion detailed individuals are not supported by this implementation or this
host. The current embodied driver schedules every organism every tick. A direct
scheduled-subject lookup was changed from a whole-population scan per subject to a
stable identifier lookup in this checkpoint, removing the immediate quadratic lookup
path, but transition work and event volume still grow with the active population.

ADR 0011's guarantee is narrower and durable: capacity pressure may slow or pause the
world only at a committed boundary; it may not merge people, drop events, change
fertility, reduce cognition, or otherwise rewrite the experiment. Scaling beyond one
host still requires measured release-build sweeps, inactivity-aware next-due
scheduling, horizontally assigned S2 partitions with canonical barriers, projection
read replicas or equivalent isolation, and storage lifecycle work.

## Next capacity evidence

Before public genesis, run reproducible release-profile sweeps at increasing founder
counts and active fractions, recording tick latency percentiles, event bytes, database
growth, memory and cognition rates, projection lag, restart time, and the first clean
capacity stop. The resumed history must match an uninterrupted reference byte for byte.
