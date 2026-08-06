# Architecture

## Shape

The system begins as a modular monolith deployed as a small number of processes:

```text
simulation runner ──append──> PostgreSQL event history
       │                            │
       │                            ├── snapshots
       │                            ├── projections
       │                            └── outbox
       │
       ├── subjective memory port ──> Hindsight adapter
       └── cognition request port ──> optional model worker

public browser ──> web origin ──> observer API ──read──> projections
                         ^
                         │
                 Cloudflare Tunnel
```

PostgreSQL is authoritative. Hindsight, LLM providers, search indexes, caches, and
live notification channels are replaceable adapters.

Hindsight 0.8.6 is the pinned initial subjective-memory service. It begins in
provider-`none` mode and therefore requires no LLM key. Durable local outbox records
decouple memory delivery from ticks; normal cognition falls back to local working
memory when recall is unavailable. See
[ADR 0003](adr/0003-hindsight-memory-boundary.md).

## Dependency direction

```text
world-domain <- sim-engine <- application
       ^                         ^
       └──── adapters ───────────┘
                  ^
             process binaries
```

- `world-domain`: durable identifiers, value objects, events, and lifecycle rules;
  no I/O or async runtime.
- `sim-engine`: pure deterministic state transitions; no database, wall clock,
  network, ambient randomness, or unordered iteration.
- `application`: use cases and ports for persistence, memory, cognition, clocks,
  and projections.
- adapters: PostgreSQL, Hindsight, HTTP, model providers, and later payments/auth.
- binaries: configuration and dependency wiring only.

## Durable event flow

A runner is the single writer for a world. One transaction:

1. verifies the expected world sequence and writer lease;
2. appends a versioned tick event batch;
3. advances the world cursor;
4. writes durable outbox entries;
5. optionally writes a snapshot at a defined boundary.

Projectors consume durable sequence numbers and update purpose-built observer read
models idempotently. PostgreSQL notifications may reduce latency but never replace
the durable cursor.

Snapshots are caches. A complete replay from durable events, or a snapshot plus its
tail, must produce the same state hash.

## Determinism boundary

The core transition has the conceptual form:

```text
plan_tick(world_state, tick_input) -> ordered domain events
apply(world_state, domain_event) -> new world_state
```

Simulation time and wall time use distinct types. Quantities that influence replay
prefer integers or fixed-point representations. Entity iteration is stably ordered.
Random streams derive from world seed, ruleset version, subsystem, tick, and entity.

Recorded external cognition is an input event during replay; replay never invokes a
remote model.

## Scientific data boundary

Catalog records identify actual Earth materials and species using stable external
identifiers where available. Raw sourced facts, citations, units, ranges, uncertainty,
and licenses remain separate from ruleset-specific normalized parameters. The engine
may approximate a process at an appropriate scale, but every approximation is explicit
and testable; it must not silently substitute an invented material, animal, or ecology.

## Deployment boundary

Only the web origin is publicly reachable. It proxies `/api` and live observer
streams to the API over a private container network. Runner, migration service,
Hindsight, and PostgreSQL remain private. A disabled Cloudflare Tunnel profile will
be included before production configuration is required.

## Scaling posture

Do not introduce Redis, Kafka, Kubernetes, or independent microservices before
measurement demonstrates a need. The first scaling metrics are event bytes per
simulated day, replay duration, snapshot size, projection lag, memory request rate,
and cognition cost.
