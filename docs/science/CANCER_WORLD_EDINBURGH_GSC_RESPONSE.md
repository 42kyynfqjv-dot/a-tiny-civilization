# Cancer World: Edinburgh patient-derived GSC perturbation response

Status: an exact-source acquisition contract and a leakage-resistant evaluation
boundary are defined. No raw DataShare artifact or response-derived model has been
downloaded or committed as part of this repository change.

## Evidence boundary

The source is University of Edinburgh DataShare deposit DOI `10.7488/ds/8038`,
handle `10283/9113`, item UUID `2bbcb530-4bd8-45d8-b0da-f1681352862a`:

> High Content Drug Screening & Analysis Dataset: A comprehensive pharmacological
> survey across heterogeneous patient-derived glioblastoma stem cell models

The deposit describes a 384-well high-content Cell Painting screen over six
patient-derived glioblastoma stem-cell (GSC) lines and 3,866 compounds. It reports
211 primary hits and a 164-compound validation collection. CellProfiler extracts
approximately 1,006 features per cell from six images per well. These counts define
the dataset's experimental scope; they do not make a screen hit a treatment.

The v1 acquisition is deliberately narrower than the whole deposit. It selects the
dose-response hit-validation archive and five provenance/QC artifacts:

| Artifact | Exact bitstream UUID | Pinned MD5 |
| --- | --- | --- |
| `1.README_Repository_HTS_Barcoding_QC_byPlate.txt` | `af9a7cae-5383-4f49-86f8-eb3bd23b203f` | `b083f841429222de064e817eba6903fe` |
| `7.Hit_Validation_HighContent.zip` | `7f7e626e-99fc-4f9d-9a89-19191943703d` | `bf5784ff589f523d4e4679aeba19f275` |
| `8.QC_Normalised_Distributions.xlsx` | `a98b30f8-f319-49fc-91bc-d25a01062d7a` | `b237b46fc85d70767bb79137f62944ed` |
| `GCGR_CellPainting_3.1.5.cpproj` | `f4df60a5-9a31-4c0e-b193-95de65c5c8a1` | `2fac59f04b778d89a63f4319f26c5a9a` |
| `AUC_script_Figure3B.Rmd` | `f19a89df-a909-42cf-8f4e-6ca0b60dcbb1` | `afb6486a960eb6bdb376f2cdfe874f56` |
| `license_text` | `209e6e37-ce06-48f0-8a06-0a411938a3ac` | `2946b37a07baeba80cea628909e28cae` |

The public record showed rounded display sizes during offline review, so the
manifest does not guess exact byte lengths. Discovery must resolve the positive,
bounded `sizeBytes` value from each exact bitstream-UUID API response and bind it
into the create-only discovery record and source snapshot. Acquisition fails closed
if that exact size, UUID, filename, MD5 algorithm/value, or HTTPS content link is
missing or differs; downloaded bytes must then match both the resolved size and the
pinned MD5. The checked-in test responses use synthetic small sizes to exercise the
DSpace 7 schema and are not evidence of authoritative upstream byte lengths.

## Evaluation firewall

The fixed release gate is disjoint at the patient-derived GSC-line boundary:

- training: `E13`, `E21`, `E28`, and `E31`;
- calibration and threshold selection: `E34` only;
- untouched final assessment: `E57` only.

No image-level, cell-level, or well-level random split is admissible. Every image,
well, dose, technical replicate, plate repeat, and derived feature for a biological
line-compound condition remains in one partition. A second generalization analysis
must hold out whole canonical compounds or chemical scaffolds across all doses,
plates, libraries, and GSC lines; compound identities are deduplicated across
libraries before split assignment.

All transformations are learned from training data only. That includes learned
normalization, feature selection, redundancy removal, imputation, representation
learning, hyperparameter fitting, and response thresholds. Predeclared plate-QC
rules are applied before modeling and may not be changed after held-out responses
are read. Technical control values may support within-plate correction, but held-out
response labels may never influence fitted parameters.

Observed calibration/final responses, outcome-derived ranks, and scoring summaries
form a held-out answer key. Keep that key in the qualification worker, separate from
candidate prompts, retrieval, memory, training artifacts, and model context. The
final `E57` partition is opened only once the candidate and scoring procedure are
frozen. Outer leave-one-GSC-line-out reporting may supplement this fixed gate, with
tuning nested wholly inside every outer training fold; it does not replace the
untouched final gate.

## What this evidence can establish

This source can calibrate or falsify narrow in-vitro predictions of compound-linked
cell survival, multiparametric morphology, phenotypic distance, and dose-response
AUC in the declared six-line GSC panel. The validation archive and accompanying QC
and analysis materials improve experimental traceability; they do not supply a
clinical counterfactual.

Six cell lines cannot estimate population benefit. Two-dimensional adherent Cell
Painting omits clinical exposure, blood-brain-barrier penetration, therapeutic
index, immune and tumor-microenvironment effects, organ-level toxicity, and patient
heterogeneity. Morphology or cell loss may reflect nonspecific toxicity. Results
therefore cannot establish patient efficacy, safety, animal efficacy, a treatment
recommendation, or a cure.

## License and attribution

The deposit declares Creative Commons Attribution 4.0 International. Retained
attribution is: “Elliott, Richard; Carragher, Neil. (2025). High Content Drug
Screening & Analysis Dataset: A comprehensive pharmacological survey across
heterogeneous patient-derived GBM stem cell models. University of Edinburgh
DataShare. https://doi.org/10.7488/ds/8038. Licensed under CC BY 4.0.” CC BY 4.0
permits reuse and adaptation, including commercial reuse, subject to attribution,
license notice, and indication of changes; it does not imply endorsement.

## Acquire

Run from the repository root when network acquisition is intentionally authorized:

```sh
cargo run --locked -p civilization-data -- source cancer-edinburgh-gsc-response --manifest data/cancer-research/edinburgh-ds8038-gsc-response-source-v1.json --output-directory data/source-cache/edinburgh-ds8038-gsc-response
```

The ignored source cache is create-only. A completed valid source snapshot makes a
rerun an offline verification: it rechecks the manifest binding and every retained
artifact without contacting DataShare. Recovery from an incomplete attempt may
reuse only exact artifacts that pass their resolved byte-length and pinned-MD5
checks; differing files or metadata are never silently replaced. See
`docs/operations/EDINBURGH_DS8038_GSC_RESPONSE.md` for the operational procedure.

Primary source locators:

- DataShare record: <https://datashare.ed.ac.uk/handle/10283/9113>
- DOI: <https://doi.org/10.7488/ds/8038>
- DSpace item API: <https://datashare.ed.ac.uk/server/api/core/items/2bbcb530-4bd8-45d8-b0da-f1681352862a>
- CC BY 4.0: <https://creativecommons.org/licenses/by/4.0/>
