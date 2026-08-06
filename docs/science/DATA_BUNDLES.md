# Scientific world-data bundles

A world-data bundle is the exact, normalized scientific input release used by one or
more worlds. It is not a design note, a loose folder of downloads, or an instruction
to launch a world. Canonical genesis is allowed only when the release validator and
its matching `WorldConfiguration` both pass.

## Schema-v1 release requirements

Every bundle contains:

- its schema, stable identifier, numeric semantic version, title, and aggregate
  license expression;
- a named and described reference domain with source links;
- the exact integer spatial grid also committed in world configuration;
- a versioned normalization pipeline, source revision, and executable/source digest;
- source records with publisher, HTTPS URL, version, retrieval date, license, byte
  length, safe relative path, media type, and SHA-256 digest of the retained artifact;
- actual-world catalog entities with at least one external identity and citation;
- exact fixed-decimal, categorical, or Boolean normalized parameters;
- a provenance record for every parameter;
- every engineering assumption, cited by the parameters it affects;
- content-addressed climate, elevation, habitat, hydrography, and soil layers whose
  shapes match the grid, whose fields declare units, and whose relative files are
  present and hash correctly.

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
SHA-256 digest therefore identifies one exact portable manifest. That manifest forms
a Merkle-style release root: it carries the path, length, and SHA-256 digest of every
retained source snapshot and normalized layer.

Validate a release with:

```bash
cargo run --locked -p civilization-data -- validate path/to/bundle.json
```

The command resolves artifact paths relative to the manifest, rejects path traversal,
reads every artifact, and checks its exact byte length and digest. It does not report a
release as valid when only the JSON manifest is available.

Also prove that schema, bundle identity/version, license, spatial grid, and content
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

The schema, pure validator, CLI, and adversarial unit tests are implemented. No Lower
Buffalo release bundle is claimed yet. The next data work is to archive and hash the
specific authoritative source snapshots, resolve stable identities for the approved
roster, and normalize measured parameters without inventing placeholder values.
