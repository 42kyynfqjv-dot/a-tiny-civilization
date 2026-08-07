# Source inspections

These small committed JSON files are deterministic, exhaustive measurements of exact
source snapshots. They are review evidence, not canonical world-data layers and not a
substitute for the raw archives retained outside Git under `data/source-cache/`.

`copernicus-satellite-land-cover-v2-1-1-2022-census.json` counts every value in all
five 64,800 × 129,600 Copernicus 2022 land-cover rasters. Its generator first verifies
the complete source snapshot, archive/member hashes and semantics, then reads all
2,048 native 2,025 × 2,025 chunks. The repository test pins the JSON byte fingerprint
and independently checks that each field totals exactly 8,398,080,000 cells.

Regenerate only to a new path; publication refuses replacement:

```bash
cargo run --release --locked -p civilization-data -- \
  inspect copernicus-land-cover-census \
  --source-snapshot data/source-snapshots/copernicus-satellite-land-cover-v2-1-1-2022.json \
  --artifact-root data/source-cache \
  --output /tmp/copernicus-land-cover-census.json
```

The census preserves modern urban and agricultural classifications as observed source
evidence. It does not silently convert them into a fictional preindustrial landscape;
that reconstruction requires a separate, explicit and provenance-bound policy.
