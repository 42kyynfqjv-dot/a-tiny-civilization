# Cancer World: runtime-isolated NCI CNS response challenges

Status: deterministic data-side foundation derived from the already frozen National
Cancer Institute CellMiner database 2.15 export dated 2025-09-17.

This derivation turns the compound- and pair-disjoint held-out partition into two
different artifacts with an explicit leakage boundary:

1. A **prompt-safe candidate catalogue** contains concrete NSC identifiers, source
   drug names, source mechanism strings, FDA-mark metadata, and canonical challenge
   identifiers. It contains no observed activity value, ComboScore, response rank,
   or interaction-direction label.
2. A **qualification-only answer key** contains the six CNS observations for every
   eligible held-out candidate, plus deterministic within-profile ranks. It must be
   loaded only after a prediction is frozen and must never enter model context,
   retrieval, memory, or a research artifact.

The answer key is written beneath the ignored source cache by convention. The
underlying NCI measurements are public; this local separation prevents the runtime
prompt, Hindsight bank, and research catalogue from revealing qualification labels.
It cannot prove that an upstream model never encountered those public measurements
during pretraining, so the score is not clean out-of-sample model generalization.

The frozen export yields 3,857 informative complete held-out single-agent
challenges and 921 informative complete held-out combination challenges. The
resulting prompt-safe catalogue is 1,318,810 bytes with SHA-256
`ab9f8087135aeb6a62c1d351d088a492b3dafb1c01dd4c37af0d0659be5362a5`.
The isolated answer key is 4,718,254 bytes with SHA-256
`559d52f45f18901d3ce8fb844f99cd88045ccd3fbd0c99cb7e8139b85e59f4ce`;
its ordered answer-payload commitment is
`6011aa87c35ed3253bc30c1416e2185c102d359a4503622b55a437122a276e6d`.

## Eligibility and labels

The existing split remains unchanged: SHA-256 assigns whole NSC compounds and whole
canonical NSC pairs to calibration or held-out partitions. A challenge candidate
must be in the held-out partition and have a retained measurement for all six CNS
lines. Missing profiles are excluded rather than imputed. Profiles whose six
retained measurements are all equal are also excluded because they contain no
pairwise ordering to predict or score. This removes 216 single-agent profiles from
the eligible set; no combination profile in the frozen export is all-tied.

For each candidate, answer observations are serialized from largest value to
smallest, breaking value ties by cell-line identifier. Rank 1 is the largest observed
value. Ties share competition rank `1 + number of strictly larger values`. NCI-60
activity z scores and ALMANAC ComboScores remain separate measurement families.
Combination values also receive a literal negative/zero/positive direction label;
no clinical-response label is invented.

The candidate catalogue contains no answer hash or commitment. From the isolated
side, the answer key records the SHA-256 of the exact pretty-JSON catalogue bytes and
commits to its own ordered answer payload. Derivation fails unless candidate
identities, NSC identities, commitments, profile lengths, and access classes agree.
A recursive serializer check rejects any answer-field key in the prompt-safe
catalogue.

## Reproduce

```sh
cargo run -p civilization-data -- derive cancer-nci-cellminer-challenges \
  --source-directory data/source-cache/nci-cellminer-2026-08-12 \
  --registry data/cancer-research/gbm-dataset-registry-v1.json \
  --catalogue-output data/cancer-research/nci-cellminer-2-15-cns-challenge-catalogue-v1.json \
  --answer-key-output data/source-cache/nci-cellminer-2026-08-12/nci-cellminer-2-15-cns-challenge-answer-key-v1.json
```

Both paths must be new: the compiler refuses to replace either artifact. Repeating
the derivation into fresh paths from the same verified ZIPs produces identical
bytes.

Before starting a qualification worker, stage the ignored key rather than mounting
the source cache directly:

```sh
bash scripts/stage-cancer-nci60-qualification-key.sh
```

The helper verifies both pinned SHA-256 values, copies the answer key to
`runtime-qualification/nci60/<answer-key-sha256>/`, and publishes it mode `0444`.
That mode is intentional: the container runs as uid 10001, which is not the owner of
the source-cache file, and receives only this immutable file through a dedicated
read-only bind. The source cache and answer key are never copied into the image or
mounted into the research worker, cognition worker, Hindsight, runner, API, or web
service. `scripts/smoke-cancer-nci60-qualification-key.sh` verifies the real bind and
hash from inside the runtime image as uid 10001. Both the source cache and staging
tree are excluded from the Docker build context as an additional non-leakage guard.

## Scientific boundary

These are measurements from six long-established two-dimensional NCI-60 CNS cell
lines. A matching runtime-isolated prediction is one public in-vitro benchmark
observation. It may reflect learned prior knowledge, mechanistic reasoning, chance,
or some mixture; one six-line ordering is not a treatment verdict and is not used to
promote an otherwise unrelated research theory. It is not evidence of patient efficacy, safety,
therapeutic index, exposure, immune response, organoid or xenograft response, or
clinical benefit.

The frozen source hashes, source-set commitment, response semantics, reuse note, and
primary-source links are documented in
[`CANCER_WORLD_NCI_CELLMINER_BASELINE.md`](CANCER_WORLD_NCI_CELLMINER_BASELINE.md).
