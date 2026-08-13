# PDC000711 HCMI GBM proteome acquisition

This runbook acquires one exact NCI Proteomic Data Commons (PDC) file and derives a
missingness-preserving 30-model glioblastoma slice. Raw and derived model-level
bytes stay in the ignored `data/source-cache/` and `data/derived-cache/` trees.
Do not commit either cache.

The trust root is
`data/cancer-research/pdc000711-hcmi-gbm-proteome-source-v1.json`. It pins:

- PDC study `PDC000711`, study-version UUID
  `ec0e442b-a0b8-4dc7-a4ba-6b5409fc68de`;
- file UUID `86e9b7f6-0776-4cb7-b761-dee14321b318`;
- `Global_all_original.txt`, 8,118,871 bytes, MD5
  `333eef379eaea258efca326d579eef21`;
- both GraphQL queries, expected study and file metadata, 75 biospecimens, and the
  exact 30 GBM case-submitter identifiers;
- PDC's CC BY 4.0 license evidence and required attribution.

## Acquire

Run from the repository root:

```sh
cargo run --locked -p civilization-data -- source cancer-pdc-hcmi-gbm \
  --manifest data/cancer-research/pdc000711-hcmi-gbm-proteome-source-v1.json \
  --output-directory data/source-cache/pdc000711-hcmi-proteome
```

The command posts the two pinned queries to `https://pdc.cancer.gov/graphql`.
It obtains a fresh signed URL, requires HTTPS, and never records that expiring URL.
It accepts the download only when the study ID, study-version UUID, file UUID,
name, location, category, type, format, byte length, and MD5 all match the trust
manifest. It sorts and canonicalizes the returned biospecimen records, then verifies
the exact GBM case set and sample-type counts.

The source directory contains:

- `Global_all_original.txt`: exact upstream bytes;
- `file-metadata.json`: stable verified PDC file metadata, with the signed URL
  removed;
- `biospecimens.json`: canonical full biospecimen join metadata;
- `source-snapshot.json`: SHA-256 and byte-length commitments for every retained
  input plus a domain-separated `source_set_sha256` content address.

If `source-snapshot.json` already exists, the command performs an entirely offline
verification and makes no network request. A partial run may reuse the source file
only if its exact pinned size and MD5 verify. It refuses to replace differing
metadata or a completed snapshot.

## Derive

```sh
cargo run --locked -p civilization-data -- derive cancer-pdc-hcmi-gbm-proteome \
  --manifest data/cancer-research/pdc000711-hcmi-gbm-proteome-source-v1.json \
  --source-directory data/source-cache/pdc000711-hcmi-proteome \
  --output-directory data/derived-cache/pdc000711-hcmi-gbm-proteome
```

The derivation re-verifies the complete source snapshot before reading the matrix.
It writes new files and refuses replacement:

- `pdc000711-gbm-proteome.tsv`: 30 GBM model columns in exact source order,
  followed by `T: Index`, `T: NumberPSM`, `T: ProteinID`, and `T: MaxPepProb`;
- `pdc000711-gbm-proteome.metadata.json`: artifact SHA-256/length, source
  commitments, dimensions and missing-cell counts, and a 30-entry provenance map
  from each derived column to its source column and PDC case/sample/aliquot IDs.

Every field is copied as text. Empty model cells stay empty, and numbers are not
parsed and reserialized. `T: Index` labels such as `1-Mar` and source
`T: ProteinID` strings are deliberately retained without inferred repair.

## Failure handling

- A metadata mismatch means upstream identity or cohort metadata changed. Retain
  the failed cache for investigation; do not weaken the manifest checks. A new
  source requires a reviewed new manifest version.
- An MD5 or size failure means the source bytes are incomplete or different. Move
  that exact failed file aside and rerun acquisition to resolve a new signed URL.
- A derivation shape failure means the row count, model-column count, annotation
  columns, or 30-case join differs from the pinned source contract. Do not impute
  or select columns by position to bypass it.
- If a partial derived directory contains only one output, move that exact directory
  aside and rerun. Outputs are intentionally create-only.

## Offline verification

```sh
cargo test --locked -p civilization-data cancer_pdc_hcmi
cargo clippy --locked -p civilization-data --all-targets -- -D warnings
```

The fixtures contain no live patient/model records or live signed URL. They exercise
the exact file response shape, a synthetic 30-GBM biospecimen join, deterministic
source-order projection, retained `T: Index`/`T: ProteinID`, and blank-cell
preservation without downloading the 8.1 MB live matrix.
