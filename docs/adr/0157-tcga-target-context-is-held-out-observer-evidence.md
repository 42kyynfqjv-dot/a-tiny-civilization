# ADR 0157: TCGA target context is held-out observer evidence

Status: accepted for Cancer World.

## Context

Cancer World artifacts can name a small number of exact central molecular targets.
The PDC000711 layer can determine whether proteins with those exact source labels
were observed across 30 patient-derived GBM models. That does not say whether a
target is recurrently altered in patient tumors.

The checked-in TCGA-GBM DR46 aggregate was derived from 374 molecularly profiled
patients with a patient-level split made before use: 303 profiled calibration
patients and 71 profiled held-out patients. Its simple baseline selected 25
protein-altering genes using calibration patients only and then measured their
prevalence in the held-out patients. It contains no patient identifier or
patient-level row.

## Decision

An observer-side qualifier may attach this narrow prevalence context to new
artifacts by exact gene symbol. For a target in the calibration-selected feature
set, it records calibration prevalence, held-out prevalence, and their absolute
difference. For every other target it records
`outside_calibration_feature_set`; it never converts missing aggregate coverage
into biological absence.

The qualifier pins the exact aggregate bytes, GDC release and API identity,
patient-set commitments, cohort sizes, feature-selection rule, and all 25
predictions. The result is immutable and bound to the world, request,
contribution hash, source hash, and method version.

This layer remains outside research prompts, research memory, and campaign
outcomes. It is retrospective observational somatic-variant context only. A
protein-altering variant is not expression, dependency, druggability, treatment
response, efficacy, safety, or clinical benefit. The console must present it
separately from patient-derived protein coverage and intervention-response
qualification.

## Consequences

- Recurrently altered artifact targets such as `EGFR`, `PTEN`, and `TP53` can be
  recognized against real patient-disjoint data without leaking patient rows.
- A target outside the 25 calibration-selected genes remains unresolved rather
  than receiving a fabricated zero.
- The held-out values may assess prevalence stability but cannot be used as
  model-generated evidence or promote a candidate.
- Expanding beyond this feature set requires a new predeclared aggregate and
  method version; silently inspecting held-out patients to choose more genes is
  prohibited.

## Verification

- Any byte change to the frozen aggregate fails its content hash.
- Duplicate genes, changed split identity, changed cohort counts, inconsistent
  prevalence error, or a changed intended-use statement fails qualification.
- Exact-target tests distinguish an evaluated target from one outside the
  calibration feature set.
- Public wording states that this is mutation-prevalence context and not
  intervention or clinical evidence.
