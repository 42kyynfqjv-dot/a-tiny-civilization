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
- `world-data`: pure scientific release schemas, provenance validation, canonical
  bytes, and configuration binding; no network or async runtime.
- `world-data-filesystem`: exhaustive offline source/tile-tree traversal and safe local
  artifact resolution; no simulation, database, network, or async dependency.
- `sim-engine`: pure deterministic state transitions; no database, wall clock,
  network, ambient randomness, or unordered iteration. Its partition reference kernel
  plans one next tick from immutable input and resolves worker proposals at a sorted,
  all-or-nothing barrier.
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

The first projector is `civilization-projector`: it derives a bounded public timeline
from committed batches, atomically advances a versioned projection cursor, and stores
append-only observer rows. It withholds reproductive, heritable-disposition,
mortality-mechanism, parentage, location, and internal-identity detail from public
copy. The observer API only reads that projection. See
[ADR 0018](adr/0018-public-timeline-projection.md).

The same process independently maintains `public-organism-v1`: immutable organism
introduction and ending facts join into safe people/animal records with sourced species
citations and event provenance. It omits reproductive category, parentage, location,
mortality mechanism, inherited weights, and supporter aliases. See
[ADR 0019](adr/0019-public-organism-index.md).

The API also exposes a bounded read-only world index so the public web client can
select a current world without receiving a simulation write capability. Each public
cursor includes its manifest hash, event-chain head, and state hash so the observatory
can visibly identify the exact committed history it displays. It polls this index, then
the safe timeline and organism projections; unavailable or empty history is shown as
such rather than replaced with invented content.

`public-finding-v1` independently projects auditable first occurrences and population
records. Its `streak` vocabulary intentionally has no output until canonical behavior
events can prove persistence rather than merely suggest it. See
[ADR 0020](adr/0020-deterministic-observer-findings.md).

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

External cognition and Hindsight recall are selected at deterministic ticks and may be
accepted only by a deterministic deadline tick. Their exact result, validation status,
or absence is recorded as an input event. Replay never invokes a remote model or
memory service.

Wall-clock throughput changes how quickly observers receive ticks, not which state
transitions occur. The primary cognition allocation is denominated in simulated time;
a separate hard currency circuit breaker can force a recorded unavailable result.

## Irreversible facts and rebuildable views

The event log preserves facts that later projections cannot reconstruct: durable
organism identity, real species identity, birth/death/lineage, situated perceptions,
communications, external inputs, ruleset activation, and the mechanical extinction
transition. These are tick-zero commitments.

Wiki pages, biographies, maps, digests, archive navigation, and supporter dashboards
are rebuildable read models. They may ship later and project the complete retained
history. A deterministic finding aid may rank generic firsts, records, and streaks,
but significance never controls causal recording.

Supporter reservations and observer aliases live outside the simulation dependency
graph. A committed birth can cause an observer-side matcher to attach an approved
alias; the runner cannot wait for or query accounts, payments, reservations, or names.

## Scientific data boundary

Catalog records identify actual Earth materials and species using stable external
identifiers where available. Raw sourced facts, citations, units, ranges, uncertainty,
and licenses remain separate from ruleset-specific normalized parameters. The engine
may approximate a process at an appropriate scale, but every approximation is explicit
and testable; it must not silently substitute an invented material, animal, or ecology.

A configured public world commits its causal time scale, full-Earth S2 address
hierarchy, WGS 84/EGM2008 physical frames, deterministic refinement policy,
partition scheduler semantics, and the SHA-256 digest of its complete normalized
scientific bundle at tick zero. The application verifies locally archived bundle bytes
before genesis; live execution and
replay never fetch scientific inputs from the network. See
[ADR 0009](adr/0009-tick-zero-world-configuration.md),
[ADR 0010](adr/0010-full-earth-causal-refinement.md),
[ADR 0014](adr/0014-conserved-ecology-refinement.md), and the
[full-Earth scientific data plan](science/FULL_EARTH.md). The
[Lower Buffalo reference tile](science/FIRST_BIOME.md) exercises that global contract.
The [bundle release contract](science/DATA_BUNDLES.md) rejects incomplete provenance,
floating-point parameters, dangling assumptions, mismatched coverage, and noncanonical
bytes before configured genesis.

Pre-normalization evidence uses a separate canonical source-snapshot contract. Exact
upstream bytes are streamed into an ignored local cache, length- and SHA-256-verified,
and never overwritten in place. A snapshot proves what was acquired; it is not a
normalized bundle and cannot satisfy genesis. Snapshots now pin public-domain Natural
Earth generalized land polygons and CC0 NOAA ETOPO 2022 v1 global bedrock relief, with
their cartographic and measurement limitations kept inside hashed manifests. See
[ADR 0015](adr/0015-exact-upstream-source-snapshots.md).

The entire planet has canonical coarse ecological state. Regional, landscape, and
embodied detail refines only when causal activity approaches it. Refinement conserves
parent totals, is stream-seeded and order-independent, and cannot be triggered by an
observer request. Fine simulation geometry is always marked as measured, transformed,
or inferred; resolution is never presented as evidence quality.

The private initial-refinement reference allocates one sourced extensive scalar from
one L10 parent to all 256 L14 descendants using checked integer Hamilton
apportionment. It commits the exact normalized world-data bundle digest and all stream
dimensions, then validates exact canonical reaggregation. Its output is a one-time
synthesis that must eventually be retained: Hamilton allocation is not population
monotone, and independently allocated scalars do not yet enforce coupled ecological
constraints. No refined value is stored in an event, snapshot, engine state, or
database record. See [ADR 0014](adr/0014-conserved-ecology-refinement.md).

The private embodied-address reference uses signed integer-millimetre EPSG:4978
coordinates and routes their geocentric ray through the exact quadratic S2 projection.
It uses rational boundary comparisons and the standard Hilbert order, so platform math
libraries cannot select a different partition. This is a proof boundary only: no
position is yet stored in an event, snapshot, or engine state. See
[ADR 0013](adr/0013-fixed-point-ecef-s2-routing.md).

## Deployment boundary

Only the web origin is publicly reachable. It proxies `/api` and live observer
streams to the API over a private container network. Runner, migration service,
Hindsight, and PostgreSQL remain private. A disabled Cloudflare Tunnel profile will
be included before production configuration is required.

## Scaling posture

Do not introduce Redis, Kafka, Kubernetes, or independent microservices before
measurement demonstrates a need. The single-process engine remains the deterministic
reference implementation. Its pure L10 ordering/barrier kernel, synthetic
dense-equivalence proof, exact ECEF-to-S2 address router, exact conserved L10-to-L14
scalar-refinement proof, durable embodied positions, and a ruleset-two per-organism
body-clock partition schedule are implemented. The engine-level foundation replays.
An ADR 0049 experimental genesis is gated on real entity identity, cited and
content-addressed provisional inputs, an assumption ledger, coupled ecological state,
retained refinements and deltas, horizontal-worker equivalence, and the persistence
barrier—not exhaustive scientific admission. Every person remains an individual;
infrastructure pressure may slow or pause wall-clock advancement at a committed hash
boundary but cannot change causal rules or discard lives. See
[ADR 0011](adr/0011-population-scale-and-capacity.md) and
[ADR 0012](adr/0012-deterministic-partition-barrier.md).

The first scaling metrics are active individuals, scheduled transitions, cross-cell
messages, event bytes per simulated day, replay duration, snapshot size, projection
lag, memory request rate, and cognition cost. Published capacity reports include the
hardware and active fraction; the project does not claim unlimited or twenty-billion-
person real-time throughput.
