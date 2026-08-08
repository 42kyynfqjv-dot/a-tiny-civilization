# ADR 0116: Body-mass observations require an explicit world selection

Date: 2026-08-08

Status: Accepted

## Context

AnimalTraits legitimately retains multiple positive adult-body-mass observations for some exact
taxa. The first retained body-mass artifact was also ordered by numeric catalog identifier, while
the canonical Rust contract orders identifier strings lexicographically. Its inspection described
the intended bytes, but the checked-in artifact could not pass the canonical reader.

## Decision

The corrected AnimalTraits v2 artifact is rebuilt from the same pinned CC0 source bytes and frozen
GBIF catalog using the canonical ordering contract. Physiology catalog v3 references the corrected
artifact and the active provisional composition references catalog v3.

A `FaunaBodyMassPlan` now pins at most one exact positive gram observation for each world fauna
taxon by profile-set digest and source-record identifier. Plan derivation selects the first record
in the already validated canonical profile order, records that choice, and leaves uncovered taxa
absent. Body-profile derivation accepts source mass only through this plan. It rejects mismatched
profile bytes, unplanned taxa, missing records, invalid units, and noncanonical plan order.

Canonical preparation currently selects from Amniote because it covers five of the 32 selected
fauna taxa, compared with three from AnimalTraits at this origin. This is a deterministic coverage
choice disclosed in genesis inputs, not a claim that either source is universally superior.

## Consequences

Multiple measurements remain source-faithful and usable without averaging or accidental row-order
coupling. The formerly unreadable AnimalTraits evidence is retained correctly for future worlds.
Adult mass remains noncausal under ADR 0115 until separate allometric mechanics are admitted.
