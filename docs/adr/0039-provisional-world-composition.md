# ADR 0039: Breadth-first composition cannot impersonate scientific admission

## Status

Accepted on 2026-08-07 for provisional full-world integration.

## Context

The project deliberately wires every world domain before the final coupled scientific
review. A partial release must be runnable enough to expose integration defects, but a
generic world-data manifest would let provisional roots accidentally look eligible for
canonical genesis. Documentation alone is too weak a boundary.

## Decision

- Use a separate canonical `ProvisionalWorldComposition` wire type. It is not a
  `WorldDataBundle` and exposes no conversion into one.
- Its only representable status is `provisional-not-scientifically-admitted`.
- A complete composition contains exactly the seven ordered Earth roles: bathymetry,
  climate, coastline, elevation, habitat, hydrography, and soil.
- It also contains exact references for the celestial ephemeris, real fauna catalog,
  and fauna trait evidence. Every artifact reference pins its path, media type, digest,
  byte length, commercially usable license expression, current scientific scope, and
  limitations.
- Outstanding cross-layer validation gaps are mandatory, canonical manifest content.
  They cannot be omitted to make a provisional release appear finished.
- Canonical bytes, strict ordering, unique artifact identities, safe relative paths,
  and content hashes make compositions repeatable and tamper-evident.
- Scientific admission remains an explicit later operation that constructs and
  validates a separate `WorldDataBundle`; it is never a status-field edit.

## Consequences

The complete-first workflow can compose and exercise the entire physical input surface
without weakening the canonical-genesis gate. Finishing a provisional composition is
evidence of integration coverage, not evidence of scientific accuracy. Independent
rebuilds, units, uncertainty, coupling, ecology, assumptions, and source review remain
required before an admitted bundle exists.
