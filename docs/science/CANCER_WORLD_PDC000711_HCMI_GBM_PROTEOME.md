# Cancer World: PDC000711 patient-derived GBM proteome layer

Status: acquisition and derivation toolchain implemented against a frozen PDC source
manifest. No live model-level matrix or derived matrix is committed to the public
repository.

## Exact source boundary

The source is NCI Proteomic Data Commons study `PDC000711`, the HCMI
patient-derived cancer-model proteomics collection. This layer uses only the provider
alternate-pipeline file `Global_all_original.txt`:

- study-version UUID `ec0e442b-a0b8-4dc7-a4ba-6b5409fc68de`;
- file UUID `86e9b7f6-0776-4cb7-b761-dee14321b318`;
- 8,118,871 bytes;
- MD5 `333eef379eaea258efca326d579eef21`;
- PDC location `studies/711/suppl/Global_all_original.txt`.

The original matrix is intentionally selected instead of a completeness-filtered or
filled matrix. Its empty fields carry observation/missingness information. Filling
them would invent quantitative evidence and would make protein-detection claims
unreproducible from the source.

The checked-in source manifest also freezes the PDC file and biospecimen GraphQL
queries. Acquisition resolves a fresh expiring signed URL, but the signed URL is
transport only and is excluded from retained canonical metadata.

## GBM cohort and join

PDC biospecimen metadata reports 75 model records for the study. The declared
selection is the exact set where `disease_type == "Glioblastoma"` and
`primary_site == "Brain"`: 30 models, comprising 27 next-generation cancer models
and three expanded next-generation cancer models.

Each matrix header is joined exactly to PDC `case_submitter_id`. Derivation fails if
the selected set changes, a selected header is absent or duplicated, a selected case
has more than one join record, or any study/file identity changes. Derived columns
retain matrix source order rather than API response order. The metadata artifact
records source and derived indexes plus case, sample, and aliquot provenance for all
30 columns.

## Missingness-preserving artifact

The derived TSV has 12,342 protein rows and 34 columns:

- 30 selected model abundance columns;
- `T: Index`;
- `T: NumberPSM`;
- `T: ProteinID`;
- `T: MaxPepProb`.

No numeric field is parsed or reformatted. An empty source field remains an empty TSV
field, never zero. The content-addressed metadata records observed and missing model
cell counts calculated from the exact 30-column artifact.

`T: Index` is retained as the provider's text label, not asserted to be a clean gene
symbol. The source includes spreadsheet-like labels such as `1-Mar`, and at least one
such label is not unique. `T: ProteinID` is retained alongside it so downstream work
can use explicit source accessions. This toolchain performs no fuzzy gene aliasing,
date-label repair, protein-group expansion, or accession-to-gene inference.

## Scientific use and limits

PDC describes the isobaric abundance values as relative rather than absolute
quantities. This layer can support auditable statements about observed protein
coverage and relative patterns across these patient-derived GBM models. It does not
contain perturbation response, clinical outcome, exposure, toxicity, dose, safety,
or counterfactual treatment evidence. A high value or broad coverage is not evidence
that targeting a protein will help a patient.

These are patient-derived models, not the original tumors or a representative
clinical cohort. Culture, model expansion, sampling, provider processing, and
protein-group ambiguity remain material limitations. Missingness may reflect assay
and processing effects as well as biology.

## License and attribution

PDC publishes its data under Creative Commons Attribution 4.0. Retained metadata
uses the attribution: “Proteomic Data Commons (PDC), National Cancer Institute,
study PDC000711; licensed under CC BY 4.0.” The license permits sharing and adaptation,
including commercial use, subject to attribution and the other CC BY 4.0 terms.

## Reproduce

```sh
cargo run --locked -p civilization-data -- source cancer-pdc-hcmi-gbm \
  --manifest data/cancer-research/pdc000711-hcmi-gbm-proteome-source-v1.json \
  --output-directory data/source-cache/pdc000711-hcmi-proteome

cargo run --locked -p civilization-data -- derive cancer-pdc-hcmi-gbm-proteome \
  --manifest data/cancer-research/pdc000711-hcmi-gbm-proteome-source-v1.json \
  --source-directory data/source-cache/pdc000711-hcmi-proteome \
  --output-directory data/derived-cache/pdc000711-hcmi-gbm-proteome
```

Operational recovery and artifact schemas are documented in
`docs/operations/PDC000711_HCMI_GBM_PROTEOME.md`.

Primary sources:

- PDC study: <https://pdc.cancer.gov/pdc/study/PDC000711>
- PDC API documentation: <https://pdc.cancer.gov/pdc-docs/api-documentation>
- PDC license and data-use page: <https://pdc.cancer.gov/pdc-docs/home>
- HCMI program: <https://www.cancer.gov/ccg/research/functional-genomics/hcmi>
- CC BY 4.0: <https://creativecommons.org/licenses/by/4.0/>
