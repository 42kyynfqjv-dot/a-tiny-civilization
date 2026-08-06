# Exact upstream source snapshots

This directory contains canonical acquisition manifests for actual scientific and
geographic source bytes. It does not contain normalized world-data bundles, canonical
simulation inputs, or a selected world seed.

Bulk upstream artifacts belong in the ignored `data/source-cache/` directory. Fetch
and verify a manifest with:

```bash
cargo run --locked -p civilization-data -- source fetch \
  data/source-snapshots/natural-earth-10m-land-v5.1.2.json \
  --artifact-root data/source-cache
```

After acquisition, the same complete set can be checked without network access by
replacing `fetch` with `validate`. Fetch never replaces an existing file: matching
bytes are reused, while any mismatch stops with an error.

The first two manifests cover Natural Earth generalized global land polygons and NOAA
ETOPO 2022 global 60 arc-second bedrock relief. Their own limitations are part of the
canonical manifests; they are evidence and acquisition proofs, not simulation-ready
world bundles. See
[ADR 0015](../../docs/adr/0015-exact-upstream-source-snapshots.md).
