# Emergent Civilization

A persistent, scientifically grounded civilization simulation in which history is
an outcome, not a script.

The project models an Earth-like physical and ecological world while simulated
people begin without our scientific categories, technologies, institutions, or
historical destination. They can only learn from perception, memory, action,
conversation, experiment, error, and cultural transmission.

The public observatory is outside the simulation. It exposes a live world,
historical timeline, evidence-backed wiki, and an archive of physical artifacts
the civilization may create. Nothing in the civilization can read or perceive
the observer system.

## Non-negotiable principles

- No technology tree, required milestones, historical eras, or guaranteed survival.
- Real-world-inspired materials, biology, ecology, weather, and causal processes.
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
principles.

## Status

Foundation under active construction. The first vertical slice will include a
deterministic Rust simulation runner, PostgreSQL event history and snapshots,
structured memory behind a Hindsight adapter, an observer API, and a public site.

No LLM key is required for the deterministic foundation.

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

Developer setup commands will be added with the bootable foundation checkpoint.
Secrets must never be committed; copy `.env.example` to `.env` when configuration
is needed.

## License

No reuse license has been selected yet. The source is publicly visible, but all
rights remain reserved until a license is added.
