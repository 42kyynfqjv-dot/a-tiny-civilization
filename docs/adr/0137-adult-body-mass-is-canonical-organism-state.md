# ADR 0137: Adult body mass is canonical organism state

Date: 2026-08-08

Status: Accepted

## Context

Ruleset 31 compiles exact adult-body-mass commitments into metabolic power, energy reserve, and
oral-transfer quantities, but the mass commitment itself remained only in the external genesis
plan. A later movement, thermal, growth, or material interaction rule would therefore have to infer
mass from a derived quantity or reopen an input artifact during replay. Neither route is an
acceptable canonical-state boundary.

## Decision

Ruleset and schema 32 add a private `organism_adult_body_mass_committed` event. Genesis emits exactly
one event after each organism initialization, and every birth emits exactly one after the birth
event using the developing parent's species-bound adult-mass commitment. Applying the event rejects
unknown organisms, duplicate commitments, invalid fixed-point values, and species mismatches.

The commitment is retained directly in organism state, state hashes, and snapshots. Ruleset-32
validation requires one for every organism; earlier rulesets reject the new state and retain their
existing bytes. Public timeline, finding, and organism projections deliberately discard the event.
It is physical provenance, not an inhabitant-facing body-size concept or observer biography claim.

## Consequences

- Replay and snapshot restoration no longer need an external body-profile artifact to recover mass.
- Future physical couplings can consume the exact committed fixed-point mass without reverse
  engineering metabolic power.
- The current birth rule inherits an adult species/body-profile commitment, not a changing juvenile
  mass. Growth and individual mass variation remain separate future ruleset decisions.
- A fresh ruleset-32 genesis and qualification are required; ruleset-31 history remains immutable.
