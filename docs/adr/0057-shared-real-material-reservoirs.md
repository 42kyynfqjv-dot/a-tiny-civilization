# ADR 0057: Survival resources are shared real-material reservoirs

Status: accepted and implemented on 2026-08-08 for ruleset 17.

## Context

Ruleset 16 gave organisms finite physiological reserves and label-free oral material
transfer, but genesis contained no accessible material. A full-world accelerated run
therefore ended in synchronized metabolic death at tick 334. Portable objects alone
also create an accidental exclusivity rule: one holder can prevent every colocated
organism from interacting with a survival source.

The first executable ecology is intentionally provisional. It must use actual cited
material identities without presenting invented regional abundance, renewal, or
species response values as measurements.

## Decision

Ruleset 17 adds spatially anchored, non-holdable `MaterialReservoirCommitment` state.
Each commitment binds a real `MaterialIdentity`, coverage cell, maximum mass, renewal
rate, evidence class, and profile digest. Species-specific oral-transfer commitments
remain physical consequences rather than agent-visible concepts such as food, water,
safe, or useful.

All colocated organisms may independently select a reservoir. Withdrawals are resolved
in the canonical partition/work order. The first withdrawal at a tick lazily settles
renewal since the reservoir's last settlement; later withdrawals at the same tick add
no second renewal. Each transfer event records the complete before/renew/transfer/after
calculation, so replay performs no clock, dataset, or ecology lookup.

Genesis commits organisms and reservoirs in one PostgreSQL append. Ruleset 17 refuses
genesis without a reservoir. Event schema 19, snapshot schema 20, and state-hash schema
20 own the new state while every earlier ruleset retains its published encoding.

The first canonical provisional resource artifact uses cited PubChem identities for
D-glucose and water. Their regional availability, renewal, and every species response
are explicitly `engineering_assumption`; the artifact status is
`provisional-not-scientifically-admitted`. These sources are an executable survival
bridge, not a claim that natural landscapes contain exposed pools of either material.

Reservoir mechanics and bodily transfers are withheld from public projections. The
observatory may later derive evidence-backed descriptions of visible behavior without
exposing private physiological or reproductive mechanics.

## Consequences

- Shared access no longer depends on possession or observer attention.
- Renewal produces events only when causally observed by a withdrawal, avoiding one
  background event per source per tick.
- The provisional world can be endurance-tested before local flora, hydrology, and
  trophic pathways receive scientific admission.
- Replacing the two-source bridge requires a new canonical artifact and ruleset, never
  mutation of a live world's commitments.
