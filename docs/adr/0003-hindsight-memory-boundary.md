# ADR 0003: Hindsight is an asynchronous subjective-memory adapter

- Status: accepted
- Date: 2026-08-06

## Context

Hindsight provides durable memory banks, retain/recall operations, graph retrieval,
and optional model-backed observations and reflection. Its extraction and reflection
can be unavailable, provider-dependent, or nondeterministic. It must not become the
source of world truth or a dependency of the simulation tick.

## Decision

- Integrate Hindsight through its HTTP API behind a project-owned `AgentMemory` port.
- Pin the initial development image to `ghcr.io/vectorize-io/hindsight:0.8.6`.
- Begin with `HINDSIGHT_API_LLM_PROVIDER=none`; no model key is required.
- Make keyless behavior explicit with `HINDSIGHT_API_RETAIN_EXTRACTION_MODE=chunks`;
  retain stores searchable chunks without calling an LLM.
- Use one isolated memory bank per character life and world.
- Commit a memory-outbox item in the same PostgreSQL transaction as its source event,
  then retain it asynchronously with deterministic operation/document identifiers.
- Provide Hindsight, no-op/fallback, and recorded-replay implementations.
- Record exact recall/reflect inputs and results used by cognition. Replay uses the
  recorded result and never calls Hindsight or a model.
- A timeout, authentication failure, malformed response, or unhealthy service returns
  an explicit unavailable outcome and never blocks a simulation tick.

## Operational configuration

- Development may use Hindsight's embedded storage in a disabled Compose profile.
- Production uses a dedicated Hindsight database/schema and workers; Hindsight does
  not own simulation migrations.
- Production authentication is enabled and the service is not publicly routed.
- `HINDSIGHT_API_FAIL_ON_EXTRACTION_ERRORS=true` prevents silent partial extraction.
- A stable worker identifier prevents in-flight work being stranded on container
  replacement.
- Development sets `HINDSIGHT_API_STORE_DOCUMENT_TEXT=false`; normalized memory units
  remain searchable while duplicate raw document storage is disabled.

## Consequences

- The deterministic engine can ship and run before any LLM credential is supplied.
- Memory delivery is eventually consistent and observable through the local outbox.
- Bank configuration and provider output remain versioned replay inputs.
- An additional fallback working-memory implementation must be maintained and tested.

## Verified references

- [Hindsight 0.8.6 release](https://github.com/vectorize-io/hindsight/releases/tag/v0.8.6)
- [Installation](https://hindsight.vectorize.io/developer/installation)
- [Retain API](https://hindsight.vectorize.io/developer/api/retain)
- [Recall API](https://hindsight.vectorize.io/developer/api/recall)
- [Configuration](https://hindsight.vectorize.io/developer/configuration)
