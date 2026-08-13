# AACR GBM5K dependency acquisition

This runbook acquires the exact version-1 AACR Figshare record for Table S4 of
*Fitness Screens Map State-Specific Glioblastoma Stem Cell Vulnerabilities*.
The source is open under CC BY 4.0 and contains normalized Bayes Factor scores,
essentiality calls, and subtype scores from GBM5K and TKOv3 screens. It is
external qualification evidence, not Cancer World knowledge and not evidence of
clinical efficacy.

The checked-in trust root is
`data/cancer-research/aacr-gbm5k-dependency-source-v1.json`. It pins article
`28183566`, version `1`, dataset DOI `10.1158/0008-5472.28183566`, related paper
DOI `10.1158/0008-5472.CAN-23-4024`, and the CC BY 4.0 boundary. The manifest
does not guess the file ID, filename, exact length, or checksum. Those values
must be resolved together from the immutable-version API and frozen before raw
bytes are accepted.

## Acquire or verify

From the repository root:

```sh
cargo run --locked -p civilization-data -- source cancer-aacr-gbm5k-dependency --manifest data/cancer-research/aacr-gbm5k-dependency-source-v1.json --output-directory data/source-cache/aacr-gbm5k-dependency-v1
```

On a new acquisition, the command requires:

- the exact article ID, version, title, dataset DOI, related paper DOI, public
  dataset status, and CC BY 4.0 license;
- exactly one non-link XLSX with a safe filename and a bounded exact length;
- matching nonzero provider-supplied and provider-computed MD5 values; and
- an HTTPS Figshare download route bound to the exact discovered file ID.

The output directory contains the exact API response, the source workbook, and
`source-snapshot.json`. The snapshot binds the manifest hash, API-response hash,
file identity, exact length, MD5, SHA-256, and aggregate source-set hash.

Acquisition is create-only. A partial workbook can resume only when the server
honors the exact requested byte range and remaining length. An existing complete
snapshot is verified wholly offline; altered or missing bytes fail closed and are
never silently replaced.

## Custody boundary

Do not place the workbook or response-derived ranks in research prompts,
Hindsight, campaign selection, ordinary artifacts, or the tissue simulator. A
future qualification worker may query an exact gene symbol only after a research
artifact independently names that target. Its immutable result may then be shown
as observer-side evidence with provenance.

Keep GBM5K and TKOv3 columns distinct. Do not infer aliases, donor identities,
drug response, druggability, blood-brain-barrier penetration, safety, animal
efficacy, patient benefit, or a cure from this source. Production qualification
remains disabled until the acquired workbook's sheets and labels are inspected,
frozen, and covered by exact parsing tests.

## Offline tests

```sh
cargo test --locked -p civilization-data cancer_gbm5k::tests
```

The tests validate trust policy and transport invariants without downloading or
embedding the source workbook.
