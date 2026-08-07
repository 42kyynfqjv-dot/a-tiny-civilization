# ADR 0037: Fauna begin with real taxa and separately licensed evidence

## Status

Accepted on 2026-08-07 for breadth-first implementation. Final occurrence selection,
trait coverage, ecological inference, and scientific validation remain required before
canonical genesis.

## Context

The world must contain actual real-world animals rather than invented analogues. A
taxonomic name list alone does not establish where an animal lives, how abundant it is,
or how its body behaves. Conversely, the convenient GBIF public-cloud occurrence
snapshot is distributed under noncommercial terms as a whole, which is incompatible
with an Apache-2.0 project that may accept supporter payments even though many
constituent records are CC0 or CC BY.

## Decision

- Acquire GBIF's frozen 2023-08-28 Backbone Taxonomy under CC BY 4.0 as the first
  stable identity catalogue. Its identifiers remain citable even though GBIF now uses
  Catalogue of Life XR as its primary live taxonomy; a later crosswalk can add COL XR
  identities without rewriting old organism identities.
- Obtain geographic evidence through a separately DOI-issued GBIF occurrence download,
  filtered to records whose individual licenses permit commercial reuse. Do not ingest
  the blanket CC-BY-NC cloud snapshot.
- Treat occurrences as presence evidence with sampling bias, not range polygons or
  abundance. Infer provisional habitat support only from multiple compatible records,
  climate, terrain, water, soil, and land cover, retaining the method and uncertainty.
- Add physiology and life-history parameters from separately cited, commercially
  compatible trait sources. Missing values remain explicit assumptions or conservative
  taxonomic estimates; they never become fictional species facts.
- Use durable individual identities for people and supporter-eligible animal tiers from
  tick zero. High-volume small organisms may use explicit cohorts whose abundance and
  life stage remain conserved.
- Public projections remain non-graphic. Reproduction, predation, injury, and death may
  affect canonical state without exposing explicit sexual or violent presentation.

## Consequences

The first taxonomy archive can be acquired without credentials using
`scripts/acquire-gbif-taxonomy.py --download`. A geographically filtered occurrence
download later requires a GBIF owner account so it receives a durable DOI; that is an
operations handoff, not a reason to stall taxonomy, fauna schemas, or behavior code.

References: [GBIF Backbone Taxonomy](https://www.gbif.org/dataset/d7dddbf4-2cf0-4f39-9b2a-bb099caae36c),
[GBIF taxonomy interpretation](https://techdocs.gbif.org/en/data-processing/taxonomy-interpretation),
and [GBIF citation guidance](https://www.gbif.org/citation-guidelines).
