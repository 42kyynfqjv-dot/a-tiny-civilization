# ADR 0115: Adult body mass is retained before causal coupling

Date: 2026-08-08

Status: Accepted

## Context

The source pipeline retains adult-body-mass aggregates for exact real taxa, but canonical body
profiles previously discarded them. Immediately deriving movement, reserve, ingestion, or thermal
physics from mass would require additional reviewed allometry and would turn one sourced value into
several unsupported claims.

## Decision

Body-profile plan schema three requires one species-bound adult-body-mass commitment per taxon. An
exact catalog/taxon match to one positive gram-valued retained Amniote aggregate carries the exact
profile-set and source-row digests and is classified as a literature approximation. Missing taxa
receive a coarse class-level engineering assumption with an independently addressable digest.
Ambiguous multiple exact records fail derivation rather than being silently averaged or selected.

The body-plan digest and contributing source-profile-set digest are retained in the world manifest.
The engine does not yet copy mass into organism state or use it in any causal equation. A later
ruleset must separately specify and validate each mass-based physical coupling. Schema-one and
schema-two body plans retain their previous canonical bytes and replay behavior.

## Consequences

Canonical preparation no longer loses available body-mass provenance, and future physical work has
a stable input boundary. Retention alone is not presented as physiology or scientific admission.
Most current founder taxa still use explicit assumptions because exact stable-identity coverage is
sparse; improving the taxonomic crosswalk or source set can replace those values in a successor
candidate without disguising the remaining gaps.
