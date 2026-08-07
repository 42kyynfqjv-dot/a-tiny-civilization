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

## Delivery order: complete first, admit second

Development is breadth-first until a provisional full-Earth world runs end to end.
The order is:

1. retain and plumb a real global source for every required Earth domain;
2. wire real species identities, provisional ecology, animal behavior, and the
   source-backed sky and tide drivers;
3. compose the provisional bundle, execute the partitioned world, and expose it in
   the observatory;
4. finish deployment, backup, recovery, and public-operation plumbing;
5. perform the integrated scientific admission pass: independent rebuilds, source
   and unit review, uncertainty propagation, cross-layer coupling checks, ecological
   calibration, and publication of the assumption ledger.

Basic integrity, licensing, schema, range, and content-hash checks still gate every
ingest. They prevent corrupt or unusable inputs from contaminating later work, but
publication-grade scientific validation does not hold up the next domain. Until the
final admission pass succeeds, outputs are explicitly **provisional**, cannot become a
canonical public world, and must not be described as scientifically validated. This
sequencing makes validation test the complete coupled system instead of polishing
isolated layers that may change when they are integrated.

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
the full repository checks. The engine-level partition foundation now replays, but
canonical world initialization remains intentionally blocked on admitted full-Earth
inputs and source-backed causal effects; the available initialization
command is visibly non-production and requires an explicit world identifier and seed.

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
Legacy bounded schema-v1 bytes remain supported. The engine can now configure, tick,
snapshot, and replay the full-Earth foundation with a durable ruleset-two per-organism
body-clock schedule. That is infrastructure, not a launch authorization: no
source-backed organism process or
production canonical initializer exists, and no canonical seed has been selected.

Full-Earth organism state now has a schema-v4 durable S2 embodied-patch position and
conditional movement fact. Full-Earth initialization/birth requires the configured
L23 patch level; replay, snapshots, and state hashes enforce it while public
projections omit it. This is the location boundary, not yet locomotion physics or
partitioned causal execution.

Canonical tile-index bytes and exhaustive offline tree traversal are now implemented.
The validator rejects missing or tampered leaves, cycles, repeated cells and paths,
false leaf counts, invalid S2 identities/parentage, and noncanonical indexes. A shared
strict S2 identity type and the partition scheduler's single-worker reference kernel
are also implemented. Its empty queue is now durable snapshot and state-hash state.
Tests pin active-L10 ordering, deterministic barriers, deferred cross-partition work,
per-partition capacity rejection, empty ticks, engine-level full-Earth replay, and
synthetic dense-versus-queued equivalence without inventing an artificial organism
heartbeat.
A private fixed-point routing reference now maps bounded integer-millimetre EPSG:4978
positions through an explicitly geocentric, integer-only S2 bridge. Shared Rust/Python
goldens pin all faces, ties, exact boundaries, and causal-level ancestors without
changing events, snapshots, configuration, or PostgreSQL. A private conserved
L10-to-L14 reference now enumerates all 256 children, allocates generic sourced
extensive totals with exact order-independent integer arithmetic, and reaggregates them
without loss. It explicitly proves why a synthesis generation must be retained rather
than recalculated. Durable embodied patches, conditional movement, and a ruleset-two
body-clock schedule are now integrated. The next engine checkpoints are a coupled
ecological quantity policy with retained refinement state and deltas, physical
movement resolution, and source-backed causal effects.

A separate provisional-world composition schema now requires all seven Earth roles,
the celestial ephemeris, the real fauna catalog, and fauna trait evidence in one
content-addressed manifest. Its only status is
`provisional-not-scientifically-admitted`, it must publish outstanding coupled-
validation gaps, and its bytes cannot decode as a genesis-eligible world-data bundle.
This is the structural boundary that permits breadth-first integration before the
final scientific admission pass.

The first actual composition is now committed as
`data/provisional/full-earth-breadth-first-0.1.0.json`. Its ten ordered references
verify against local bytes and commit under SHA-256
`4187ceb79a1e19e9479a61a97a454399446c0808300d23c168f84bed5feea6b4`.
The manifest openly carries six coupled-validation gaps; completing this manifest does
not clear any of them.

Six upstream snapshots are now committed independently of any world bundle. They pin
Natural Earth v5.1.2 generalized land polygons, NOAA ETOPO 2022 v1 bedrock relief, two
CHELSA-BIOCLIM+ temperature products, the complete 1981–2010 ERA5 monthly normal-period
request, and the Copernicus C3S 2022 v2.1.1 global land-cover response. Each snapshot
retains exact artifact lengths/hashes, version and licence evidence, and material
limitations. Acquisition refuses replacement and complete offline validation streams
every retained byte. A source snapshot still cannot authorize genesis by itself.

The Natural Earth path has now generated and independently inspected its full global
L6→L10 Boolean `land-reference` release: 24,576 tiles, 6,291,456 target cells, and
root-index SHA-256 `d8ac669b89f2903987766a2f55763b415bd7234097307ff63fcb7771099580ac`.
It is a generalized land cross-check only; it does not make a canonical coastline,
scientific bundle, or startable full-Earth world.

The ETOPO path has independently generated its global L6→L10 four-point-quadrature
terrain release over the same 6,291,456 target cells, with root-index SHA-256
`0794832d533a81e0889779a78aa39d730a3b09a98edff37b57ef76f394504876`.
The CHELSA path has now generated and independently traversed its provisional global
L6→L10 monthly near-surface temperature-normal release over all 6,291,456 target
cells. Its 24,576 tiles commit under root-index SHA-256
`7ff41b785e85f6314689bd31fcddd6546608dc9bd23ed60456a9b4826671cd9e`;
the inspector reports 18,404 cells without land-source support and monthly source
ranges from -63.850 °C through 40.050 °C. It remains land-only CHELSA evidence, not a
globally complete admitted climate: ERA5 ocean, sea-surface, precipitation, wind, and
sea-ice composition plus integrated validation are still required.
The acquired Copernicus land-cover artifact contains one verified 2,351,763,989-byte
NetCDF member on its documented 64,800 × 129,600 grid, with LCCS class and all four
quality fields. Its portable inspector independently verifies the complete snapshot,
archive/member digests, ZIP CRC, exact axes, variable types, and pinned product
metadata. The exhaustive class/quality census covers all 8,398,080,000 cells across
all 2,048 native chunks and is retained under `data/source-inspections/` with a pinned
byte fingerprint. The mixed-class L6/L10 payload now conserves class proportions and
all four sampled quality signals, while ADR 0036 pins a 32 × 32 exact face-UV target
quadrature and integer source-area lookup. The bounded-cache, parallel-address,
resumable and atomically published global normalizer plus independent release inspector
are implemented. The complete observed-class release now contains 24,576 tiles and
6,291,456 target cells under independently traversed root SHA-256
`ca93fa8f3c6d2876bdb4e45f4a4229ddad3e34167e9652cd1cb019f00cc186cc`.
The inferred habitat/coastline evidence roots remain pending.

The public JRC Global Surface Water v1.5 10-degree release now has a deterministic,
network-free 1,512-artifact inventory and a bounded-parallel, no-replacement acquisition
path for global occurrence, 2024 seasonality, and transitions. All 504 occurrence tiles
are retained (18,639,906,940 bytes) and pass a complete structural GeoTIFF inventory:
40,000 × 40,000 cells per tile, 0.00025-degree pixels, LZW row strips, and horizontal
differencing. The portable reader decodes bounded source rows without depending on a
system GIS installation. The provisional occurrence-code release has now been
generated and independently traversed across 24,576 tiles and all 6,291,456 L10 cells
under root-index SHA-256
`82d77b6cdfa56109fee93560e60790890b1e276a11d22c067cce01b16024e02f`.
It preserves all source codes without interpretation, including 426,256 explicit
out-of-source cells and 4,054,698 code-255 cells. This is breadth-first water evidence,
not an admitted hydrography layer; exact snapshot publication, terrain coupling,
palette-code semantics, reconstruction policy, and integrated scientific validation
remain pending.

The breadth-first soil path now enumerates 27 official SoilGrids global overview
artifacts: nine physical/chemical properties for 0–5 cm topsoil at the 5th, 50th, and
95th prediction quantiles. The acquisition helper is resumable and refuses replacement.
It deliberately gets a real global uncertainty-bearing soil vector wired before the
final pass expands to all six native-depth products and validates scientific coupling.
All 27 retained rasters (3,619,187,287 bytes) now pass a portable BigTIFF pyramid and
bounded-chunk inspection; source-specific one-pixel footprint differences and the
signed no-data sentinel remain explicit rather than being silently harmonized. The
global 0–5 cm vector release has now been generated and independently traversed across
24,576 tiles and 6,291,456 L10 cells under root-index SHA-256
`4bda39813eb6a6faaf3b286ec3aeea0ad260108e38b626320a4dda878d91db2e`.
All nine properties retain their Q0.05/Q0.5/Q0.95 values independently; per-property
no-data totals remain explicit and differ where the native source footprints differ.
This is still provisional topsoil evidence, not a six-depth or scientifically admitted
soil state.

Fauna now has a commercially compatible source boundary and retained exact taxonomy:
the 971,465,842-byte frozen CC BY 4.0 GBIF Backbone supplies stable real taxon
identities. Its full Darwin Core archive inventory and `Taxon.tsv` schema are parsed by
the Rust data tool. A streaming derivation scanned all 7,746,724 taxon records and
emitted a 256,508,217-byte, duplicate-checked catalog of 1,822,234 accepted Animalia
species under SHA-256
`b0597d47bc616b8ed2c18e7ba625a460538e9bac4bbae920f3f016095b966fa0`.
Distribution will come from a separate
DOI-backed occurrence extract filtered to CC0/CC BY records. ADR 0037 prevents the
convenient noncommercial cloud snapshot, modern observation density, or missing traits
from silently becoming canonical animal truth. The first physiology/life-history and
feeding-ecology evidence set now retains twelve exact AnimalTraits, Amniote, and
EltonTraits artifacts (67,009,045 bytes) under inventory SHA-256
`b03ce7a3bf08188ba756e256f353f11b6f5d651b652e132a829a60bb844e0499`.
Its acquisition rechecks upstream MD5s, SHA-256s, schemas, complete row shapes, and
published record counts. Inferred EltonTraits values remain distinguishable from
species-level evidence. A canonical provisional manifest now pins the three-source
composition and structurally separates retained observations from assumptions; its
wire status cannot claim scientific admission. Taxonomic crosswalk and parameter
admission are still pending.

The heavens now have an account-free long-range source path, retained bytes, and a
causal contract. Both JPL DE441 ephemeris halves, their technical/orientation evidence,
checksum evidence, and the official NAIF usage rules are locally retained and
content-hashed (3,308,164,805 bytes total). A deterministic six-artifact inventory is
committed under SHA-256
`a253715e23e547d07f2e7be066a3fa437974b54f1c8a78f876f144ff8be22742`.
DE441
supplies actual Sun/Earth/Moon and planetary positions across roughly 30,000 years;
genesis pins the epoch, deterministic interpolation drives light and seasons, and
Sun/Moon equilibrium potential drives the first tide model. ADR 0038 forbids a
decorative calendar, wall-clock sky, agent-visible astronomy labels, or silent
extrapolation beyond source coverage. The portable Rust reader now verifies both
little-endian DAF/SPK directories, all 28 type-2 segments, NAIF target/center/frame
identities, epoch intervals, and one-based data ranges. Its bounded Chebyshev evaluator
already resolves actual barycentric Earth plus geocentric Sun and Moon vectors at an
exact integral TDB second. Fixed-scale tick conversion, exact millimetre vectors,
replay-safe Sun/Moon tide geometry, local radial-horizon illumination, and a reduced
inverse-square solar-distance forcing ratio are now implemented with checked integer
arithmetic. Ruleset three now executes the pinned DE441 evaluator at each simulation
tick, records its fixed-scale Sun/Moon state in the hash chain, and replays without
opening source files. It remains opt-in until its downstream physical effects are
implemented. Earth-orientation transforms and the coupled ocean response remain pending.

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
- a cursor-driven, append-only safe public timeline projection and read-only API are
  implemented; every item links to a committed event and omits explicit mechanism data;
- cursor-driven people/animal indexes now expose only sourced species, life timing, and
  event provenance; public records omit reproductive, parentage, location, mortality,
  and supporter-alias detail;
- generated observer wiki and conditional artifact archive;
- versioned deterministic first/record finding aids are implemented with committed-batch
  replay tests; streaks remain intentionally absent until behavior events can establish
  persistence without observer inference;
- downloadable event ranges and visible state-hash verification.

### 5. Supporter participation

- isolated observer accounts, moderated reservations, eligibility queues, and aliases;
- observer-only reservation persistence is implemented: verified payment enters
  moderation, approved reservations match only already committed births, and immutable
  payment/match history cannot affect canonical events or runner dependencies;
- human and enabled-animal birth matching only after a paid reservation is valid;
- idempotent Stripe webhooks and Apple Pay/Google Pay through Stripe;
- Apple and Google sign-in, refunds/transfers, and extinction handling.

### 6. Public operations

- Cloudflare Tunnel for the web origin only and Access for administrative routes;
- PostgreSQL WAL/base backups to offsite object storage with restore drills;
- service budgets, incident disclosure, metrics, and archive checksums.

The production path now pins Cloudflare Tunnel 2026.7.2 by multi-architecture image
digest and structurally limits it to the web-only edge network. PostgreSQL 17 has a
checksum-pinned WAL-G 3.0.8 image, continuous archived-WAL configuration, private
Cloudflare R2 destination contract, bucket-scoped credentials, and independent
libsodium client-side encryption. Checked-in wrappers take base backups and create a
fresh isolated restore project; the restore verifier replays a selected world from
genesis, checks snapshot-plus-tail equivalence, and compares event/state hashes to the
committed cursor. The complete local encrypted base-backup/WAL-recovery probe passed.
R2 bucket/token creation, persistent encryption-key escrow, the first real offsite
backup, and a recorded production restore drill remain owner-account handoffs.

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
