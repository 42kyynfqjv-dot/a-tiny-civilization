# ADR 0043: Neutral local signal propagation

## Status

Accepted on 2026-08-07.

## Decision

An emitted signal is a physical, local sound observation. It carries only a bounded
amplitude, source identity, exact simulation tick, and local spatial eligibility.
It is not a word, message, intention, language token, social role, or observer
annotation. Recipients receive the same label-free sound reading through canonical
events; memory, association, imitation, and any later learned convention remain
separate mechanisms.

Propagation is deterministic, scoped to the same embodied patch in the first
implementation, ordered by recipient identity, and recorded in the event log. Public
projections do not expose raw signals or infer their meaning.

## Consequences

The first signal ruleset needs replay, snapshot, event-schema, capacity, and
observer-boundary coverage. Range, attenuation, and species-specific hearing require
source-pinned evidence before they replace same-patch delivery.
