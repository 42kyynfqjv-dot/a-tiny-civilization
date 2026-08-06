# A Tiny Civilization

A persistent, scientifically grounded civilization simulation in which history is
an outcome, not a script. The public observatory will live at
[atinycivilization.com](https://atinycivilization.com).

The project models the actual materials, organisms, and ecological relationships of
Earth while simulated
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

## Status

Foundation under active construction. The first vertical slice will include a
deterministic Rust simulation runner, PostgreSQL event history and snapshots,
structured memory behind a Hindsight adapter, an observer API, and a public site.

No LLM key is required for the deterministic foundation.

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
simulation runner, and web application start in dependency order. Host ports bind to
loopback and are not publicly exposed.

Inspect the stack with `make ps` or `make logs`, and stop it with `make down`.

The runner deliberately does not invent a seed or silently create a world. To exercise
durable ticks without claiming to launch the eventual real-biome world, explicitly
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

No LLM key is needed in provider-`none` mode. The project will request one only when
model-backed extraction and reflection are ready for an explicit integration test.

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
