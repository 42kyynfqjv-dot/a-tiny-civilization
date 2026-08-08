# ADR 0117: Fauna ecology evidence is pinned before behavior

Date: 2026-08-08

Status: Accepted

## Context

The retained EltonTraits artifact has exact stable-identity diet and activity aggregates for 23 of
the 32 fauna taxa selected at the committed origin. Turning those observer descriptions directly
into drives, actions, habitat suitability, or privileged food labels would script behavior and
violate the discovery boundary.

## Decision

A canonical `FaunaEcologyPlan` pins the exact profile-set digest, real taxon identity, and ordered
source-record identifiers for retained `diet-*` and `activity-*` profiles. Resolution fails closed
on changed profile bytes, missing rows, noncanonical ordering, or non-ecology traits.

The plan is an evidence boundary only. It is not organism state, is not exposed through perception,
and supplies no action, preference, affordance, habitat, abundance, or survival rule. Genesis
derivation and manifest integration follow this contract; any causal use requires a later ruleset
and a separate decision for each transformation.

## Consequences

Available real-world ecology evidence can be retained now without giving inhabitants biological or
cultural knowledge they did not discover. Uncovered taxa remain explicit absence rather than
imputed ecology. The public wiki may later cite the source rows as observer evidence, but cannot
present them as observed behavior in the simulated world.
