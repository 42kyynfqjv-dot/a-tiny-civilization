# Cancer World: NCI-60 and ALMANAC response baseline

Status: derived from National Cancer Institute CellMiner database 2.15, export
dated 2025-09-17.

This is Cancer World's first end-to-end real intervention-response slice. It adds
single-agent activity and drug-combination interaction measurements from the six
explicitly labeled NCI-60 CNS cell lines. It does **not** turn Cancer World into a
clinical model: these are long-established two-dimensional cell lines, not patients,
organoids, xenografts, immune-competent tumors, or clinical outcomes.

## Frozen source

The acquisition command retains two compact normalized workbooks from the official
NCI CellMiner host:

| Artifact | ZIP bytes | SHA-256 |
| --- | ---: | --- |
| NCI-60 average activity z scores | 8,514,360 | `17e6a62597a32caa5d43d2e8f81422c108b47bcefd51a3b2269834001da1d7aa` |
| ALMANAC ComboScores | 1,653,406 | `161a0801e5a34c1fd1a3ae5b3c743d0b481d979b1c61fb08c75031b980233a0a` |

The source-set commitment is
`d8015cbc0ed8e2fce6be466a0725d1762b5e9276dc830187049776326324da14`.
The raw workbooks remain in the ignored source cache. The committed aggregate names
observed public drugs and cell lines, but does not include the full response matrices.

NCI says its text may be reused with NCI credited as the source. This project also
links to the original products, does not use the NCI logo, and treats the data as
hypothesis-generating evidence rather than NCI endorsement.

## What was normalized

The single-agent workbook contains 25,731 NSC compounds and 147,914 observed CNS
responses. CellMiner supplies a known mechanism for 1,116 records and marks 326
records as FDA approved. Its response value is an average activity z score after
CellMiner quality control; a higher value means greater relative sensitivity in the
NCI-60 pattern. It is not a dose or probability of response.

The ALMANAC workbook contains 5,355 source records representing 5,242 canonical NSC
pairs and 30,589 observed CNS ComboScores. Some pairs occur in both source orders or
otherwise have more than one record: 113 records repeat a canonical pair and can have
materially different values. The normalizer preserves this count and uses the median
of the available records for each pair and cell line. It does not silently keep the
first or last row.

Of the normalized CNS combination observations, 9,326 have positive scores. NCI's
documentation says a higher ComboScore generally means the combination inhibited
growth more effectively than expected from the component drugs tested separately.
That is an in-vitro interaction signal, not evidence of safety or clinical benefit.

## Leakage-resistant checks

The split unit is the whole compound for NCI-60 and the whole canonical drug pair for
ALMANAC. A fixed SHA-256 rule assigns four buckets to calibration and one to held-out
assessment. No held-out response enters a calibration median.

For single agents, a deliberately simple predictor takes the median calibration
response for the same declared mechanism and CNS line, requiring at least three
calibration compounds. It covers 758 of 1,297 eligible held-out observations across
128 compounds. Its mean absolute error is 0.493 z-score units, compared with 0.638
for the cell-line-only median baseline—a 22.7% reduction.

For combinations, the predictor takes the median calibration ComboScore for the same
canonical mechanism pair and CNS line, again requiring at least three calibration
pairs. It covers 3,187 of 6,222 eligible held-out observations across 551 pairs. Its
mean absolute error is 40.172 score units, compared with 44.651 for a zero-interaction
baseline and 41.316 for a cell-line-only median. That is only a modest improvement.
The honest result is useful: mechanism labels alone leave most combination behavior
unexplained, so Cancer World must earn progress against real held-out measurements.

## Reproduce

```sh
cargo run -p civilization-data -- source cancer-nci-cellminer \
  --output-directory data/source-cache/nci-cellminer-2026-08-12

cargo run -p civilization-data -- derive cancer-nci-cellminer-baseline \
  --source-directory data/source-cache/nci-cellminer-2026-08-12 \
  --registry data/cancer-research/gbm-dataset-registry-v1.json \
  --output data/cancer-research/nci-cellminer-2-15-cns-response-baseline-v1.json
```

Acquisition resumes from already retained exact ZIPs, writes an immutable manifest,
and verifies workbook version metadata. Derivation rechecks byte lengths, hashes,
inner workbook paths, export metadata, source-registry membership, repeated-pair
handling, and all split commitments before writing a new aggregate.

## Qualification boundary

This slice makes Cancer World falsifiable against real perturbation data. It does not
make a simulated candidate a treatment. A promising result must still survive full
dose-response review, independent cell lines, patient-derived organoids or cultures,
xenografts where appropriate, toxicity and exposure work, and independent wet-lab
falsification before any clinical claim is possible.

Primary sources:

- NCI CellMiner dataset downloads:
  <https://discover.nci.nih.gov/cellminer/loadDownload.do>
- CellMiner NCI-60 activity documentation:
  <https://discover.nci.nih.gov/cellminer/html/drug_zscore.html>
- CellMiner ALMANAC ComboScore documentation:
  <https://discover.nci.nih.gov/cellminer/html/drug_almanac_combo_score.html>
- NCI-ALMANAC study results: <https://dtp.cancer.gov/ncialmanac/initializePage.do>
- NCI information reuse policy: <https://www.cancer.gov/policies/copyright-reuse>
