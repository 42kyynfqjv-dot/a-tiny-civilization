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

## Release-profile capacity evidence

The pre-genesis partition-kernel sweep is now published in
[the 2026-08-08 release report](PARTITION_CAPACITY_SWEEP_2026-08-08.md). It exercises 66 through
66,000 durable subjects at 1%, 10%, and 100% active fractions, records exact event bytes and stable
event/schedule digests, and measures release-build wall throughput. Separate deterministic tests
prove dense/queued equivalence, rejected-budget atomicity, and identical history under checkpoint
restart, recomputed retry, reassignment, and changed worker-result arrival order.

The fresh v19 integrated ruleset-30 qualification supplies the real-world-path complement:
PostgreSQL storage, replay, sparse snapshots, five current projections, 4,000 Hindsight deliveries,
and local cognition for the actual 66-founder candidate. The synthetic sweep is not extrapolated
into a full-stack population promise. A later envelope should add end-to-end PostgreSQL sweeps and
the first measured clean capacity stop before population growth approaches this host's observed
limits; that is an operational scaling milestone, not a blocker for the bounded initial genesis.

## Corrected ruleset-18 qualification

A later release-build run on the same host exercised the actual composition-0.1.1 closure and
ruleset 18 after ADR 0085 removed minute-scale life-history placeholders:

- 66 founders (two people and 64 real-species fauna), with zero post-genesis births through tick
  2,347 (8.15 simulation days);
- 2,388 committed batches and 998,085 canonical events;
- 39 sparse snapshots, with the newest anchor at tick 2,328;
- 2,812 Hindsight memory deliveries completed with zero recorded delivery errors;
- 40 cognition requests, one locally prepared replay-safe recall/result, and 39 deadline-latch
  consumptions; no private cognition payload was sent to an external provider;
- observer projections current through sequence 2,388 with 66 organisms, 67 safe timeline items,
  and six deterministic finding aids;
- 90,821,939 bytes of PostgreSQL JSONB event payload storage and a 124,032,691-byte disposable
  database at the measured boundary; observer telemetry counted 350,115,477 canonical payload
  bytes before PostgreSQL representation/compression;
- exact genesis replay, snapshot-plus-tail resume, continuous-run restart, and the bounded
  ten-tick qualification command all reached their committed state hashes.

This is still a small-population qualification, not a scale promise. It does establish that the
current real-input runner, memory boundary, cognition deadline, projection, and replay paths work
together before provider export or public genesis.

Provider connectivity was subsequently qualified without changing this world: the fixed synthetic
OpenRouter free-route probe returned a schema-valid result from
`google/gemma-4-26b-a4b-it:free` with 412 prompt tokens, 8 completion tokens, and zero billed
micro-USD. It read neither PostgreSQL nor Hindsight and is therefore protocol evidence, not a
live-cognition export test.

The consolidated ADR-0089 qualification report subsequently replayed the same history and passed
all checks in one read-only run: contiguous history, snapshots, four current projections, complete
memory delivery, all due cognition latches and consumptions, an exercised Hindsight recall/result,
and nonempty observer content. The retained JSON schema reports future cognition work separately so
an unelapsed simulation-time deadline cannot create a false failure.

ADR 0090 packaged that passing report with all seven seed-derived genesis JSON documents, their
original nested manifest, and source commit `2c5e752d5e204119dcd68cbd3d6549ecb7d083bf`. The disposable
bundle occupied 651,468 filesystem bytes; its root `SHA256SUMS` digest was
`4c1f9d36a73c3b623dea4fd19980c5eadb057add797a6c489905520874ca32f5`. It contained no canonical
event payloads.
