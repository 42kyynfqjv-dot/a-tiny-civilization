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
- bounded inherited dispositions that cannot contain learned knowledge, cultural
  concepts, observer labels, or model output;
- exact external cognition and memory inputs, including timeout or absence;
- a one-way dependency boundary from world events to observer projections;
- mechanical extinction and a one-time transition to an immutable archive;
- ruleset changes activated only by a recorded event under the live-world patch
  policy;
- state hashes at verification boundaries.

Any species offered for supporter naming must use durable individual identity before
that feature is enabled. Observer aliases never become world state.

The first candidate identity tier is `ranged-tetrapod-individuals-v1`: source-ranged
tetrapods with separately retained local-occurrence corroboration may become durable founders,
while smaller fauna remain ecological evidence
until cohort and life-stage mechanics exist. This is an identity policy, not a claim
of abundance, native status, habitat suitability, or scientific admission.

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

The ruleset-17 quality history now also exercises shared-load, range-atomic projection
rebuilds. Its first published development envelope and the remaining scale boundary are
recorded in [the 2026-08-08 capacity report](operations/CAPACITY_REPORT_2026-08-08.md).

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
final admission pass succeeds, outputs are explicitly **provisional** and must not be
described as scientifically validated. ADR 0049 permits a mechanically qualified
public **experimental** world with a published assumption ledger; improved scientific
inputs apply only to a successor world. This sequencing makes validation test the
complete coupled system instead of polishing isolated layers that may change when they
are integrated.

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
- bounded public commitment ranges for comparing manifest, batch-chain, and state-head hashes
  without publishing private canonical event payloads;
- restart/resume and tamper-detection tests against PostgreSQL.

The offline bundle, restart-safe PostgreSQL runtime, snapshots, extinction transition,
authorized successor operation, and tamper detection are implemented and covered by
the full repository checks. The complete provisional full-Earth input closure and its
source-backed ruleset-26 causal path are now admitted. Canonical preparation and
initialization wrappers accept only an offline-verified public seed resolution and
derive the world identifier without operator choice. The published beacon resolution is now
fixed, and the exact ruleset-26 candidate has passed isolated mechanical qualification; public
launch remains gated on scientific and observatory admission rather than seed resolution.

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
body-clock schedule. Subsequent rulesets add source-backed organisms, ecology,
materials, memory, learning, and neutral artifact traces. The production canonical
initializer now exists behind the public-seed verification boundary. The first
canonical seed commitment and its post-round resolution are public, immutable, and mechanically
qualified without rerolling.

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
body-clock schedule are now integrated. Ruleset four additionally executes a narrow,
deterministic body-to-perception-to-action path for newly initialized provisional
worlds; it is an integration checkpoint, not a scientific behavioural claim. The next
engine checkpoints are a coupled ecological quantity policy with retained refinement
state and deltas, physical movement resolution, and source-backed causal effects.

Rulesets five through ten now extend that integration path with source-bound local
temperature readings, resolved adjacent-patch movement, bounded persistent perception,
citable real-material instances, neutral grasp/release and carried-object movement,
same-patch physical signal propagation, and canonical bodily regulation. The bodily
regulator retains exact energy, hydration, fatigue, and thermal load accumulators,
derives label-free need pressure without per-tick rounding drift, and emits restrained
mechanical mortality and same-tick extinction when a fatal budget is exhausted. Every
ruleset-ten body atomically pins measured metabolic power and a species-matching
regulation profile. Engineering-assumption profiles may participate only in an
explicitly labelled experimental genesis under ADR 0049 and must remain visible in the
public assumption ledger.

Provisional origin-environment schema 2 now joins the exact canonical L10 cells from the pinned
ETOPO terrain, JRC surface-water occurrence-code, and nine-property SoilGrids topsoil releases in
addition to the existing Copernicus land-cover and CHELSA temperature cells. Each value retains its
global root and containing-tile digest; soil also retains ordered quantile-source provenance. The
join is deliberately noninterpretive and noncausal under ADR 0123: inhabitants receive no source
codes, soil-property names, habitat labels, or implied uses. Schema-1 archives remain byte-readable.
The next causal surface checkpoint must version and justify each physical mapping independently.

Configuration schema 6 now commits that local surface closure into tick-zero private inputs under
ADR 0124. It retains exact terrain millimetres, one JRC occurrence source code, and all nine ordered
SoilGrids source-quantile triples, requires complete non-missing soil values, and binds them to the
same L10 and embodied patches as environment and weather. Older configuration shapes cannot accept
the new field. No engine rule reads it yet; the next ruleset must define label-free physical effects
behind new event, snapshot, and state-hash versions.

The same-seed v14 genesis chain and disposable `canonical_candidate_v13` now prove this boundary
against the real canonical origin. All 147,466 provisional references were traversed before an
atomic schema-6 tick-zero commit; genesis replay, snapshot-plus-tail replay, and the committed state
hash agree. The exact evidence is recorded in
`docs/operations/CANONICAL_GENESIS_V14_2026-08-08.md`. Full-duration qualification remains attached
to the next mechanics candidate rather than treating this configuration-only proof as admission.

Ruleset 29 now makes the measured ETOPO relief range affect only private movement fatigue under ADR
0125. It emits no terrain or altitude reading and lets the existing action/outcome learner discover
the bodily consequence without an answer key. Event, snapshot, and state-hash schemas advance to
29; JRC and SoilGrids remain committed but uninterpreted. The one-kilometre proportional reference
is explicitly provisional and must be assessed during scientific validation rather than tuned in a
running world. The same-seed tick-zero PostgreSQL proof is recorded in
`docs/operations/CANONICAL_GENESIS_V15_2026-08-08.md`. That same candidate now passes the complete
1,000-tick replay, snapshot, Hindsight, local-model, projection, and mechanics qualification gate;
it remains undeployed pending the separate launch-quality review and operations activation.

Ruleset 30 adds the first causal SoilGrids use under ADR 0126. The exact source-bound median
coarse-fragment volume fraction adds a checked proportional factor to the already terrain-adjusted
private movement load. The canonical origin retains 30 source units per 1,000 complete-volume units;
inhabitants receive no soil/property/use label and can encounter only the bodily consequence. The
other eight SoilGrids properties and the JRC surface-water source code remain causally unread. The
fresh same-seed ruleset-30 candidate passed the full 1,000-tick replay, Hindsight, local-model,
projection, mechanics, and observer-smoke gate. Its immutable evidence supersedes, without
mutating, the ruleset-29 launch evidence. The exact genesis and qualification-manifest digests are
now bound to a machine-verifiable experimental quality-world admission covering seven required
dimensions. The exact-current-code v19 evidence supersedes the earlier ruleset-30 qualification
bundles and binds activation to source commit `3d823f8da34c9ac1bceb543febf7359ac4ad26e3`. Admission
schema 2 additionally freezes the exercised simulation, schema, canonical-data, and qualification
paths: any difference requires a fresh qualification before launch. The admission explicitly does
not claim scientific admission or authorize public deployment; production configuration,
activation, live verification, and deployment remain separate operations.

Ruleset eleven replaces the four-step integration cadence with a seeded, situated,
need-responsive baseline policy over the existing label-free action grammar. Candidate
selection uses only canonical world state, bodily pressure, exact local reachability,
and ordered identities; it never reads a material or species label, observer state,
wall time, or model output. The selected act and its physical effects remain ordinary
replayable events behind new event, snapshot, and state-hash schema boundaries. This
is a freeform baseline, not learned cognition: imitation and coupled ecology remain
subsequent causal checkpoints.

Ruleset twelve adds the first neutral ingestion effect without leaking a food or water
label into action selection. Material instances can retain exact mass plus ordered,
species-bound oral-transfer profiles; a matching held-object swallow records conserved
mass before applying exact energy and hydration recovery to that tick's bodily state.
Depletion, replay, snapshots, schema downgrades, causal action/effect ordering, and
observer privacy are enforced. The committed profiles in tests are explicit engineering
assumptions: toxicity, injury, and digestion detail remain open. Until scientific
admission, such a response profile may be used only by an explicitly labelled
experimental world under ADR 0049.

Ruleset thirteen records one bounded action/outcome association after every scheduled
bodily transition. The update uses only signed total-pressure change, applies no
action-to-need answer key, and biases future primitive-action weights without reducing
any action below a nonzero exploration floor. Event/action/body/value ordering,
standalone-action rejection, bounded state, replay, snapshots, schema downgrades, and
observer privacy are covered. This is broad action-kind reinforcement only;
target/property generalization, delayed credit, teaching, beliefs, and forgetting
remain unimplemented. Ruleset eighteen now supplies the first imitation substrate:
each organism can attend to at most one co-located organism's directly witnessed,
label-free primitive action per tick and retain a bounded tendency toward that action.
Stable patch grouping keeps event growth linear, replay/snapshot schemas isolate the
new state, and public projections discard it. It does not infer purpose, success,
words, inventions, or relationships.

Ruleset fourteen makes births a delayed world-caused outcome. Every participating body
pins a species-bound reproductive commitment; compatible mature organisms at the same
exact patch receive deterministic sim-time opportunities, and successful opportunities
create private pending development with stable identities and recovery clocks. Only an
exactly bound due development can produce a birth. Missing, fabricated, or reordered
development/birth events fail before commit; unavailable developing parents resolve
through a neutral private end event. Public projections omit all mechanism, category,
partner, parentage, and profile detail and retain only the existing restrained birth
outcome. There is no population cap. The fixture profile is an explicit engineering
assumption; learned courtship, caregiving, non-pair modes, litter size, and individual
genetic variation remain open. A canonical provisional body-profile plan now gives the
supported full-Earth initializer one exact, content-addressed source for every
founder's and selected fauna taxon's initial age, metabolism, regulation, and
reproductive commitment; later bodily rulesets fail closed when that plan or a taxon
entry is absent. Reproductive commitment schema two now uses exact taxon/category
maturity aggregates from the pinned Amniote Life-History profile set when retained,
labels them as literature approximations, and gives every missing category its own
explicit engineering-assumption fallback. These private values and their source-row
digests never become organism concepts or public reproductive detail. Body-profile
schema three also retains exact taxon-matched adult-body-mass aggregates with explicit
assumption fallbacks. Mass is source-addressed but deliberately noncausal until a later
ruleset specifies the allometry for movement, reserves, ingestion, and thermal physics.

Ruleset fifteen adds bounded individual variation over the same eleven use-neutral
primitive actions. Founder dispositions derive from the world seed, stable identity,
action kind, and canonical species-profile fingerprint. Each offspring selects one
parental weight per action and may receive tightly bounded novel variation; learned
values, perceptions, memories, categories, observer state, and model output are
excluded. Development commits the offspring disposition and birth copies it exactly
while starting all life-local learned and bodily state empty. Genesis, apply,
snapshots, and replay recompute every disposition; mixed profiles for the same species
fail closed. Public projections expose none of this private detail. Event schema
seventeen, snapshot/state-hash schema eighteen, and body-profile plan schema two
isolate the boundary while ruleset-fourteen/schema-one history remains readable.
These are explicitly engineering-assumption phenotypic priors for an ADR 0049
experimental world, not genes, personality, intelligence, or scientific admission.

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

The active ruleset-18 genesis path now pins its append-only successor,
`data/provisional/full-earth-breadth-first-0.1.1.json`, whose fourth world component is the
normalized, independently source-pinned fauna physiology v3 catalog. V3 replaces a retained
AnimalTraits body-mass artifact whose identifiers were numerically rather than lexicographically
ordered, and canonical preparation now writes an explicit per-world body-mass selection plan before
constructing body profiles. Multiple source observations remain intact and are never averaged. The
current Amniote input covers five of 32 selected fauna taxa, while every uncovered mass stays an
explicit engineering assumption. Preparation, initialization,
runner defaults, and root-owned runtime staging all select the same 0.1.1 bytes and artifact set;
the earlier 0.1.0 composition remains immutable evidence for histories that already reference it.
The 0.1.1 filesystem audit now traverses 147,466 unique artifacts (10,164,215,509 bytes), including
every leaf under its six unique global tile roots. Runtime staging derives and re-verifies that
closure plus the two pinned DE441 evaluator inputs; root-only staging is no longer accepted.

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
opening source files. It is the default for new provisional worlds; older worlds retain
their committed ruleset. Earth-orientation transforms and the coupled ocean response
remain pending.

The retained ERA5 normal-period archives now have an executable point-evidence boundary rather
than only a one-year schema probe. A canonical artifact binds the seed-selected origin to all 30
verified annual archives and preserves six complete 1981–2010 monthly series—air temperature,
precipitation, two wind components, sea-surface temperature, and sea-ice fraction—as 2,160 exact
source binary32 values. The committed origin resolves to ERA5 row 265 / column 1026. This is
noncausal evidence. A second canonical artifact now derives deterministic fixed-point monthly
minimums, means, maximums, and coverage counts without host floating-point arithmetic. It retains
the terrestrial cell's absent sea-surface and sea-ice values explicitly and records 30 observations
per month for air temperature, precipitation, and both wind components. Weather generation,
land/ocean composition, temporal downscaling, and cross-variable scientific admission remain
separate later mechanics.

### 3. Situated memory and bounded cognition

- local deterministic working memory plus the asynchronous Hindsight adapter;
- one bank per life, durable retain outbox, recorded recall inputs, deadlines, and
  replay that never contacts Hindsight;
- simulated-time cognition allocation and recorded wall-cost circuit breaker;
- keep every provider credential deployment-only and perform a real call only after
  the durable request/receipt/deadline boundary is ready.

The project-owned retain/recall contracts, deterministic per-life bank and request
identities, atomic PostgreSQL delivery outbox, lease/retry worker, keyless Hindsight
HTTP adapter, no-op fallback, and recorded-replay adapter are implemented. Actual
perception memories and canonical recall-result events begin with embodied cognition;
the foundation does not invent placeholder experiences.

The strict bounded-cognition contract, OpenAI-compatible adapter, and free-first route
ladder are now implemented under ADR 0051. The versioned registry has 256 route slots,
separates production allocations from trial/development endpoints, caps one job at
sixteen network attempts, records normalized attempt outcomes, and leaves the sole
approved paid DeepSeek V4 Flash route unreachable without per-job authorization.
Ruleset 16 canonically selects one world-total request from exact body-owned state,
commits its fixed simulated-time deadline, and carries it through event schema 18 and
snapshot/state-hash schema 19 under ADR 0052. Ruleset 17 preserves that boundary while
adding shared real-material reservoirs under ADR 0057, event schema 19, and
snapshot/state-hash schema 20. Hindsight results are normalized and
re-admitted only by exact comparison with accepted local memory deliveries. PostgreSQL
job insertion and exclusive request leases now commit against migration 0011 under
ADR 0053. The same migration establishes immutable recall/result/latch tables, a
database-enforced 16-call route prefix, and integer-micro-dollar accounts and paid
reservations. Stepwise attempt/result methods, deterministic deadline latching, and
canonical result consumption are now wired end to end. The cognition worker records
recall once, persists each dispatch before an HTTP call, recovers interrupted attempts
without duplicating them, and executes only configured routes. Ruleset 16 derives the
subject from canonical state; the runner freezes an exact result or explicit absence
at the 60-tick deadline and replay never calls Hindsight or a provider. A valid response
can only bias an already legal primitive action by a fixed amount. Late results remain
auditable and billable but cannot replace the latch. The local Compose profile starts
the worker with every provider optional and the approved paid route disabled by
default. Remaining work in this checkpoint is accelerated multi-world/load verification
and production operator admission for whichever credentials are actually enabled.

The first executable resource layer is implemented as a canonical provisional artifact
bound to the origin, fauna population, and body-profile digests. It gives every founder
species positive energy and hydration routes through cited D-glucose and water
identities while marking availability, renewal, and response values as engineering
assumptions. Atomic PostgreSQL genesis, same-tick shared withdrawal ordering, lazy
renewal, replay, private observer handling, and an accelerated replay-verified
tick-1,183 survival gate are covered. Local flora, hydrology, toxicity, and trophic
replacement remain the later
scientific-validation pass; a live world will never have its committed bridge edited.

Snapshot persistence and restart cost are now bounded under ADR 0058. Running worlds
retain genesis, terminal, and every 64th cache checkpoint. Runner startup anchors the
latest snapshot to its immutable event batch and replays at most the short tail, while
the operator verifier still performs the independent genesis replay. On the
tick-1,183 quality world this reduced measured startup verification from minutes to
about half a second; new-world steady-state snapshot storage is approximately 64 times
smaller than the former every-transition policy.

### 4. Evidence observatory

- project canonical maps, events, people, animals, lineages, and provenance;
- a cursor-driven, append-only safe public timeline projection and read-only API are
  implemented; every item links to a committed event and omits explicit mechanism data;
- cursor-driven people/animal indexes now expose only sourced species, life timing, and
  event provenance; public records omit reproductive, parentage, location, mortality,
  and supporter-alias detail;
- generated observer wiki and conditional artifact archive;
- versioned deterministic first/record finding aids are implemented with committed-batch
  replay tests; the v2 finding projection adds logarithmic human-presence streak milestones
  established directly by tick and life events, without inferring customs or intentions;
- downloadable event ranges and visible state-hash verification.

The public observatory review is now a source-bound machine gate rather than a prose sign-off.
Its schema-1 admission freezes the exact web and policy trees, required public routes, edge-security
tests, supporter isolation, provenance, and presentation review against the experimental quality
world. Both public-genesis preflight and deployment reject drift, while the admission is structurally
unable to authorize deployment.

### 5. Supporter participation

- isolated observer accounts, moderated reservations, eligibility queues, and aliases;
- observer-only reservation persistence is implemented: verified payment enters
  moderation, approved reservations match only already committed births, and immutable
  payment/match history cannot affect canonical events or runner dependencies;
- human and enabled-animal birth matching only after a paid reservation is valid;
- strict idempotent Stripe webhook verification and its transactional append-only event ledger are
  implemented; the server-side, fixed-Price, reservation-idempotent Checkout client is exposed only
  through an authenticated, CSRF-protected account route;
- provider-neutral accounts plus hashed, expiring, revocable browser sessions are implemented;
  account-bound reservation/Checkout orchestration is also implemented and retry-safe. Apple and
  Google now have strict code/token adapters, browser-bound single-use attempts, hardened cookies,
  and edge-proxy coverage. A versioned automatic screen rejects obvious abuse before payment and
  immutable human moderation still gates activation. A full-refund operator command now durably
  prepares terminal refunds, uses Stripe idempotency, and immutably records completion; transfers
  are prohibited by policy. An operator-only queue now reports paid labels oldest-first, fails a
  monitor check when review age exceeds its threshold, and stores immutable moderator decisions;
  rejection resumes the refund automatically. The public supporter panel now discovers disabled
  integrations without exposing broken controls, starts Google or Apple sign-in, creates person or
  individually recorded animal Checkout reservations, reads account history, and cancels unmatched
  reservations with CSRF protection; payment and OAuth remain disabled until operator configuration.
  Authenticated supporters can also cancel their own unmatched reservations through a
  CSRF-protected, retry-safe route; paid cancellations automatically use the durable full-refund
  path while unpaid cancellations never contact Stripe. A bounded private account-history route
  reports only the caller's reservation lifecycle and coarse refund state while withholding payment
  identifiers, internal account subjects, and moderation evidence.

### 6. Public operations

- Cloudflare Tunnel for the web origin only and Access for administrative routes;
- PostgreSQL WAL/base backups to offsite object storage with restore drills;
- service budgets, incident disclosure, metrics, and archive checksums.

The public incident response policy and append-only disclosure template now separate world events
from operational failures, require material notices within 24 hours, preserve committed-boundary
and privacy rules during response, and retain explicit history/data-impact and verification fields.
Repository checks reject removal of that contract.

Stripe-enabled production preflight now requires an attributable moderator identity, and a
checked-in systemd timer executes the stale paid-label queue check every fifteen minutes. Enabling
the timer and routing failed-unit alerts are owner operations; the monitor never approves labels.
Configured production OAuth and Stripe flows are now additionally pinned to the exact
`atinycivilization.com` HTTPS callbacks. Stripe configuration must be structurally live-mode, while
an entirely unconfigured account/payment surface remains valid for a read-only launch.

The production path now pins Cloudflare Tunnel 2026.7.2 by multi-architecture image
digest and structurally limits it to the web-only edge network. PostgreSQL 17 has a
checksum-pinned WAL-G 3.0.8 image, continuous archived-WAL configuration, private
Cloudflare R2 destination contract, bucket-scoped credentials, and independent
libsodium client-side encryption. Checked-in wrappers take base backups and create a
fresh isolated restore project; the restore verifier replays a selected world from
genesis, checks snapshot-plus-tail equivalence, and compares event/state hashes to the
committed cursor. The complete local encrypted base-backup/WAL-recovery probe passed.
R2 bucket/token creation, persistent encryption-key escrow, the first real offsite
backup, and a recorded production restore drill are implemented handoffs but are
explicitly deferred by the owner for the first genesis. Static production preflight
accepts that deferred state; backup and restore commands still fail closed unless the
complete encrypted offsite configuration is present.

The long-term host now passes the composed read-only public-genesis preflight with its protected
production configuration, exact v19 evidence, both admissions, and a freshly staged 147,469-file
composition-0.1.1 runtime closure. Obsolete generated staging, unused build cache, and unused
images were reclaimed without touching retained inputs or active volumes, restoring the disk
reserve above the backend health floor. ADR 0135 makes that complete composed gate a hard
precondition of the deployment helper rather than a runbook-only instruction.

The admitted ruleset-32 successor has now independently passed that same host gate with the v24
portable genesis proof, v20 tick-1,000 qualification evidence, replacement quality and observatory
admissions, and a fresh 147,469-file composition-0.1.2 runtime closure. The first run failed closed
only on the capacity reserve while both complete runtime stages were retained; reclaiming 26.1 GiB
of reproducible Cargo development output restored about 30 GiB free without removing either stage
or any world/evidence data. The repeated full preflight then passed without creating a world,
changing services, or deploying the site; the exact record is in
`docs/operations/HOST_PREFLIGHT_RULESET32_2026-08-09.md`.

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

The first real-input ruleset-18 qualification now covers initialization against all 147,466 pinned
provisional references, DE441-driven canonical advancement, stop/restart at the exact stored
cursor, genesis and snapshot-plus-tail replay, and complete observer projection. It also caught and
removed arbitrary tick-count life-history placeholders before any public genesis; ADR 0085 records
the corrected simulation-time guardrails and the 1,271-tick regression run. ADR 0086 adds an exact,
writer-locked `advance-qualification` command that refuses production; its first bounded proof
advanced precisely ten ticks and replayed at the resulting cursor.

Remote cognition now has a second independent activation boundary: provider credentials and cost
authorization are insufficient unless `COGNITION_EXTERNAL_EXPORT_APPROVED=true` is set. Runtime and
production-preflight tests enforce ADR 0087 before any private cognition payload can leave the host.

ADR 0088 adds a database-free, fixed synthetic OpenRouter probe. The dynamic free endpoint passed
the production schema and zero-cost checks without reading world or Hindsight data; live cognition
export remains separately gated.

Ruleset 19 now supplies the first use-neutral durable transformation substrate. Applying primitive
force to an object already held by the actor accumulates a bounded scalar surface trace, and the
changed trace is immediately available only through direct touch perception. Canonical history has
no artifact, tool, mark, symbol, writing, or intended-use category. A fifth append-only observer
projection may file traced real-material objects as artifacts with exact provenance; it has no
dependency path back to the runner. The next provisional genesis and fail-closed qualification now
default to ruleset 19, while existing ruleset-18 evidence remains immutable and independently
verifiable.

The first ruleset-19 qualification pass also proved that two survival reservoirs alone do not expose
the neutral material-handling path: 1,000 ticks produced no grasp or surface-trace event because
reservoir transfer bypasses held objects. Provisional material-resource schema 2 now adds one finite,
cited silicon-dioxide object with no oral response, affordance, or use label. ADR 0094 records the
negative qualification and requires a fresh corrected history rather than changing it in place.

ADR 0095 adds the first generated observer-wiki API over those projections. Its entries keep
physical-event provenance separate from observer interpretation and retain exact event/tick/source
citations. The corrected qualification world returned one silicon-dioxide altered-material entry
plus six factual findings. No writing, symbolism, or purpose is claimed before the world supplies
evidence for it.

Ruleset 20 adds the missing neutral arrangement substrate without adding writing. A force action
may address one of eight physical contact regions; exact region and aggregate traces are perceived
and replayed, while the engine assigns no pattern, symbol, or purpose. Its first isolated
qualification history reached tick 1,000 with 85 regional transitions spanning all eight regions,
all five observer projections current, and exact stop/resume/replay. ADR 0096 records the schema and
observer boundaries. At that checkpoint Hindsight delivery and cognition fallback were exercised
separately; the later ruleset-25 qualification closes the then-unmet model-receipt gate.

Ruleset 21 removes a separate emergence ceiling in communication: autonomous local sound now has
eight selectable physical intensities instead of one invariant amplitude. The values have no token,
word, message, or meaning, but direct source-bound sound memories and bounded cognition can
distinguish them. The next provisional world and qualification default to ruleset 21; ruleset-20
history remains byte-identical. ADR 0098 records the policy and model-contract boundary.

Ruleset 22 closes the first learning loop without supplying meaning. An organism may privately
associate a directly heard amplitude with the same source's next directly witnessed primitive
action, then weakly bias its response when that amplitude is heard again. Association events and
state never enter public projections, newborns inherit none of them, and the engine contains no
message or vocabulary. Fresh genesis and qualification now default to ruleset 22; ADR 0099 records
the temporal and privacy invariants.

Ruleset 23 makes immediate movement direction selectable instead of an engine-side draw. Four
label-free adjacent motor coordinates can be explored or weakly selected by cognition, while exact
action-to-relocation coupling rejects teleportation and direction tampering. No map, place name,
route, or destination enters the world. Fresh genesis and qualification default to ruleset 23;
ADR 0100 records the boundary and the corrected preservation of selected signal intensity.

Ruleset 24 closes that motor loop without supplying navigation. Each organism privately retains a
bounded bodily-outcome value for each of the four adjacent movement coordinates, and that experience
weakly adjusts only the matching future direction. Values are life-local, non-heritable, omitted
from every public projection, and replay-checked against the exact move and bodily transition. Fresh
genesis and qualification default to ruleset 24; ADR 0101 records the boundary.

Ruleset 25 lets a private sound association retain the exact movement motor coordinate directly
witnessed after that sound. Hearing the amplitude again can weakly bias only that direction, while
the engine still stores no word, message, destination, intention, or meaning. Legacy generic
associations remain replay-compatible; new associations are bounded, life-local, non-heritable, and
absent from public projections. Ruleset-25 histories retain that behavior; ADR 0102 records the
boundary.

Ruleset 26 reserves the scarce external-cognition budget for living people. Fauna retain the full
deterministic embodied policy, learning, signaling, reproduction, and public identity paths but can
never consume an LLM request. This fixes the canonical-candidate finding that the first successful
Qwen receipt went to a moth merely because fauna outnumbered people 32 to 1. Fresh genesis and
qualification default to ruleset 26; ruleset-25 history remains byte-identical. ADR 0108 records the
participation-tier boundary.

Ruleset-25 integrated qualification now continues through tick 1,381 with exact genesis and
snapshot-plus-tail replay, all five projections current, 3,672 error-free Hindsight deliveries, and
one zero-cost Qwen2.5 1.5B receipt prepared before its fixed deadline and consumed in canonical
history. The private CPU model runs behind an exact same-host URL allowlist and an unexposed Compose
service. Adapter schema v5 grammatically closes every action/coordinate pairing while retaining the
independent Rust receipt validator. The checksum-covered evidence bundle contains no canonical
event payloads or secrets.

The first publicly committed no-reroll seed is now resolved to world
`b3ea736d-7a5a-5161-a74b-fa8c4302d333`. Its exact 147,466-reference genesis closure was initialized
in an exclusive local candidate database and passed the same fail-closed ruleset-25 gate at tick
1,560, including complete Hindsight delivery and one latched, consumed, replayed local Qwen receipt.
The retained evidence bundle contains no canonical event payloads. This qualifies the mechanical
launch path; deployment remains closed for the scientific/assumption and public-observatory reviews.

Canonical fauna preparation now closes the false-positive range gap without an authored list or
reroll. It intersects the modeled-range pool with a hash-pinned, research-grade, non-captive,
commercially reusable iNaturalist observation query within 75 kilometres of the committed origin.
The retained evidence means reported local presence only—not abundance, native status, or habitat
suitability—and initialization independently rederives the exact intersection before genesis.

Canonical candidate v3 carries that intersection into a fresh ruleset-26 genesis and immutable
world manifest. It passed isolated qualification at tick 1,560 with exact replay, every observer
projection current, 3,937 error-free Hindsight deliveries, and one consumed local Qwen receipt.
Its checksum-covered evidence supersedes candidate v2 for launch review while leaving candidate v2
immutable. Deployment remains closed pending the scientific/assumption and observatory admission
reviews.

Canonical candidate v4 additionally derives category-specific maturity timing from the pinned
Amniote Life-History aggregates where exact taxon/category records exist. Missing values remain
independently addressable assumptions. The same-identity candidate passed at tick 1,680 with exact
replay, current projections, complete Hindsight delivery, and one fixed-deadline local Qwen receipt.
Its immutable evidence supersedes candidate v3 for launch review; this remains a provenance and
mechanical improvement rather than scientific admission or deployment approval.

Canonical candidate v5 adds a separate, immutable adult-body-mass selection plan and pins the
corrected physiology catalog v3 without changing the public seed, origin, or selected taxa. Five
fauna masses are exact source-addressed literature approximations; every uncovered value remains an
explicit assumption, and mass remains noncausal. The fresh candidate passed at tick 1,680 with
exact replay, 3,939 error-free Hindsight deliveries, current projections, and one pre-deadline
loopback Qwen receipt with paid dispatch disabled. Its immutable evidence supersedes candidate v4
for launch review but does not authorize deployment or scientific admission.

The next noncausal evidence boundary is now typed for fauna ecology. A canonical plan can retain
exact EltonTraits diet/activity source rows by stable taxon identity while being structurally unable
to create an agent drive, action, affordance, habitat decision, or food label. The committed-origin
source covers 23 of 32 selected fauna taxa and contributes 257 exact trait/row pairs. Canonical
preparation derives the plan, and initialization re-resolves every pair before pinning both plan and
profile-set digests in the immutable world manifest. Separately reviewed causal ecology remains a
later step.

Canonical candidate v6 carries that noncausal ecology plan into a fresh same-identity genesis. It
passed at tick 1,680 with exact genesis and snapshot-tail replay, all five projections current, all
3,939 Hindsight deliveries complete without errors, and one fixed-deadline local Qwen receipt
consumed with paid dispatch disabled. Its immutable evidence supersedes candidate v5 for launch
review. The evidence changes provenance coverage only and does not authorize deployment, causal
ecology, or scientific admission.

Canonical candidate v7 additionally carries the complete point-scoped ERA5 normal-period evidence
into the same-identity manifest. Preparation and initialization independently produced identical
2,160-value artifacts before the fresh candidate passed at tick 1,680 with exact replay, complete
Hindsight delivery, current projections, and one consumed local Qwen receipt. Its immutable
evidence supersedes candidate v6 for launch review. ERA5 remains noncausal source evidence and does
not authorize weather mechanics, scientific admission, or deployment.

Canonical candidate v8 additionally pins deterministic fixed-point monthly ERA5 summaries derived
from that exact evidence. Preparation and initialization independently reproduced both artifacts,
and the runner verified their digest relationship before manifest commitment. The fresh candidate
passed at tick 1,680 with exact replay, complete Hindsight delivery, current projections, and one
consumed local Qwen receipt. Its immutable evidence supersedes candidate v7 for launch review. The
normals remain noncausal and do not authorize weather mechanics, scientific admission, or
deployment.

Ruleset 27 makes only the defensible first weather dimension causal. Genesis configuration schema 5
binds the complete fixed-point ERA5 temperature, precipitation, and wind normal contract to the
committed origin, while the engine derives seeded daily temperature anchors and integer
interpolation from simulation time alone. Bodily regulation and direct physical perception now use
that changing temperature without exposing weather, month, season, or survival labels. Wind and
precipitation remain pinned but noncausal until their temporal distributions and covariance are
defined. Fresh genesis and qualification default to ruleset 27; ruleset-26 history remains
byte-compatible. ADR 0120 records the provisional boundary.

Canonical candidate v9 carries that boundary into a fresh same-identity genesis. Its exact
ruleset-27 event chain, schema-5 weather binding, snapshot-plus-tail replay, all five projections,
3,939 error-free Hindsight deliveries, person-only cognition deadlines, and one consumed zero-cost
local Qwen result passed the fail-closed qualification report at tick 1,680. The immutable evidence
bundle supersedes candidate v8 for mechanical launch review but does not authorize deployment or
scientific admission. The qualification also caught and removed a replay-schema allowlist trap
before evidence was sealed.

The candidate-v9 pacing finding is now encoded as an operations invariant rather than retained as
tribal knowledge. A cognition-qualified wrapper requires tick 0 / sequence 1, advances one tick,
waits without moving simulation time for one durable free local receipt, and only then invokes the
unchanged bounded runner for the exact remainder. Timeout fails the disposable attempt instead of
moving a cognition deadline or slowing a real world. ADR 0121 records the separation.

Ruleset 28 makes the remaining pinned ERA5 atmospheric summaries physically perceptible without
granting agents weather concepts. A seed-bound paired-day `water_flux` draw exactly preserves every
30-day phase mean, while `air_motion` is the checked absolute-component sum of the measured monthly
wind means rather than a fabricated speed distribution. Both are private touch-channel scalars;
rain, wind, direction, season, forecast, and use labels remain absent. Event, snapshot, and
state-hash schemas advance to 28, fresh genesis and qualification default to it, and the gate
requires both readings in canonical history. ADR 0122 records the provisional boundary.

Canonical candidate v10 carries that boundary into a fresh same-identity genesis. It passed at
tick 1,680 with exact replay, 110,880 perceptions of each new physical scalar, all 4,071 Hindsight
deliveries complete without error, all five projections current, and one consumed zero-cost local
Qwen receipt. Its immutable evidence supersedes candidate v9 for mechanical launch review but does
not authorize deployment or scientific admission. A prior disposable attempt caught canonical
property ordering before any tick committed and was preserved rather than rewritten or promoted.

The candidate-v10 memory-delivery timing is now an explicit runner mode. `memory-worker --drain`
uses the same durable claim, idempotent Hindsight retain, and acknowledgement path, but removes the
continuous service's idle delay and exits only on an empty ready queue. Delivery and storage errors
are fatal in drain mode. Production's continuous retrying worker remains unchanged.

The next canonical input revision corrects EltonTraits fixed-decimal body-mass retention. Physiology
catalog v4 contains 174,070 EltonTraits profiles, and canonical preparation selects adult mass from
that independent set before falling back to Amniote. Source-informed coverage at the committed
origin rises from five to 24 of 32 fauna taxa without merging publications or making mass causal.
Composition 0.1.2 and ADR 0117 pin the correction; it requires a fresh genesis and qualification
before it can supersede the existing immutable candidate.

Ruleset 31 makes that retained mass causal at the first bounded scale. FmrBT's published 293 K
endotherm/ectotherm mass fits replace the universal 100 W placeholder; usable energy reserve is
seven simulation days of the resulting power, while glucose and water transfers are 1% of adult
mass. Weakest-evidence propagation keeps nine mass-derived metabolic values classified as
engineering assumptions and 24 as literature approximations. The compiler emits pinned integer
microwatts, so replay performs no floating-point allometry. ADR 0136 records the remaining
assumptions and the distinct event, state-hash, and snapshot schema boundary.

Ruleset-31 qualification now verifies the same causal boundary from the stored canonical event
history: initialized and born organisms must contain more than one committed metabolic power, every
usable energy reserve must equal exactly seven days of that power, oral profiles must contain varied
body-scaled portions, and at least one physical oral transfer must occur. This database-side proof
complements the launch-evidence verifier's exact mass/profile arithmetic and prevents an input bundle
that was compiled correctly but never exercised by the world from passing production admission.

Ruleset 32 closes the remaining canonical-state gap by recording each exact adult-body-mass
commitment as a private organism event after initialization and birth. The value now survives event
replay, state hashing, and snapshots directly instead of remaining reachable only through the
external body-profile plan or being inferred from metabolic power. Public projections discard the
commitment, and schema-32 validation requires exactly one species-matching value for every organism.
ADR 0137 records the boundary; growth and individual changing mass remain later mechanics.

Canonical input revision v24 rederives the same 14 JSON artifacts and corrects the checksum
manifest to use relative paths. The bundle verifies unchanged after copying to a different host
directory, closing a production portability defect found in v23. Its exact `SHA256SUMS` digest is
recorded in `docs/operations/CANONICAL_INPUT_REVISION_V24_2026-08-08.md`; it remains input evidence,
not qualification or launch authorization.

The canonical initializer now shares a side-effect-free genesis constructor with a database-free
operator proof. That proof validates the portable input manifest and every content-addressed
full-Earth artifact, constructs the exact configured genesis batch and snapshot, and requires
event-zero replay and snapshot replay to reproduce the same state. Its first complete v24 pass
exposed an obsolete single-profile-set assumption before PostgreSQL mutation. Ruleset 32 now pins
every distinct retained body-mass and life-history source-set digest under stable ordered manifest
keys while older rulesets preserve their singular provenance contract. ADRs 0138 and 0139 record
the two boundaries. Database durability and bounded-tick qualification remain separate gates.

Canonical qualification v20 now closes those remaining ruleset-32 gates on a fresh isolated
PostgreSQL and Hindsight pair. Database genesis reproduced the v24 proof hashes, then 1,000 ticks
passed complete replay, snapshot-tail, memory, zero-cost local cognition, projection, mechanics,
mass-state, body-scaled oral-transfer, and public observer checks. The checksum-covered evidence
bundle binds source commit `30a26a0` and contains no canonical event payloads. Ruleset-32 quality
and public-observatory successor admissions now supersede the ruleset-30 launch-review defaults;
both continue to state that scientific admission and public deployment authorization are false.

The live ruleset-33 world exposed a communication asymmetry: organisms could learn that a directly
heard physical form preceded a witnessed motor action, but the action policy offered only weak paths
for copying that form or reusing it under the same embodied pressure. Ruleset 34 adds bounded neutral
imitation and context-conditioned form reuse without words, referents, intentions, or guaranteed
convergence. The existing world receives the stateless policy driver at disclosed tick 65,000, so
all earlier transitions retain their exact replay behavior. ADR 0145 records the boundary.

The same live history then exposed a spatial discretization artifact: all 24 people remained inside
a roughly 200-metre area while exact level-23 patch equality made them socially isolated. Ruleset 35
uses the bounded level-18 local landscape vicinity for capped hearing and one-to-one social
attention, backed by a deterministic per-tick spatial index, and adds a neutral movement tendency
toward directly heard sources that can produce exact-patch physical encounters. Reproduction still
requires exact contact and is never scheduled. The live ruleset-33 world activates the stateless
driver after tick 75,000; ADR 0146 records the replay boundary and the remaining scientific
limitations.
