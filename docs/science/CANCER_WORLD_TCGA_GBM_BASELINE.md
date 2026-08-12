# Cancer World: first real TCGA-GBM baseline

Status: derived from open NCI GDC Data Release 46.0 (2026-08-10).

This is Cancer World's first end-to-end real patient-data slice. It is deliberately
small in scope: population descriptors and open masked somatic mutations. It is not
a tumor simulator, treatment-response dataset, survival model, or clinical result.

## What ran

The data tool acquired the complete open `TCGA-GBM` case response and every open WXS
masked somatic-mutation MAF returned by the pinned query:

- 617 project cases;
- 464 gzip MAF artifacts, each checked against GDC byte length and MD5;
- GDC API release `46.0`, API tag `8.5.0`, and API commit
  `8f7c2a51ab0084b216ad1b62a3fae8b945439c53`;
- exact response and aggregate source-file commitments in the derived artifact.

Raw patient-level responses and MAF files remain in the ignored local source cache.
They are not committed to the public repository. The committed result contains only
aggregate measurements and commitments to the patient sets.

## The split was made before use

Each GDC case UUID is assigned by a fixed SHA-256 rule. Four buckets enter
calibration and one bucket is held out. The current result is:

- 492 calibration patients, 303 with open mutation profiles;
- 125 held-out patients, 71 with open mutation profiles.

No sample, aliquot, mutation, or tissue section can move a patient across that
boundary. Multiple MAF specimens for a patient are unioned before prevalence is
calculated, so a patient with multiple samples is not counted as multiple people.

## Observed baseline

Across the 374 molecularly profiled patients, the normalizer retained 35,885 unique
patient-level protein-altering variants. The median profiled patient has 43. The
most prevalent genes include PTEN, TP53, EGFR, NF1, PIK3CA, ATRX, PIK3R1, and RB1.
These names and frequencies were derived from the frozen source bytes; they were not
inserted as expected Cancer World answers.

As an intentionally simple out-of-sample benchmark, the calibration cohort's 25
most prevalent genes predict the held-out cohort using calibration prevalence alone.
Its mean absolute prevalence error is 28,189 parts per million (2.819 percentage
points), and its mean binary Brier score is 82,063 parts per million. A future
Cancer World genomic model must improve on that frozen baseline without selecting
features from the held-out patients. Beating it would demonstrate added predictive
information, not treatment efficacy.

The clinical slice contains 617 patients, 492 recorded deaths, and age-at-diagnosis
data for 596. The reported 382-day median is specifically the median recorded
days-to-death among observed deaths. It is not a Kaplan-Meier estimate and must not
be presented as population survival. Alive follow-up coverage in this API slice is
too incomplete for survival qualification.

## Reproduce

```sh
cargo run -p civilization-data -- source cancer-tcga-gbm \
  --output-directory data/source-cache/tcga-gbm-dr46-open-2026-08-12

cargo run -p civilization-data -- derive cancer-tcga-gbm-baseline \
  --source-directory data/source-cache/tcga-gbm-dr46-open-2026-08-12 \
  --registry data/cancer-research/gbm-dataset-registry-v1.json \
  --output data/cancer-research/tcga-gbm-dr46-patient-baseline-v1.json
```

The acquisition command is resumable after transport failure and refuses to replace
a completed acquisition. The derivation verifies every source artifact again before
reading it.

## What this achieves—and what it does not

Cancer World now has a real, held-out molecular baseline it can fail against. A
future calibrated model must reproduce calibration-cohort distributions without
reading the held-out cohort, then be scored against the held-out distributions.

This dataset cannot qualify an intervention because it does not contain controlled
counterfactual response. The next useful evidence slice is longitudinal recurrence
or patient-derived-model perturbation response. Adding more scaffolding before one
of those slices runs is explicitly deprioritized.

Primary sources:

- NCI GDC TCGA-GBM: <https://gdc.cancer.gov/about-data/publications/gbm_2013>
- GDC API search and retrieval:
  <https://docs.gdc.cancer.gov/API/Users_Guide/Search_and_Retrieval/>
- GDC open/controlled access policy:
  <https://gdc.cancer.gov/access-data/data-access-processes-and-tools>
