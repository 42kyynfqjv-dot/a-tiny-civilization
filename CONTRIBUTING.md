# Contributing to A Tiny Civilization

Thank you for helping build a public, auditable experiment. Contributions are accepted
under the repository's [Apache-2.0 license](LICENSE).

## Before opening a change

Read the [project contract](docs/PROJECT_CONTRACT.md),
[architecture](docs/ARCHITECTURE.md), and [roadmap](docs/ROADMAP.md). They are part of
the engineering interface, not background reading.

The most important boundary is simple: observer, payment, account, wiki, model, and
memory systems may observe or interpret committed history, but may not select or alter
a canonical world outcome. Do not add a dependency from the runner or engine to an
observer-facing crate.

## Good first contributions

- tests that make a determinism, replay, persistence, privacy, or boundary invariant
  harder to accidentally break;
- source snapshots with official release, version, license, and citation evidence;
- documentation that distinguishes source evidence, normalized world facts,
  in-world claims, and observer interpretation;
- restrained observer projections that cite committed events and keep biological and
  violence mechanisms out of public presentation;
- accessibility, reliability, and developer-experience fixes that preserve the
  existing contracts.

Please open an issue or discussion before changing event schemas, hashing material,
world configuration, scientific normalization semantics, agent perception/action
grammar, live-world rules, or public moderation policy. Those changes need an ADR and
an explicit migration/replay story before implementation.

## Local checks

The standard full check uses the local Compose PostgreSQL service and verifies Rust,
database integration, architectural dependency boundaries, deterministic replay, and
the observatory:

```bash
cp .env.example .env
make up
make check
```

For the self-contained core proof, run:

```bash
cargo run --locked -p civilization-verify -- verify verification/demo-bundle.json
```

Do not commit `.env`, source caches, downloaded artifacts, database volumes, build
outputs, credentials, payment data, or supporter information.

## Pull requests

Keep each change narrow and describe:

1. the invariant or user-visible behavior it changes;
2. why it cannot influence canonical history if it is observer-side;
3. tests run and any intentionally untested external integration;
4. for scientific inputs, exact source/license evidence and every approximation or
   assumption introduced.

Do not rewrite committed event history or edit an existing migration after it has been
published. Add a forward migration. Do not silently tune a live world for pacing or
audience response; follow the live-world patch policy in
[ADR 0006](docs/adr/0006-world-provenance-and-lifecycle.md).

## Conduct and safety

Be respectful and discuss the work in good faith. Report security issues privately as
described in [SECURITY.md](SECURITY.md). For public-facing copy, avoid explicit sexual
or violence detail and do not make claims of agent sentience or scientific accuracy
that the retained evidence cannot support.
