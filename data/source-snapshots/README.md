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

The committed manifests currently cover Natural Earth generalized land polygons, NOAA
ETOPO 2022 global bedrock relief, two CC0 CHELSA-BIOCLIM+ temperature products, the
complete 1981–2010 ERA5 normal-period request, and the Copernicus C3S 2022 global
land-cover response. Their limitations and licence evidence are part of the canonical
manifests; they remain acquisition evidence rather than simulation-ready world bundles.
See [ADR 0015](../../docs/adr/0015-exact-upstream-source-snapshots.md).
