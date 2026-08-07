# ADR 0056: Provisional metabolism remains explicit in canonical body state

## Status

Accepted and implemented on 2026-08-07 in metabolic-rate commitment schema two.

## Context

The first seed-derived origin has 2,148 real species in commercially usable iNaturalist
modeled ranges but no exact species-identity overlap with the retained AnimalTraits
metabolic observations. Blocking all mechanics on a new scientific join conflicts with
the implementation-first integration plan. Writing a guessed value into a type that
implicitly meant “source measurement” would be worse: replay would be deterministic,
but the provenance claim would be false.

## Decision

- Metabolic-rate commitment schema two carries an explicit
  `PhysiologicalEvidenceBasis` alongside its fixed-point power value.
- Source-derived selections are marked `source_measurement`; temporary values are
  marked `engineering_assumption` in canonical organism state.
- Legacy schema-one commitments deserialize as source measurements and reserialize to
  their original field shape, preserving archived replay bytes.
- The provisional artifact generator can therefore exercise real, locally ranged taxa
  before scientific admission without representing its temporary physiology as
  evidence.
- Exact metabolic-plan generation remains available and fails closed when a planned
  species lacks a retained positive watt observation.
- No assumption changes the species identity, range provenance, or public claim that
  the value is provisional and unvalidated.

## Consequences

The full integration path can test fauna identity, reproduction, persistence, and
capacity now. Scientific review can replace assumption artifacts before genesis or in
a successor world, but it cannot rewrite a launched world's committed inputs. Public
provenance can distinguish measured, literature-approximated, and engineering values
without reverse-engineering record identifiers.
