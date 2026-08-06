# Scientific world-data bundles

A world-data bundle is the exact, normalized scientific input release used by one or
more worlds. It is not a design note, a loose folder of downloads, or an instruction
to launch a world. Canonical genesis is allowed only when the release validator and
its matching `WorldConfiguration` both pass.

## Shared release requirements

Every bundle contains:

- its schema, stable identifier, numeric semantic version, title, and aggregate
  license expression;
- a named and described reference domain with source links;
- the exact coverage definition also committed in world configuration;
- a versioned normalization pipeline, source revision, and executable/source digest;
- exact pre-normalization source-snapshot IDs and manifest digests, with every
  full-Earth layer linked to the snapshot(s) that supplied its evidence;
- source records with publisher, HTTPS URL, version, retrieval date, license, byte
  length, safe relative path, media type, and SHA-256 digest of the retained artifact;
- actual-world catalog entities with at least one external identity and citation;
- exact fixed-decimal, categorical, or Boolean normalized parameters;
- a provenance record for every parameter;
- every engineering assumption, cited by the parameters it affects;
- content-addressed scientific layers whose fields declare units and whose release
  roots are present and hash correctly.

The public observatory may accept observer-only supporter payments. Consequently,
canonical evidence cannot use sources that expressly prohibit commercial use. The
validator rejects explicit `-NC`, `noncommercial`, and `non-commercial` license
expressions at source-snapshot, source-record, and bundle boundaries. This is a
minimum admission guard rather than a substitute for license review; see
[ADR 0025](../adr/0025-public-world-data-license-admission.md).

Bounded schema v1 is retained for development fixtures and published compatibility.
It requires one integer raster and climate, elevation, habitat, hydrography, and soil
layers with matching dimensions.

Canonical schema v2 is full-Earth only. It requires the pinned S2 hierarchy and
WGS 84/EGM2008 physical frames plus climate, elevation, bathymetry, coastline,
habitat, hydrography, and soil layer families. Each layer points to a
content-addressed tile-tree root with declared S2 level coverage and leaf count.
Traversed child indexes and tiles are verified against the path from that root; a
fine simulation level never implies that its source was measured at that resolution.

Every tile-index node is canonical compact JSON containing one layer identity and a
nonempty, sorted list of child-index or tile references. Each entry carries a real
structurally valid 64-bit S2 CellId, its matching level, media type, safe relative path,
decimal byte length, and SHA-256 digest. A child index must contain only cells inside
the S2 scope that referenced it and must strictly refine another index scope. Tile cells
may exist at several levels because source/aggregate and refined representations are
distinct data products.

Schema v2 also records a manifest cutoff date, explicitly per-source epoch composite,
mean-sea-level definition, direct-human-feature exclusion/inference policy, and
sensitive-location policy. A source retrieved after the manifest cutoff is rejected.
The release is a present-geography counterfactual biosphere, not a synchronized modern
photograph or a reconstructed prehistoric Earth.

Vectors use ascending byte order and contain no duplicate identifiers. Parameter
values cannot contain floating-point numbers. Decimal quantities use integer bounds,
an integer typical value, a decimal-place count, and an explicit unit, so two hosts
cannot interpret a binary float differently. Signed mantissas and unsigned artifact
byte lengths serialize as decimal strings so JSON implementations cannot round them.

The schema rejects use labels such as `edible`, `food`, `tool`, `medicine`, `prey`,
`shelter`, `building`, `weapon`, `technology`, or `invention` in parameter
property/category codes. Physical properties and effects belong in the bundle;
conclusions an agent would need to discover do not.

## Evidence classes

Each parameter uses exactly one class:

1. `direct_measurement` requires source artifacts and no assumption link;
2. `documented_transformation` requires source artifacts, a stated deterministic
   method, and no assumption link;
3. `literature_approximation` requires source artifacts and may cite explicit
   assumptions;
4. `engineering_assumption` requires at least one assumption-ledger entry and may
   retain contextual sources.

An assumption that no parameter cites is rejected. An entity with no normalized
parameter is also rejected, preventing attractive catalog entries from implying a
scientific implementation that does not exist.

## Canonical bytes and configuration binding

Release JSON uses the compact field order emitted by the Rust schema. The validator
parses, validates, re-encodes, and requires byte equality. Whitespace-only or key-order
variants are rejected even when they represent the same generic JSON value. The
SHA-256 digest therefore identifies one exact portable manifest. A schema-v1 manifest
carries every retained source and normalized raster directly. A schema-v2 manifest
carries retained source records plus normalized tile-tree roots; hashes are checked
again as each branch or leaf is traversed.

Full-Earth schema-v2 provenance is deliberately two-stage: source records retain the
specific artifacts needed by the release, while normalization also pins the canonical
source-snapshot manifests from which those artifacts were acquired. A layer cannot
refer to an undeclared snapshot, and a full-Earth bundle cannot omit snapshot evidence.
See [ADR 0016](../adr/0016-full-earth-bundles-bind-source-snapshots.md).

Validate a release with:

```bash
cargo run --locked -p civilization-data -- validate path/to/bundle.json
```

The command resolves artifact paths relative to the manifest, rejects path traversal,
outside-root resolution and symbolic-link leaf files, then reads every declared source,
index node, and tile. It checks canonical index bytes, S2 parentage, unique index scopes,
unique `(level, cell)` tiles, globally unique artifact paths, exact byte lengths,
digests, cycle absence, a depth-derived index bound, and the declared leaf count. It
does not report a release as valid when any branch or leaf is absent. This is
intentionally an expensive release/genesis gate, not a per-tick operation.

Also prove that schema, bundle identity/version, license, coverage geometry, and content
digest match a proposed tick-zero configuration:

```bash
cargo run --locked -p civilization-data -- validate \
  path/to/bundle.json \
  --configuration path/to/world-configuration.json
```

The command performs no network access. A successful URL field means the URL is
well-formed immutable provenance; it does not replace the required local source
artifact and digest.

## Pre-normalization source snapshots

An exact source snapshot precedes a world-data bundle. Its canonical manifest records
the upstream revision/release, dataset version, retrieval date, scope, limitations,
license and version evidence, and every artifact URL, role, byte length, and SHA-256
digest. Schema v1 requires every artifact URL to contain either the declared immutable
revision or, for an official frozen release with no commit identity, the declared
versioned release locator. Fetching is an explicit operator action; live simulation and
replay never use the network.

Acquire only missing artifacts into the ignored local cache:

```bash
cargo run --locked -p civilization-data -- source fetch \
  data/source-snapshots/natural-earth-10m-land-v5.1.2.json \
  --artifact-root data/source-cache
```

The fetcher uses HTTPS, streaming hashes, bounded lengths, safe directories, temporary
files, and no-replacement publication. Existing matching artifacts are reused;
existing mismatches fail. The same manifest and complete cache can then be verified
offline with `source validate`. See
[ADR 0015](../adr/0015-exact-upstream-source-snapshots.md).

The first dependency-light source inspection is available for the pinned Natural Earth
polygon stream. It verifies the complete source snapshot before parsing the ESRI
shapefile framing and prints a deterministic, raw-IEEE-bits summary; it is not a
normalized coastline or a canonical layer root:

```bash
cargo run --locked -p civilization-data -- inspect natural-earth-land \
  --source-snapshot data/source-snapshots/natural-earth-10m-land-v5.1.2.json \
  --artifact-root data/source-cache
```

The pinned ETOPO NetCDF schema is likewise inspected with the repository's pure-Rust
NetCDF-4 reader, so normalization does not rely on host GDAL or NetCDF tools. The
reader verifies every `lat` and `lon` coordinate lies on the expected 60-arc-second
half-arcsecond lattice before it inspects or derives data:

```bash
cargo run --locked -p civilization-data -- inspect etopo \
  --source-snapshot data/source-snapshots/etopo-2022-v1-60s-bed.json \
  --artifact-root data/source-cache
```

The first CHELSA artifact is inspected by the same portable reader. This command
verifies the complete evidence-bound snapshot, checks the pinned 20,880 × 43,200
January `Band1` grid shape, and emits the raw IEEE-754 coordinate endpoints and
source hashes. It establishes exactly what has been retained; it does not assign
units or semantics beyond the source metadata, aggregate the raster to S2, or make
it a canonical climate layer:

```bash
cargo run --locked -p civilization-data -- inspect chelsa-january-temperature \
  --source-snapshot data/source-snapshots/chelsa-bioclim-plus-v2.1-tas-january-1981-2010.json \
  --artifact-root data/source-cache
```

One raw grid cell can be examined without a GIS dependency. The zero-based row and
column refer only to the retained source-axis order; the result emits raw coordinate
and `Band1` IEEE-754 bits, not a converted temperature or an S2 assignment. See
[ADR 0027](../adr/0027-chelsa-raw-grid-access-before-normalization.md).

```bash
cargo run --locked -p civilization-data -- inspect chelsa-january-cell \
  --source-snapshot data/source-snapshots/chelsa-bioclim-plus-v2.1-tas-january-1981-2010.json \
  --artifact-root data/source-cache \
  --row 10440 --column 21600
```

The first reproducible elevation intermediate is a regular sampling of the verified
source's `z` values. It retains the source `f32` IEEE-754 bits in source row order,
includes both source digests and the selected spacing in an 84-byte binary header, and
publishes only to a new path. At the default five-arc-minute spacing it contains
2,160 × 4,320 samples (37,324,884 bytes):

```bash
mkdir -p data/derived-cache
cargo run --locked -p civilization-data -- derive etopo-grid \
  --source-snapshot data/source-snapshots/etopo-2022-v1-60s-bed.json \
  --artifact-root data/source-cache \
  --sample-arc-minutes 5 \
  --output data/derived-cache/etopo-2022-v1-5m.grid
```

This is a provenance-bound elevation input for the forthcoming S2 tile normalizer; it
is deliberately not a substitute for an S2 tile-tree root, a coastline, or a complete
canonical world-data bundle. Generated intermediates are local operator artifacts and
are not committed to the source repository.

The geographic input boundary used by that future normalizer is independently
inspectable. Coordinates are exact E7-degree WGS 84 values and route through a
fixed-point WGS 84 ECEF ray into the shared S2 contract:

```bash
cargo run --locked -p civilization-data -- inspect geographic-route \
  --latitude-e7 387000000 --longitude-e7=-903000000 --s2-level 10
```

This is a route inspection, not an elevation tile. Its remaining normalizer
requirements are recorded in [ADR 0021](../adr/0021-geographic-source-to-s2-normalization.md).

ETOPO itself is a 60-arc-second `Area` raster. Its first centre is exactly
latitude `-647940` and longitude `-1295940` in half-arcseconds; each source row or
column advances by 120 half-arcseconds. The source-centre route keeps that lattice
exact rather than rounding it to E7 decimal degrees:

```bash
cargo run --locked -p civilization-data -- inspect etopo-cell-route \
  --row 5399 --column 10800 --s2-level 10
```

This only proves the address and exact area support of one declared source cell. The
support convention is specified in [ADR 0022](../adr/0022-etopo-area-cell-support.md);
a canonical elevation layer still requires a versioned aggregation kernel.

For reproducible preparation of that aggregation, a sampled source-centre index pairs
each raw selected ETOPO `f32` value with its exact S2 centre address. It is still not a
tile or a claim of full-cell ownership; see [ADR 0023](../adr/0023-etopo-s2-centre-index.md).

```bash
cargo run --locked -p civilization-data -- derive etopo-centre-index \
  --source-snapshot data/source-snapshots/etopo-2022-v1-60s-bed.json \
  --artifact-root data/source-cache \
  --sample-arc-minutes 60 --s2-level 10 \
  --output data/derived-cache/etopo-2022-v1-60m-centres.bin
```

The command only writes a new output path. Retain the emitted source and output digests
with any downstream aggregation; generated intermediate bytes are local artifacts and
are not committed to Git.

The index can be independently revalidated and summarized at an ancestor S2 level:

```bash
cargo run --locked -p civilization-data -- derive centre-summary \
  --input /tmp/atc-etopo-2022-v1-60m-centres.bin \
  --s2-level 0 \
  --output /tmp/atc-etopo-2022-v1-60m-centre-summary-l0.bin
```

This output is a fixed-point **source-centre quadrature summary**. It contains sample
counts and min/mean/max relief values in signed millimetres, along with input/source
digests. It is useful evidence while building the true normalizer, but does not claim
that source rectangles are contained by, or area-weighted over, the target S2 cells.
See [ADR 0024](../adr/0024-etopo-source-centre-quadrature-summary.md).

## Current state

The schema-v1 compatibility path, schema-v2 full-Earth contract, canonical tile-index
format, exhaustive offline tree traversal, CLI, and adversarial tests are implemented.
The tests cover tampered leaves, false counts, cycles, malformed S2 identities,
duplicate/unsorted entries, noncanonical index bytes, wrong layer/level metadata, and
cross-face parentage. Exact pre-normalization snapshots now pin public-domain Natural
Earth generalized global land artifacts, CC0 NOAA ETOPO 2022 global 60 arc-second
bedrock relief, and one CC0 CHELSA-BIOCLIM+ v2.1 January 1981–2010 temperature normal
with official release, license, and version evidence. The ETOPO
pipeline can now derive a portable, hash-bound global elevation intermediate and a
separately checked source-centre quadrature summary from that evidence. No normalized
S2 layer root or canonical seed is claimed yet. The CHELSA inspector verifies the
retained January grid without turning it into climate semantics. Lower Buffalo remains
only the first high-resolution conformance tile. The next data work is deterministic
S2 normalization and planet-level roots, then reference-tile normalization without
placeholder values.
