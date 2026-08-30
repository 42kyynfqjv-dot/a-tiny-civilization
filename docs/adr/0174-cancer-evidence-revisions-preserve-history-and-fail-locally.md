# ADR 0174: Cancer evidence revisions preserve history and fail locally

Date: 2026-08-30

Status: Accepted

## Context

The live research pipeline exposed four independent contract faults. Duplicate or
malformed Europe PMC rows could invalidate an entire novelty batch. Campaigns may
now run ten adversarial tests, while tissue admission still accepted at most five.
The admission scan selected old non-surviving syntheses before its bounded limit,
which could starve a later survivor. Finally, a generic experiment proposal could
pair diagnostic sensing with a treatment endpoint, producing a structurally valid
but inert virtual experiment.

These are observer-side research defects. They must not rewrite canonical Cancer
World history, completed model receipts, historical experiments, or prior audits.

## Decision

- Novelty method 2 validates each external match independently, ignores malformed
  external metadata, bounds query terms, and selects one duplicate source by a
  total deterministic order. Method-1 audits remain immutable and valid.
- The evidence worker requests method-2 audits for every eligible artifact. During
  that bounded backfill, observer views prefer the highest available method no
  newer than the current implementation and fall back to method 1, so a revision
  cannot make existing evidence disappear from the console.
- Virtual experiment plan schema 2 requires diagnostic sensing to use detection
  sensitivity and forbids treatment modalities from using that endpoint. Historical
  schema-1 plans remain readable and executable. New blind-discovery outputs are
  pinned to schema 2, and model-adapter version 18 records that normalization.
- Tissue protocols accept three through ten distinct campaign-result hashes. A
  root is eligible only when its current virtual result supports the prediction or
  is inconclusive; a root with an adverse result remains ineligible. Three later
  supporting tests with no falsification may promote an initially inconclusive
  root.
- Tissue admission filters for a survived synthesis before its bounded scan. Old
  falsified or inconclusive syntheses therefore remain history without occupying
  the finite admission window.

## Consequences

Recomputing novelty does not mutate or conceal method-1 evidence. One bad external
record can delay only itself, not the full backlog. Newly proposed experiments have
an endpoint that their selected virtual modality can actually measure. Escalated
campaign survivors reach the tissue tier with their complete immutable evidence,
while adverse roots and incomplete campaigns still fail closed.

Migration 0064 widens only the tissue evidence cardinality and replaces the insert
validator with the same explicit root allowlist. Event batches, world state,
Hindsight banks, completed provider receipts, and historical research artifacts are
unchanged.
