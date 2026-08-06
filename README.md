# A Tiny Civilization

A persistent, scientifically grounded civilization simulation in which history is
an outcome, not a script. The public observatory will live at
[atinycivilization.com](https://atinycivilization.com).

The project models the actual materials, organisms, and ecological relationships of
Earth across the planet's present-day land masses and oceans, while simulated
people begin without our scientific categories, technologies, institutions, or
historical destination. They can only learn from perception, memory, action,
conversation, experiment, error, and cultural transmission.

The public observatory is outside the simulation. It exposes a live world,
historical timeline, evidence-backed wiki, and an archive of physical artifacts
the civilization may create. Nothing in the civilization can read or perceive
the observer system.

## Non-negotiable principles

- No technology tree, required milestones, historical eras, or guaranteed survival.
- Actual real-world materials, species, biology, ecology, weather, and causal
  processes, represented through documented scientific approximations.
- Agents perceive properties and effects, never privileged database labels.
- Objective truth, subjective memory, cultural belief, and observer inference are
  separate data domains.
- Model output can propose bounded thoughts or actions; it cannot rewrite the world.
- External AI and memory services are optional. The world continues deterministically
  when they are unavailable or budgets are exhausted.
- Randomness is seeded, stream-scoped, auditable, and replayable.
- Extinct worlds are immutable archives; a successor world receives a new seed.
- Supporters may label and follow lives but cannot purchase influence over them.

See [the project contract](docs/PROJECT_CONTRACT.md) and
[architecture overview](docs/ARCHITECTURE.md) for the boundaries that enforce these
principles. The [public roadmap](docs/ROADMAP.md) separates facts that must exist from
tick zero from observer features that can be projected later.

Contributors should begin with [CONTRIBUTING.md](CONTRIBUTING.md); it explains the
determinism, scientific-evidence, privacy, and dependency-boundary rules that apply to
every pull request.

## Status

Foundation under active construction. The first vertical slice will include a
deterministic Rust simulation runner, PostgreSQL event history and snapshots,
structured memory behind a Hindsight adapter, an observer API, and a public site.

No LLM key is required for the deterministic foundation.

The canonical spatial domain is the full Earth. It uses coarse conserved ecology
globally and deterministic finer detail only around causal activity; opening a map can
never refine or change the world. Direct modern infrastructure, borders, writing, and
place labels are excluded, while uncertain reconstruction is disclosed rather than
presented as pristine or prehistoric Earth. See the
[full-Earth decision](docs/adr/0010-full-earth-causal-refinement.md) and
[scientific data plan](docs/science/FULL_EARTH.md).

The [Lower Buffalo–Ozark river valley](docs/science/FIRST_BIOME.md) is the first sourced
high-resolution conformance tile, not a world boundary or selected starting site.
Canonical genesis remains blocked until the complete content-addressed global bundle,
causal refinement, and deterministic partition scheduler pass their published gates.
The [scientific bundle contract](docs/science/DATA_BUNDLES.md) and offline
`civilization-data` validator enforce those gates without fetching from the network,
including exhaustive content-hash traversal of global tile indexes and leaves.
The pure single-worker L10 scheduler kernel now pins ordering, barrier, and capacity
semantics and matches a synthetic dense reference history. A private integer-only
EPSG:4978-to-S2 reference also pins all six faces, exact projection boundaries, and
L10/L14/L18/L23 ancestor routing. A private L10-to-L14 refinement proof now allocates
all 256 descendants with exact integer conservation, deterministic residual streams,
canonical reaggregation, and an explicit Alabama-paradox guard. Neither reference
changes durable schemas. Persistent full-Earth execution remains intentionally
disabled until exact source bundles, coupled ecological quantities, retained refined
state, locomotion physics, scheduling, and persistence are integrated. Schema-v4
state now retains and replays a full-Earth organism's durable S2 embodied-patch
position and resolved movement, while public projections intentionally omit both.

Exact upstream snapshots now pin both Natural Earth v5.1.2 generalized global land
polygons and NOAA ETOPO 2022 v1 global 60 arc-second bedrock relief. The latter is
491,284,376 verified NetCDF bytes plus official release, license, and version evidence;
it brings real global elevation and bathymetry evidence without mistaking it for an
ecological bundle. HTTPS acquisition streams into an ignored cache, never replaces an
existing file, and finishes with complete offline verification. See
[ADR 0015](docs/adr/0015-exact-upstream-source-snapshots.md).

The pinned Natural Earth source has now produced an independently inspected,
content-addressed L6→L10 `land-reference` release: 24,576 packed tiles and 6,291,456
exact S2-cell-centre classifications, rooted at
`d8ac669b89f2903987766a2f55763b415bd7234097307ff63fcb7771099580ac`. It is a
generalized cartographic land cross-check, explicitly not a coastline or a complete
ecological bundle.

All twelve CHELSA-BIOCLIM+ v2.1 monthly 1981–2010 land-temperature normals are also
pinned with exact hashes and a shared-grid inspection gate. They are annual climate
evidence, not a weather model or a complete climate layer; ocean forcing,
precipitation, wind, and ecological coupling remain required.

Every person remains an individual even if the population becomes enormous. Load may
slow or pause wall-clock advancement after a committed hash boundary; it may never
change fertility, mortality, cognition, or event detail. The project publishes measured
capacity rather than claiming unlimited real-time scale. See the
[population-scale decision](docs/adr/0011-population-scale-and-capacity.md).

## Verify the core claim

The repository includes a non-production history bundle grounded to the real GBIF
`Homo sapiens` taxon. Verify its event hash chain, replay from genesis, replay from a
snapshot plus its tail, final state hash, and mechanical extinction entirely offline:

```bash
cargo run --locked -p civilization-verify -- \
  verify verification/demo-bundle.json
```

Regenerate the same bytes and compare them with the committed bundle using
`./scripts/verify-demo.sh`. Neither command contacts PostgreSQL, Hindsight, an LLM, or
the network.

## Repository layout

The intended modular-monolith layout is:

```text
apps/                 Rust process entry points
crates/               Domain, engine, application, and adapters
db/migrations/        Versioned PostgreSQL migrations
docs/                 Architecture decisions and operating documentation
web/                  Public observatory and wiki
```

## Development

Prerequisites are Docker Engine, Docker Compose, and Make. Start the complete local
foundation with:

```bash
cp .env.example .env
make up
make smoke
```

Then open `http://127.0.0.1:3000`. PostgreSQL, the migration job, observer API,
simulation runner, observer projector, and web application start in dependency order.
The projector only reads committed event batches and builds append-only public timeline
rows; it cannot advance a world. Host ports bind to loopback and are not publicly
exposed.

Inspect the stack with `make ps` or `make logs`, and stop it with `make down`.

The runner deliberately does not invent a seed or silently create a world. To exercise
durable ticks without claiming to launch the eventual full-Earth world, explicitly
initialize the built-in non-production proof fixture:

```bash
make proof-world \
  WORLD_ID=019fd4a9-b7f9-7891-ab51-cdf71d2b7701 \
  WORLD_SEED=101
```

The already-running runner detects that committed genesis, verifies its complete event
history against the latest snapshot, and advances one simulation transition per wall
interval without skipping simulation steps. Restarting the runner causes another full
verification and resumes at the exact next sequence. Repeating `init-proof` is
idempotent only when the immutable manifest and genesis inputs match. The proof command
is never used to select or initialize a canonical public-world seed.

Hindsight is optional and intentionally excluded from the default startup because its
full development image is large. Start the pinned keyless service with:

```bash
make hindsight-up
```

The first start downloads and caches roughly 220 MB of open local embedding/reranking
models in a named volume; subsequent container replacements reuse that cache. A host
that later forbids all container egress can set `HINDSIGHT_HF_OFFLINE=1` after this
first successful start to suppress model-metadata refreshes.
This starts Hindsight 0.8.6 in provider-`none`/zero-LLM chunk mode plus a separate
memory-delivery worker. Subjective records are committed to PostgreSQL with their
source transition before the worker can send them. Stable operation and document IDs
make lost acknowledgements safe to retry; service failure never blocks a simulation
tick. The current proof engine does not fabricate subjective perceptions, so the queue
remains empty until embodied perception begins in the full-Earth/reference-tile
milestone.

No LLM key is needed for retain or recall in this mode. The project will request one
only when model-backed extraction and reflection are ready for an explicit integration
test.

With the local stack running, execute all unit, PostgreSQL integration, architecture,
and web checks outside containers with:

```bash
make check
```

Secrets must never be committed; `.env.example` documents every supported runtime
value while `.env` remains ignored.

## License

Licensed under the [Apache License 2.0](LICENSE). Contributions are welcomed under
the same license.
