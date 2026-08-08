# ADR 0062: Public world telemetry is an incremental disposable projection

## Status

Accepted on 2026-08-08.

## Context

Observers need to know whether a world is actually advancing and whether its public
read models are current. Recomputing total event volume by scanning JSON history on
every HTTP request would turn a useful transparency endpoint into database read
amplification and a denial-of-service surface.

## Decision

The projector maintains `public-world-telemetry-v1` with its own durable cursor and an
atomic, checksum-verified range consumer. It incrementally records canonical event
count and deterministic compact-JSON batch bytes. The public API combines those
counters with the world cursor, the latest commit time, all public projection cursors,
and projection-derived living person/fauna counts.

`GET /api/v1/worlds/{world_id}/telemetry` returns the counter's own through-sequence
and lag alongside every other lag. Totals therefore describe an explicit committed
prefix rather than pretending to be current during a rebuild. Missing or discarded
telemetry starts at sequence zero and is rebuilt from immutable history.

Wall-clock commit time, projection lag, and storage counters are observer operations
only. The runner cannot import or read them, and they never affect canonical work.

## Consequences

- Public requests are bounded reads independent of history length.
- A new telemetry projection version can rebuild without changing the world.
- `canonical_payload_bytes` measures deterministic compact JSON for event batches; it
  is not PostgreSQL on-disk size and is labelled accordingly.
- Capacity pages can show measured progress and lag without an observer LLM.
