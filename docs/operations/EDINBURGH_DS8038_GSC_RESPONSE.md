# Edinburgh DS8038 GSC response acquisition

This runbook acquires the six-artifact v1 slice of University of Edinburgh
DataShare DOI `10.7488/ds/8038` into an ignored local source cache. No raw deposit
bytes are committed. The trust root is
`data/cancer-research/edinburgh-ds8038-gsc-response-source-v1.json`.

## Pinned boundary

The manifest fixes handle `10283/9113`, item UUID
`2bbcb530-4bd8-45d8-b0da-f1681352862a`, CC BY 4.0 evidence, and these exact DSpace
bitstreams:

| Role | File | UUID | MD5 |
| --- | --- | --- | --- |
| plate metadata and QC flags | `1.README_Repository_HTS_Barcoding_QC_byPlate.txt` | `af9a7cae-5383-4f49-86f8-eb3bd23b203f` | `b083f841429222de064e817eba6903fe` |
| dose-response validation | `7.Hit_Validation_HighContent.zip` | `7f7e626e-99fc-4f9d-9a89-19191943703d` | `bf5784ff589f523d4e4679aeba19f275` |
| normalization and plate QC | `8.QC_Normalised_Distributions.xlsx` | `a98b30f8-f319-49fc-91bc-d25a01062d7a` | `b237b46fc85d70767bb79137f62944ed` |
| CellProfiler pipeline | `GCGR_CellPainting_3.1.5.cpproj` | `f4df60a5-9a31-4c0e-b193-95de65c5c8a1` | `2fac59f04b778d89a63f4319f26c5a9a` |
| AUC analysis source | `AUC_script_Figure3B.Rmd` | `f19a89df-a909-42cf-8f4e-6ca0b60dcbb1` | `afb6486a960eb6bdb376f2cdfe874f56` |
| license evidence | `license_text` | `209e6e37-ce06-48f0-8a06-0a411938a3ac` | `2946b37a07baeba80cea628909e28cae` |

Exact upstream byte lengths were unavailable in the offline source review. Rounded
DataShare display sizes are not converted or treated as byte commitments. The
command must resolve `sizeBytes` independently from each exact UUID API endpoint,
require it to be a positive integer no greater than that artifact's manifest bound,
and freeze the resolved value before downloading content. It fails closed rather
than guessing when `sizeBytes` is absent, zero, malformed, or out of bounds.

## Acquire

From the repository root, run:

```sh
cargo run --locked -p civilization-data -- source cancer-edinburgh-gsc-response --manifest data/cancer-research/edinburgh-ds8038-gsc-response-source-v1.json --output-directory data/source-cache/edinburgh-ds8038-gsc-response
```

For a new cache the command verifies the exact item API response first, including
UUID, DOI, handle, title, and rights. It then requests each manifest-selected
bitstream by exact UUID. Every response must have the expected UUID and filename,
`MD5` checksum algorithm and value, positive bounded `sizeBytes`, and the exact
HTTPS content URL for that UUID. Redirects or identity changes must not broaden the
allowed DataShare origin.

Only after discovery passes does content acquisition begin. A retained artifact is
accepted only when its byte count equals the just-frozen `sizeBytes` and its MD5
equals the manifest value. Stable discovery metadata excludes transient transport
details. The completed source snapshot binds the trust manifest, verified item and
bitstream metadata, resolved exact byte lengths, and cryptographic commitments to
all six retained files.

The output directory contains:

- `discovery.json`: canonical frozen item metadata plus the six verified bitstream
  records and their resolved exact `sizeBytes` values;
- the six raw artifacts under their exact provider filenames;
- transient `<filename>.part` files while a raw artifact is incomplete;
- `source-snapshot.json`: the completed canonical/digest binding for discovery and
  all six accepted artifacts.

## Create-only and restart behavior

The output directory is create-only: the command must not overwrite a completed
snapshot, accepted source bytes, or stable metadata with different content.

- If `discovery.json` exists but `source-snapshot.json` does not, the command reuses
  and validates discovery entirely offline, then resumes missing `<filename>.part`
  downloads. It does not rediscover mutable metadata during that recovery run.
- A completed individual artifact is reused only after its exact frozen length and
  pinned MD5 verify. Invalid or ambiguous partial state causes a closed failure;
  move that exact partial cache to a quarantine location for inspection before
  rerunning into a fresh directory.
- Do not hand-edit `discovery.json`, a resolved size, or a checksum after a failed
  attempt. A transport interruption is recoverable by rerunning against the same
  validated frozen discovery record.
- A UUID, name, DOI, rights, content-link, MD5, or size mismatch is an upstream
  identity change, not a reason to weaken checks. Preserve the failed evidence and
  review a new manifest version if the upstream release truly changed.

## Completed-snapshot offline verification

When `source-snapshot.json` is present, rerunning the same command performs wholly
offline canonical/digest verification and makes zero requests. It re-reads the
checked-in manifest and `discovery.json`, re-hashes all six source files, rechecks
every frozen length and MD5, and validates the aggregate snapshot binding. Missing
or altered cache content fails verification without falling back to the network.
Recovery requires quarantining the exact invalid cache and explicitly starting a
new acquisition.

This distinction is intentional: a valid completed snapshot is immutable evidence,
whereas a partial run is restartable work. Neither mode silently repairs or replaces
evidence.

## Evaluation custody

Acquisition does not authorize leakage-prone modeling. Downstream processing must
enforce the fixed GSC split: `E13`, `E21`, `E28`, and `E31` for training; `E34` for
calibration; and `E57` for untouched final assessment. Whole compounds or chemical
scaffolds are held out across doses and plates for compound-generalization tests.
Images, cells, and wells are never independent split units, and all learned
transforms are fit on training partitions only.

Held-out responses and response-derived ranks are an isolated qualification answer
key. They must remain outside candidate prompts, memory, retrieval, fitting inputs,
and ordinary research artifacts. Operational access to the final key is granted
only after the candidate, transforms, thresholds, and scoring code are frozen.

## Offline tests and fixtures

The files in `apps/data/testdata/edinburgh-ds8038/` are schema fixtures. They model
one DSpace 7 item response and the six exact-identity bitstream responses, including
content `href` values and pinned MD5 checksums. Their deliberately small positive
`sizeBytes` fields are synthetic test values, not authoritative DataShare sizes, and
the directory contains no raw screen archive.

Run the focused offline tests when available:

```sh
cargo test --locked -p civilization-data cancer_edinburgh_gsc::tests
```

The tests must not contact DataShare or treat fixture byte lengths as source facts.
