# ADR 0139: ruleset 32 pins multiple profile source sets

Status: accepted

## Context

One canonical organism profile plan can legitimately select adult-body-mass or life-history rows
from more than one retained source compilation. The complete ruleset-32 plan does so: its species
coverage combines independently retained datasets and explicitly marked engineering fallbacks.

The runner previously reduced each evidence class to at most one non-assumption profile-set digest.
That restriction was compatible with earlier genesis inputs but rejected the complete v24 plan even
though every selected row already carried its own source-set, record, and record-content digest.
Rewriting all rows to pretend they came from one set would make provenance less accurate.

## Decision

Ruleset 32 and later accept all distinct non-assumption source-set digests in canonical sorted
order. Genesis pins every digest in `WorldManifest.scientific_datasets`; when an evidence class has
multiple sets, stable zero-padded keys distinguish them. The body-profile plan digest continues to
bind the exact species-to-row selections and engineering assumptions.

Rulesets before 32 retain the single-source-set restriction and legacy manifest key. This preserves
their immutable genesis reconstruction behavior.

## Consequences

- Canonical v24 can retain exact multi-dataset provenance without collapsing or relabeling sources.
- Adding, removing, or changing any selected source set changes the ruleset-32 manifest and genesis
  hash.
- Replays of older rulesets keep their original construction contract.
- The database-free canonical proof and its regression test enforce this boundary before persistent
  initialization.
