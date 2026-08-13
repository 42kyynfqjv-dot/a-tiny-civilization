# ADR 0156: Patient-derived evidence is an observer-side exact-target check

Status: accepted for Cancer World.

## Context

Cancer World can now name a small number of central molecular targets in a new
research artifact. NCI PDC study `PDC000711` provides a real proteomic matrix for
30 patient-derived glioblastoma models, but it does not provide intervention
response, safety, or clinical-outcome labels. Letting the research model inspect
the matrix would leak evidence into hypothesis generation and make later
corroboration circular.

The source `T: Index` column also contains ambiguous and spreadsheet-corrupted
labels. Silently repairing those labels would add an unrecorded biological
inference to what should be a narrow evidence check.

## Decision

New research contributions may return at most four sorted, unique, uppercase gene
symbols that are explicit central subjects of the artifact. Target identity alone
does not count as evidence.

An isolated observer-side evidence worker may then look up those exact symbols in
the frozen, content-addressed PDC-derived matrix. It records, per target, the exact
protein accessions, the number of cohort models assayed, the number with an
observed value, and one of `observed`, `not_detected`, or `unresolved`. It performs
no alias expansion, fuzzy matching, spelling correction, gene-family expansion,
spreadsheet-label repair, or missing-value imputation.

The qualification is immutable and bound to the world, request, contribution
hash, source bytes, method version, and exact cohort. The public console labels it
as molecular presence only. It cannot change research history, campaign outcomes,
or model memory, and it cannot establish mechanism, treatment response, efficacy,
safety, clinical benefit, or a cure.

The raw and derived matrix bytes remain ignored by Git and inaccessible to the
research worker. Only the evidence worker receives two read-only,
content-addressed paths after startup verification. Public APIs expose the bounded
qualification result, not patient/model-level matrix values.

## Consequences

- Hypothesis generation remains separated from real-data checking.
- Exact observed coverage can prioritize independent follow-up without being
  misrepresented as intervention evidence.
- Ambiguous symbols remain honestly unresolved until a separately reviewed,
  versioned identifier-normalization method exists.
- Moving beyond target presence requires a new qualified evidence layer with an
  endpoint-appropriate held-out design; this ADR does not authorize that claim.

## Verification

- Historical contribution schemas serialize identically and cannot contain
  molecular targets.
- The qualifier rejects changed source identity, hashes, dimensions, joins,
  missingness accounting, or transformation policy.
- The real ignored artifact validates against its exact recorded hashes and the
  `1-Mar`/`MARCH1` boundary remains unresolved without inference.
- Database triggers reject cross-world, partial, mutable, or checksum-inconsistent
  qualifications.
- Production boundary tests prove the research worker cannot see the PDC files
  and the evidence worker can read them only through verified read-only mounts.
