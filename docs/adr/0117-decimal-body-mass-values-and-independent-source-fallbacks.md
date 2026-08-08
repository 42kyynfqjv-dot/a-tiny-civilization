# ADR 0117: Retain decimal body masses and select across independent sources

Date: 2026-08-08

Status: Accepted

## Context

The EltonTraits compiler accepted `BodyMass-Value` only when Python's `isdigit()` returned true.
That silently discarded 7,589 decimal-valued bird rows and 3,658 decimal-valued mammal rows. The
v1 artifact consequently retained only integer-valued masses even though the pinned source files
contained exact fixed decimals.

The body-mass plan also accepted only one profile set. Choosing Amniote maximized coverage among
the then-available inputs, but prevented a world from using one independently compiled source and
falling back to another without inventing a merged dataset.

## Decision

EltonTraits v2 parses unsigned fixed decimals into an integer mantissa and explicit decimal scale.
It never passes source values through binary floating point. Invalid syntax, excessive precision,
and signed-64-bit overflow fail the compiler. The v2 artifact retains 174,070 canonical profiles,
including 13,662 positive adult-body-mass profiles.

Body-mass plan derivation accepts repeated independent profile-set arguments in explicit priority
order. For each planned species it selects the first canonical positive gram profile from the
first covering set. Every selection retains that set's digest and source-record identifier. The
body-profile builder requires every referenced source set and resolves each selection against its
own digest; it never merges or averages source evidence.

Canonical preparation pins EltonTraits v2 first and Amniote v1 as fallback. At the current origin,
that raises source-informed adult-mass coverage from five to 24 of 32 fauna species. The remaining
eight fauna species and Homo sapiens retain clearly labelled engineering assumptions.

## Consequences

Decimal source values are no longer lost, selection precedence is deterministic and inspectable,
and independent publications remain independent artifacts. Adult mass remains noncausal under
ADR 0115; a later allometric ruleset must separately specify and qualify any causal use.
