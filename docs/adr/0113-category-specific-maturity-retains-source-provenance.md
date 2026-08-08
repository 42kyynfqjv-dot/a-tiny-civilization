# ADR 0113: Category-specific maturity retains source provenance

Date: 2026-08-08

Status: Accepted

## Context

The provisional body-profile plan previously assigned every member of a broad fauna class the
same engineering-assumption maturity age. The pinned Amniote Life-History profile set already
retains female- and male-maturity aggregates for some exact species identities, but the body plan
did not consume them. Treating those compiled aggregates as direct observations would also
overstate their evidence basis.

## Decision

Reproductive-physiology commitment schema two contains a canonically ordered maturity commitment
for every supported private birth category. Each entry carries its own tick duration, evidence
basis, source-profile-set digest, source-record identifier, and source-record digest.

Canonical body-plan derivation joins a maturity value only on exact catalog and taxon identifier,
exact category trait, positive integer days, and the expected unit. A retained Amniote aggregate is
classified as `literature_approximation`, never `source_measurement`. A category with no exact
retained value receives a separately addressable `engineering_assumption`; evidence for one
category never fills another. Initialization validates the complete commitment, the body-plan
artifact remains checksum-covered, and the world manifest retains the contributing source-profile
set digest.

The engine uses the exact category-specific value only for its private mechanical maturity check.
Organisms receive no reproductive labels or concepts, and public projections continue to omit
category, mechanism, partner, parentage, and physiological detail. Schema-one commitments retain
their previous bytes and coarse fallback behavior so archived histories remain replayable.

## Consequences

Available species/category life-history evidence now changes provisional reproductive timing
without being promoted beyond its source quality. Gaps remain visible and independently
replaceable. Development duration, recovery, opportunity frequency, and many taxa still use broad
engineering assumptions, so this change improves provenance but does not scientifically admit the
reproductive model.
