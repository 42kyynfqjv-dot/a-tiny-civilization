# ADR 0153: Cancer evidence is qualified by intended use, not a realism percentage

Status: accepted for Cancer World source admission.

## Context

Cancer World needs to produce research packages that an independent laboratory can
evaluate, not merely plausible prose or visually convincing virtual tumors. Rich
glioblastoma evidence exists, including cross-sectional multi-omics, longitudinal
primary/recurrent pairs, spatial and single-cell atlases, patient-derived models,
and some material obtained through rapid autopsy. These sources measure different
parts of different tumors under different collection and assay processes.

No source observes every cell and molecular state of a complete living tumor over
time. Destructive tissue assays cannot repeatedly measure the same region, and an
observed patient history does not contain the outcome of alternative treatments
the patient did not receive. Therefore “95% simulation of a real tumor” has no
single denominator and can be made deceptively high by fitting measurements the
model already saw.

## Decision

The versioned registry at
`data/cancer-research/gbm-dataset-registry-v1.json` is the sole initial source-
candidate inventory. Registry membership is not admission. Every source begins as
`registry_only_terms_and_artifacts_unverified`; exact releases, files, terms,
checksums, and transformations must be frozen before any parameter is described as
source-calibrated.

Patient-level bytes and controlled-access credentials never enter the public
repository. Controlled data require documented authorization. Missing numeric
parameters are rejected rather than supplied by a language model. Cohort splitting
is by patient, never by sample, section, cell, or assay, because those smaller units
can leak one person's tumor into both calibration and validation.

Cancer World uses intended-use qualification. A candidate may be called ready for
independent experimental falsification only when one frozen evidence package shows:

1. reproduction of declared untreated or standard-care baselines;
2. patient-disjoint calibration and held-out validation;
3. generalization across at least two independent datasets;
4. comparison against declared strong simple and published baselines;
5. uncertainty, parameter-sensitivity, and failure analysis;
6. content-addressed code, data manifests, transformations, and results; and
7. a feasible external experiment with controls and a result that would reject the
   hypothesis.

Passing that gate does not establish safety, efficacy, or a cure. It produces a
serious, reproducible request for outside validation. A single aggregate “tumor
realism percentage” is prohibited. Narrow metrics are allowed only with their
endpoint, cohort, split, baseline, confidence interval, and intended use attached.

## Initial source axes

- TCGA-GBM supplies broad retrospective molecular population structure.
- GLASS supplies primary/recurrent longitudinal evolution.
- Ivy GAP and the PRJNA1337938 spatial atlas supply anatomically resolved and
  single-cell/spatial structure.
- CPTAC high-grade glioma supplies proteomic, phosphoproteomic, metabolomic, and
  lipidomic layers alongside genomic and transcriptomic measurements.
- NCI PDMR supplies characterized patient-derived model routes, including rapid
  autopsy among its acquisition sources, for later model qualification.

The axes remain separate until a documented join proves compatible patient,
specimen, diagnosis, assay, unit, and time semantics. The normalizer cannot create a
fictional “complete patient” by combining measurements from unrelated donors.

The first real-data vertical slice is recorded in
[`docs/science/CANCER_WORLD_TCGA_GBM_BASELINE.md`](../science/CANCER_WORLD_TCGA_GBM_BASELINE.md).
It establishes an aggregate patient-disjoint TCGA-GBM molecular baseline. It does
not advance the virtual lab to intervention-response qualification.

## Verification

- The committed registry parses and has a stable content digest.
- Sources are uniquely ordered and include spatial, longitudinal, and rapid-autopsy
  evidence axes.
- Setting any registry source to support counterfactual treatment claims fails.
- Permitting patient data in the public repository fails.
- Changing the split unit from patient to sample fails.
- Removing any qualification requirement or allowing invented numeric parameters
  fails.

## References

- Ivy Glioblastoma Atlas Project: <https://gbm.brain-map.org/static/home>
- GLASS Data Resource: <https://glass-consortium.org/datasets/>
- TCGA-GBM publication data: <https://gdc.cancer.gov/about-data/publications/gbm_2013>
- CPTAC high-grade glioma publication data:
  <https://gdc.cancer.gov/about-data/publications/CPTAC-3_2024_1>
- NCI PDMR model information:
  <https://dctd.cancer.gov/drug-discovery-development/reagents-materials/pdmr/models>
- Spatial GBM atlas data availability: <https://pmc.ncbi.nlm.nih.gov/articles/PMC13031279/>
- GDC open and controlled access policy:
  <https://gdc.cancer.gov/access-data/data-access-processes-and-tools>
