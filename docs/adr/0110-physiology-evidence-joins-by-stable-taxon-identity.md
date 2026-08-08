# ADR 0110: Physiology evidence joins by stable taxon identity

Date: 2026-08-08

Status: Accepted

## Context

Independent source datasets can cite the same GBIF taxon while rendering its scientific name
differently. The canonical range selection uses accepted binomials; AnimalTraits often includes the
taxonomic authority. Requiring the complete display identity to be equal discarded exact retained
measurements for `Turdus migratorius` and `Catharus guttatus` even though both sides pinned the same
GBIF identifiers.

Metabolic evidence coverage is also sparse. Requiring an observation for every selected fauna taxon
made the presence of one uncovered species erase valid evidence for every covered species.

## Decision

- Cross-source physiology joins require exact catalog and taxon identifier equality. They never use
  fuzzy scientific-name matching.
- A resolved body commitment uses the world's canonical species identity. The selected source record
  ID, source-row digest, profile-set digest, and exact measured value remain unchanged.
- Metabolic-rate plan schema two records the complete selected subset with retained exact evidence.
  An empty schema-two plan means measured coverage is zero; it is not an error or an estimate.
- Every uncovered species remains an explicit engineering assumption in the provisional body plan.
  A source measurement for one species is never generalized to another.
- Legacy nonempty schema-one plans retain their original bytes and validation.

## Consequences

The committed candidate now retains two exact source measurements and labels the other 30 fauna
rates as assumptions instead of choosing between all-measured and all-assumed. Scientific-name
presentation changes cannot break a stable catalog join, while catalog mismatches still fail closed.
Sparse coverage remains visible and can be replaced only in a successor candidate or world input.
