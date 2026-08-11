# ADR 0149: populated world successors use an auditable retirement event

Status: accepted on 2026-08-11.

## Context

The public ruleset-33 world predates the ruleset-36 cognition design. The owner authorized a fresh
Genesis after the new cognition mechanics were implemented. PostgreSQL correctly permits only one
active open-ended observer world, while Cancer World occupies a separate experiment slot.

Treating the old population as dead would be false. Updating its database status without a canonical
event would break replay. Continuing both ordinary worlds indefinitely would defeat the single-live-
world contract and make the observatory's default ambiguous.

## Decision

A populated ordinary world may be closed only by the explicit canonical event
`world_retired_for_successor`, which contains the immutable successor world ID.

- `retired` is a distinct terminal status. It is neither `extinct` nor `archived` by mechanical
  extinction.
- Living organisms remain living in the final state. No death or extinction event is fabricated.
- The event is observer-side lifecycle metadata and cannot be perceived inside either world.
- Any pending fixed-deadline cognition request is resolved as `world_retired` in the same atomic
  transition, with zero external-evidence hashes.
- A retired world is immutable at the database trigger and store layers.
- Only an uninitialized successor ID may be bound, and the successor must later cite the retired
  world as its predecessor.
- The operator command requires a literal confirmation flag, verifies the entire source history,
  holds the canonical-writer lock, and rejects experimental worlds.
- The observatory states plainly that observation moved to a successor and that the population was
  not reported extinct.

## Verification

The engine test proves retirement preserves living people, selects event schema 37 and snapshot
schema 36, and replays from genesis. A cognition test proves a pending request is resolved in the
same batch. An isolated PostgreSQL integration test proves the migration, immutable terminal state,
default observer ordering, predecessor link, and one-successor slot end to end.

## Consequences

This is an explicit human lifecycle intervention and must never be described as an emergent outcome.
The retired history remains downloadable and verifiable. Future worlds normally end through their
mechanical extinction condition; this path exists for disclosed versioned successor cutovers and
requires direct owner authorization.
