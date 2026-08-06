# ADR 0019: Public organism records are safe, evidence-backed observer indexes

## Context

The observatory needs people and animal pages without assigning a simulated person an
observer identity or leaking private implementation detail into public presentation.
The first durable event schema already carries stable organism identity and sourced
species facts, while birth category, parentage, location, and mortality mechanism are
not appropriate for the restrained public surface.

## Decision

- `public-organism-v1` reads only checksum-verified committed event batches and has an
  independent durable cursor.
- It writes immutable organism-introduction rows and immutable life-ending rows. A
  public life record joins those observer facts; no row contains a birth category,
  parent, location, death cause, or supporter alias.
- Every listed record identifies its source event, sequence, tick, species citation,
  and provenance (`world_fact`). The API is bounded and read-only.
- The projector rejects missing, altered, out-of-order, duplicate-introduction, and
  duplicate-ending history rather than inventing a repair.

## Consequences

Individual people and animals can be browsed from their first durable appearance,
including after a world is archived. Rich biographies, lineages, artifacts, and wiki
interpretation can be added as later projections without changing the canonical log.
Observer aliases remain an isolated supporter concern and never become an organism
attribute here.
