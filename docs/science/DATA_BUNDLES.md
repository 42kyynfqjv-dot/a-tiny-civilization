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
- source records with publisher, HTTPS URL, version, retrieval date, license, byte
  length, safe relative path, media type, and SHA-256 digest of the retained artifact;
- actual-world catalog entities with at least one external identity and citation;
- exact fixed-decimal, categorical, or Boolean normalized parameters;
- a provenance record for every parameter;
- every engineering assumption, cited by the parameters it affects;
- content-addressed scientific layers whose fields declare units and whose release
  roots are present and hash correctly.

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

## Current state

The schema-v1 compatibility path, schema-v2 full-Earth contract, canonical tile-index
format, exhaustive offline tree traversal, CLI, and adversarial tests are implemented.
The tests cover tampered leaves, false counts, cycles, malformed S2 identities,
duplicate/unsorted entries, noncanonical index bytes, wrong layer/level metadata, and
cross-face parentage. No global scientific release or canonical seed is claimed yet.
Lower Buffalo remains only the first high-resolution conformance tile. The next data
work is to archive the first global source snapshots and exact licenses, construct
planet-level roots, and normalize the reference tile without placeholder values.
