# Roadmap

This roadmap separates facts that must exist from the first canonical tick from
interfaces that can be projected later. Phasing must not erase history or create a
back door into it.

## Tick-zero invariants

These are write-side commitments and must be active before the first durable world:

- an append-only, versioned event envelope scoped by world, tick, and total sequence;
- an explicit, unpreviewed world seed and pinned ruleset/configuration manifest;
- stable identity for every individually modeled person and animal, including real
  species identity, birth, death, lineage, and deterministic event ordering;
- a versioned identity tier for species or life stages modeled as cohorts;
- full-Earth coverage, conserved off-region state, causal refinement, and deterministic
  partition/event scheduling;
- durable individual representation for every person, with resource exhaustion able
  only to pause after a committed boundary;
- sourced real-world entities and an explicit assumption ledger;
- label-free perceptions and primitive actions rather than invention names;
- exact external cognition and memory inputs, including timeout or absence;
- a one-way dependency boundary from world events to observer projections;
- mechanical extinction and a one-time transition to an immutable archive;
- ruleset changes activated only by a recorded event under the live-world patch
  policy;
- state hashes at verification boundaries.

Any species offered for supporter naming must use durable individual identity before
that feature is enabled. Observer aliases never become world state.

## Rebuildable read-side features

These can phase in without losing earlier history, provided their primitive evidence
is present in the log:

- map and ecological projections;
- people, animal, place, lineage, and culture pages;
- observer wiki and artifact archive;
- deterministic first/record/streak finding aids and change maps;
- extinct-world browsing and comparison;
- follow feeds, accounts, supporter queues, and payment UI.

Every projection has its own version and durable cursor. It can be discarded and
rebuilt without changing a world.

## Checkpoints

### 0. Public foundation — complete

- Apache-2.0 public repository and project contract;
- Rust modular monolith, PostgreSQL migration foundation, API, and runner;
- public observatory shell and generated wiki/artifact/supporter surfaces;
- local Compose stack, keyless Hindsight profile, smoke checks, and CI.

### 1. Deterministic history proof — complete

- versioned world manifest, event envelope, deterministic event hashing, and state
  hashing;
- actual durable genesis, ticks, organism identity, birth/death/lineage primitives,
  extinction, archive, and explicitly authorized successor creation;
- full replay and snapshot-plus-tail replay with matching hashes;
- a downloadable verification bundle and a five-minute verification command;
- restart/resume and tamper-detection tests against PostgreSQL.

The offline bundle, restart-safe PostgreSQL runtime, snapshots, extinction transition,
authorized successor operation, and tamper detection are implemented and covered by
the full repository checks. Canonical world initialization remains intentionally
blocked on the full-Earth scientific bundle and partition scheduler; the available
initialization command is visibly non-production and requires an explicit world
identifier and seed.

### 2. Full Earth, reference tile, and embodied lives

- pin full present-geography Earth coverage, source/assumption/license ledgers, and
  content-addressed global layer roots;
- implement deterministic S2 partition scheduling, conserved causal refinement, and
  capacity stop/resume equivalence;
- complete Lower Buffalo–Ozark as a high-resolution conformance tile, not a world edge;
- implement space/time scale, energetics, mortality, perception, primitive action,
  communication, heredity, and versioned fauna identity tiers;
- demonstrate that agents receive properties and effects, never scientific labels.

Configuration schema v2 now defines full-Earth S2 addressing, WGS 84/EGM2008 physical
frames, four causal resolution tiers, deterministic event partitions, durable
individual people, and pause-at-committed-boundary capacity behavior. Scientific bundle
schema v2 requires content-addressed global climate, elevation, bathymetry, coastline,
habitat, hydrography, and soil roots plus an explicit counterfactual-baseline policy.
Legacy bounded schema-v1 bytes remain supported. The current engine deliberately blocks
full-Earth genesis until partition execution exists. No canonical seed has been
selected.

Canonical tile-index bytes and exhaustive offline tree traversal are now implemented.
The validator rejects missing or tampered leaves, cycles, repeated cells and paths,
false leaf counts, invalid S2 identities/parentage, and noncanonical indexes. A shared
strict S2 identity type and the partition scheduler's pure single-worker reference
kernel are also implemented. Its tests pin active-L10 ordering, deterministic barriers,
deferred cross-partition work, per-partition capacity rejection, empty ticks, and
synthetic dense-versus-queued equivalence without changing persistent event schemas.
A private fixed-point routing reference now maps bounded integer-millimetre EPSG:4978
positions through an explicitly geocentric, integer-only S2 bridge. Shared Rust/Python
goldens pin all faces, ties, exact boundaries, and causal-level ancestors without
changing events, snapshots, configuration, or PostgreSQL. A private conserved
L10-to-L14 reference now enumerates all 256 children, allocates generic sourced
extensive totals with exact order-independent integer arithmetic, and reaggregates them
without loss. It explicitly proves why a synthesis generation must be retained rather
than recalculated. It likewise changes no durable boundary. The next checkpoints are
measurement-oriented global source snapshots and normalized L10 roots, a coupled
ecological quantity policy with retained refinement state and deltas, and durable
embodied position/movement integration.

Two upstream snapshots are now committed independently of any world bundle. Natural
Earth v5.1.2 pins generalized global land polygons at an immutable revision; NOAA
ETOPO 2022 v1 pins a global 60 arc-second bedrock NetCDF release through an explicit
versioned-release locator. Both retain artifact URLs, exact lengths/hashes, version and
license evidence, and material limitations. The source CLI performs HTTPS-only
streaming acquisition with no replacement and complete offline verification. This
proves the acquisition chain but does not create a canonical coastline, relief layer,
or full-Earth root.

### 3. Situated memory and bounded cognition

- local deterministic working memory plus the asynchronous Hindsight adapter;
- one bank per life, durable retain outbox, recorded recall inputs, deadlines, and
  replay that never contacts Hindsight;
- simulated-time cognition allocation and recorded wall-cost circuit breaker;
- request an LLM key only for the first explicit model-backed integration test.

The project-owned retain/recall contracts, deterministic per-life bank and request
identities, atomic PostgreSQL delivery outbox, lease/retry worker, keyless Hindsight
HTTP adapter, no-op fallback, and recorded-replay adapter are implemented. Actual
perception memories and canonical recall-result events begin with embodied cognition;
the foundation does not invent placeholder experiences.

### 4. Evidence observatory

- project canonical maps, events, people, animals, lineages, and provenance;
- generated observer wiki and conditional artifact archive;
- versioned deterministic first/record/streak finding aids with replay tests;
- downloadable event ranges and visible state-hash verification.

### 5. Supporter participation

- isolated observer accounts, moderated reservations, eligibility queues, and aliases;
- human and enabled-animal birth matching only after a paid reservation is valid;
- idempotent Stripe webhooks and Apple Pay/Google Pay through Stripe;
- Apple and Google sign-in, refunds/transfers, and extinction handling.

### 6. Public operations

- Cloudflare Tunnel for the web origin only and Access for administrative routes;
- PostgreSQL WAL/base backups to offsite object storage with restore drills;
- service budgets, incident disclosure, metrics, and archive checksums.

## Vertical-slice proof

The first public vertical slice is done only when a fresh clone can:

1. start the stack and create a world from an explicit seed;
2. run durable ticks, stop, restart, and resume at the exact next sequence;
3. replay from genesis and from a snapshot plus tail to the same state hash;
4. detect a modified or missing event;
5. mechanically archive an extinct test world exactly once;
6. create a successor only through an explicit authorized command;
7. export a self-contained verification bundle and verify it without PostgreSQL;
8. show the corresponding map, timeline, organisms, and provenance through the
   observatory;
9. prove that observer, supporter, payment, and remote-memory availability cannot
   affect a canonical tick.
10. run the same local history through dense and partitioned scheduling to identical
    bytes, and stop/resume a capacity boundary without changing them.
