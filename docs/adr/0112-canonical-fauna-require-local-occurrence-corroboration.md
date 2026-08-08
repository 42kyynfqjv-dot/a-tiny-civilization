# ADR 0112: Canonical founder fauna require local-occurrence corroboration

Date: 2026-08-08

Status: Accepted

## Context

The pinned iNaturalist modeled-range release is useful global evidence, but a polygon containing the
committed origin does not establish that a species has actually been observed nearby. The first
candidate chain therefore retained geographically implausible founder taxa. Curating that list by
hand or selecting another seed would violate the no-reroll and no-authored-tech-tree principles.

## Decision

Canonical preparation intersects the modeled-range candidates with a separately acquired,
hash-pinned iNaturalist observation query centered on the committed origin. The query is limited to
research-grade, non-captive Animalia observations within 75 kilometres whose observation record is
licensed CC0 or CC BY. It retains raw response pages, a canonical query manifest, and a normalized
evidence artifact containing observation identity, taxon identity, date, license, and source URL.

This intersection is only evidence of reported local presence. It makes no claim about abundance,
native or introduced status, habitat suitability, observation independence, or exact founder count.
The existing seed-derived identity-tier selection operates on the corroborated pool without an
authored allowlist or seed change. Canonical preparation fails closed when the source directory is
absent. Initialization independently rederives the intersection and rejects different bytes, while
the final occurrence-evidence digest is retained in the world manifest.

## Consequences

Obvious global-range false positives cannot silently become canonical founders merely because their
modeled polygon covers the origin. Every selected founder taxon has both range-model and local
observation provenance. Sparse observations can still underrepresent real local fauna and cannot
support population estimates, so those limitations remain explicit scientific assumptions rather
than being hidden in founder selection.
