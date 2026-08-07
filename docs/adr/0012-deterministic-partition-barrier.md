# ADR 0012: Partition work resolves through one deterministic tick barrier

## Status

Accepted on 2026-08-06 as the ordering contract for the single-worker reference
kernel. Durable empty-schedule integration was accepted on 2026-08-07. This does not
yet authorize canonical full-Earth genesis in the production runner.

## Context

Full-Earth execution cannot depend on which worker finishes first, which partitions
happen to be loaded, or whether a process restarted. At the same time, introducing a
durable generic queue before embodied positions and real causal processes exist would
freeze placeholder concepts into state hashes and snapshots.

The smallest useful checkpoint is therefore a pure ordering and barrier kernel. It can
prove the semantics later workers must preserve without changing the published event
envelope, database schema, or current engine state.

## Decision

- `world-domain` owns one strict `S2CellId` value type. Its canonical JSON form is
  exactly sixteen lowercase hexadecimal characters. It validates the 64-bit S2
  sentinel structure and provides exact level and ancestor operations.
- Execution ownership is the configured planet-aggregate level, initially S2 L10.
  L14, L18, and L23 remain causal-detail tiers and always route to their L10 ancestor;
  they are not additional worker-ownership schemes.
- A plan is derived only for `current_tick + 1` from an immutable start-of-tick
  schedule. Overdue work is an error and no empty tick may be skipped.
- Only partitions with due work are materialized. They are visited in ascending numeric
  S2 CellId order; the scheduler never enumerates all Earth cells.
- Same-partition work uses the explicit numeric tuple
  `(phase_code, subject_key, process_code, occurrence)`. Rulesets own the numeric code
  registry. Enum declaration order, map iteration, debug strings, and worker arrival
  never define order. A work key must be unique across all partitions at one due tick.
- A worker emits immutable proposals identified by
  `(destination_partition, origin_work_key, emission_index)`. Emission indices for one
  work item are contiguous from zero. The barrier sorts by that tuple regardless of
  output arrival order.
- Every active source partition returns exactly one output, containing exactly one
  explicit result for every due work key. A work item that emits nothing still returns
  an empty result, so due work cannot disappear silently. Cross-partition effects
  resolve only at the barrier. Work produced by the barrier must be due after the tick
  being resolved, so no worker can recursively trigger same-tick work.
- The configured event limit is checked independently for every source-partition
  output before any result is accepted. One overflow rejects the complete tick plan;
  it never truncates events or mutates the borrowed pre-tick schedule.
- When integrated with embodied state, the coordinator appends `TickAdvanced` exactly
  once and evaluates extinction only after all lifecycle proposals for that tick,
  including births, resolve.

## Durable engine integration

- Schema-v1 scheduler checkpoints use a canonical, validated JSON envelope for the
  exact queue. The queue is now part of `EngineState`, snapshot schema v4, and state-
  hash schema v4 for partitioned configurations. Replay therefore verifies both the
  physical lives and their future causal work.
- Ruleset 1 admits no scheduled causal organism process yet. Its canonical queue is
  deliberately empty; adding a synthetic per-person heartbeat merely to populate the
  scheduler would violate [ADR 0011](0011-population-scale-and-capacity.md). A nonempty
  queue fails closed until energetics or another real causal process is admitted.
- A partitioned tick validates and advances the durable queue through the same barrier
  kernel. Transition event limits are enforced independently for each L10 partition;
  world-control events use a distinct global bucket.

## Deliberate non-decisions

- Its Rust module remains private to `sim-engine`; application and adapter crates cannot
  build canonical work until that embodied boundary is accepted.
- `location_id: Option<EntityId>` is not treated as an Earth position. The private
  fixed-point ECEF-to-S2 reference is now frozen by
  [ADR 0013](0013-fixed-point-ecef-s2-routing.md), but durable embodied position and
  movement still require an explicit schema decision before real genesis.
- No `DomainEvent`, `EventBatch`, PostgreSQL, or world-configuration field is added by
  this integration. The schedule is derived from configuration at genesis and carried
  by snapshots and state hashes.
- Whether production persists one atomic whole-tick batch or separately staged
  partition fragments remains open. A per-partition public commit protocol would need
  a new barrier/event schema and must be decided before canonical genesis.
- Phase and process code assignments are not invented in this checkpoint.

## Verification

The reference tests prove:

- input permutations produce the same plan and duplicate work is rejected;
- descendants route to the same configured ancestor on all six S2 faces;
- reversed worker-result arrival and multiple origins targeting one destination yield
  identical resolved events;
- same-tick generated work, missing/duplicate partition or work results, gaps in
  emission indices, and overdue work fail closed;
- exactly-at-budget succeeds while budget-plus-one leaves both the original schedule
  and causal state unchanged and reusable;
- empty ticks advance exactly once; and
- a synthetic set of durable people produces the same due-work set, ordered event
  bytes, individual state, and next schedule under dense scanning and queued partition
  execution on every tested tick. Workers derive proposals from immutable pre-tick
  state, and resolved events are applied only after the barrier accepts every result.

## Consequences

This freezes deterministic map/barrier ordering and makes its empty foundation
replayable without pretending that organism behavior is implemented. The library can
configure, tick, snapshot, and replay a full-Earth foundation, while the application
still exposes no canonical initializer. Exact source admission, the first real causal
process, horizontal-worker equivalence, and production persistence remain genesis
gates.
