# ADR 0085: Provisional life history is denominated in simulation time

## Status

Accepted on 2026-08-08.

## Context

The first ruleset-18 qualification world exposed a serious placeholder leak. Every organism used
the same values—maturity after one tick and development after twelve ticks—so a person was born at
tick 34, only 2 hours and 50 minutes after genesis in a five-minute-tick world. The implementation
was deterministic and replayable, but the committed timing was not credible.

## Decision

Provisional body-profile generation expresses life-history guardrails in simulation seconds and
converts them to ticks with ceiling division. Tick duration can therefore change without silently
changing the represented biological duration.

`Homo sapiens` has a separate provisional guardrail: founders begin at 20 years, maturity is 15
years, development is 280 days, recovery is 365 days, and opportunities are spaced 28 days apart
with a 200,000-per-million deterministic initiation threshold. Fauna use coarser guardrails keyed
by the source range package (insects, arachnids, birds, mammals, reptiles, and a shared aquatic or
other-animal fallback). No fauna guardrail permits minute- or hour-scale development.

These values remain explicitly marked `engineering_assumption`. They prevent obviously invalid
pacing; they do not claim species-level scientific admission. A later cited life-history pipeline
must replace them per species without rewriting worlds that already pinned version two.

## Verification

The same seed and 32-taxon selection were regenerated after the change. The corrected disposable
world advanced through tick 1,271 (4.4 simulation days), committed 1,294 batches and 548,210 events,
created 22 snapshots, replayed to the committed state hash, and projected with zero post-genesis
births. The predecessor qualification world had produced one person and three animal births by
tick 511.

## Consequences

- The public observer remains free of reproductive mechanism detail; it receives only safe birth
  facts when a correctly timed birth eventually commits.
- Genesis artifacts visibly disclose coarse assumptions rather than presenting them as measured
  species traits.
- Changing these guardrails changes the body-profile artifact digest and therefore requires a new
  world; existing history remains replayable.
