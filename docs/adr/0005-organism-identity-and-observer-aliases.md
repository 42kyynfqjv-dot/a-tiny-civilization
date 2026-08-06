# ADR 0005: Organism identity is canonical; supporter names are observer aliases

- Status: accepted
- Date: 2026-08-06

## Context

Biographies, genealogy, replay, mortality, and later supporter naming cannot recover
individuals that were represented anonymously. At the same time, assigning a durable
row to every insect, egg, or microscopic organism would overwhelm the useful scale of
the simulation. Supporter state must never become a cause of births or behavior.

## Decision

- Every individually modeled organism receives a stable world-scoped identity before
  its first canonical event.
- Birth events carry deterministic ordering, real species identity, parent identities
  when known, versioned reproductive/birth attributes, place, and tick.
- Death and lineage facts are canonical events. In-world social names, if developed,
  are situated cultural facts and are not observer aliases.
- Each ruleset declares identity tiers by real species and life stage. Cohort modeling
  must be explicit; no blanket vertebrate/invertebrate shortcut is assumed.
- Any species offered for supporter naming must emit individual births with stable
  identity before reservations for it can be sold.
- A reservation records the first world sequence it may match. Only a later eligible
  committed birth can be matched, using deterministic queue order.
- Matching and alias attachment occur in the observer application after the birth.
  Simulation crates cannot depend on supporter, authentication, or payment crates.
- Rejecting, refunding, or removing an observer alias never modifies the birth event.

## Consequences

- Deferred biographies and wiki pages can reconstruct all individually retained lives.
- High-volume species can use documented cohorts without pretending anonymous members
  are recoverable later.
- Supporters can follow a life without purchasing its existence or influencing it.
- CI needs an explicit dependency-direction check before supporter code is introduced.
