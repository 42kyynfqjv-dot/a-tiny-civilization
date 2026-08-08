# ADR 0058: Runner resume uses sparse event-anchored snapshots

Status: accepted and implemented on 2026-08-08.

## Context

Persisting a full engine snapshot after every transition made a 1,183-tick quality
world consume 363 MB in `snapshots` versus 47 MB in canonical event batches. Strict
runner startup also replayed all 1,204 batches from genesis, taking minutes even for
this small population. Both costs grow without bound and would turn routine restarts
into availability incidents.

Snapshots remain replaceable acceleration caches. Event batches are the immutable
canonical history, and the public verifier must retain an independent genesis replay.

## Decision

PostgreSQL retains snapshots at sequence zero, genesis, every 64th committed sequence,
and every terminal transition. A transition still constructs and validates its exact
post-state snapshot in memory so atomic cursor/effect checks are unchanged; only cache
persistence becomes sparse.

The long-running runner resumes from the newest snapshot only after loading the event
batch at the same sequence and requiring equality of world ID, sequence, batch hash,
and post-state hash. It then replays the tail and compares the result with the durable
world cursor. At most 63 ordinary transitions sit after a periodic checkpoint.

`verify-world` and initialization retry checks continue to replay from genesis and
independently compare snapshot-plus-tail. The fast path is an operational resume path,
not a replacement for full historical verification.

## Consequences

- Steady-state snapshot storage falls by approximately 64 times.
- Restart work is bounded by one snapshot plus a short tail instead of world age.
- A corrupt or unrelated snapshot cannot silently become canonical because its
  immutable event anchor and durable cursor must both agree.
- The interval is a storage policy and may change without changing world rules or event
  schemas; old snapshots remain valid caches.
