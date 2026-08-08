# ADR 0096: Label-free surface regions preserve arrangements

Accepted on 2026-08-08 as ruleset 20.

## Decision

Every material object in ruleset 20 retains eight independently addressable physical contact
regions. `PrimitiveAction` gains an optional `contact_region` motor coordinate that is valid only
for `apply_force`; the coordinate carries no glyph, mark, character, writing, purpose, or material
affordance label. A held-object force action selects one region, changes that region and the
aggregate trace by the same exact force amount, and gives the actor direct touch readings of both
bounded values.

The deterministic baseline policy receives eight otherwise equivalent force candidates when it
holds an object. Selection remains driven by the existing seeded, situated policy and can later be
influenced only through the existing bounded cognition contract. The engine neither rewards a
pattern nor decides that any arrangement represents something.

Ruleset 19 histories keep their aggregate trace event, empty region state, schema 21 events, and
schema 22 state/snapshots. Ruleset 20 uses event schema 22 and state/snapshot schema 23. Its new
event records exact before/after values for both the selected region and total.

## Observer boundary

`public-artifact-v1` stores the optional contact region alongside the same world-fact trace delta.
Legacy ruleset-19 rows retain `NULL`. The public evidence route may expose the quantitative region,
but artifact classification remains a separate observer inference and the private actor is never
projected.

## Enforcement and qualification

- Actions reject out-of-range regions and regions on any primitive other than `apply_force`.
- Region and total arithmetic must each equal the exact applied force and remain bounded.
- Engine validation reconstructs the complete trace/perception set from primitive actions and
  rejects missing, extra, aggregate-only, or fabricated regional events.
- Canonical validation requires exactly eight regions whose checked sum equals the aggregate.
- Snapshot-plus-tail and genesis replay must match the committed ruleset-20 state.
- Pre-genesis qualification requires at least one projected non-null region for ruleset 20+
  histories.

The first isolated ruleset-20 qualification world (`00000000-0000-4000-8000-000000424246`)
reached tick 1,000 with 85 regional trace transitions. Every region from 0 through 7 was exercised;
all five projections reached sequence 1,018 and replay matched. A stop at tick 327 and exact resume
for the remaining 673 ticks produced the same verified chain. The backfill also exposed and fixed a
projector pool-starvation bug: no more than the four default pool connections are held concurrently,
and the fifth projection runs from its independent cursor afterward.

