# ADR 0015: Upstream scientific evidence is acquired as exact source snapshots

## Status

Accepted on 2026-08-06 for pre-normalization source acquisition. A source snapshot is
not a world-data bundle, normalized simulation state, a tick-zero input, or permission
to start canonical genesis.

## Context

The full-Earth bundle must be built from actual, cited Earth data. A page URL and a
dataset name are insufficient: upstream files can change in place, multipart formats
can be incomplete, license pages can drift, and a downloader can silently overwrite
the evidence that an earlier normalization used.

Raw global sources are also too large and legally varied to commit casually to Git.
The repository needs a reviewable acquisition record and a reproducible local cache
without pretending that downloaded source bytes have already become simulation-ready
ecology.

## Decision

- Schema-v1 `SourceSnapshotManifest` records a stable snapshot ID, publisher,
  documentation and license URLs, upstream release and immutable revision, dataset
  version, retrieval date, declared scope, explicit limitations, and every retained
  artifact.
- Every artifact has one role (`data`, `documentation`, `license_evidence`, or
  `version_evidence`), a portable lowercase relative path, HTTPS download URL, media
  type, decimal byte length, and SHA-256 digest. A complete manifest requires all four
  roles and strict path ordering. Zero lengths, zero hashes, duplicate limitations,
  unsafe paths, non-HTTPS URLs, missing roles, and mutable-looking revisions fail.
- Schema v1 requires `revision_in_every_artifact_url`: the declared lowercase
  hexadecimal upstream revision must occur literally in every artifact URL. A provider
  without revision-bound immutable locators needs a future explicit locator policy; a
  floating URL beside an unrelated hash is not accepted silently.
- Canonical source-snapshot bytes are compact field-ordered JSON followed by one LF.
  The manifest digest covers those exact bytes. Artifact paths describe the local
  cache layout; they may normalize upstream filename case while download URLs retain
  the exact upstream names.
- `civilization-data source fetch` streams only missing artifacts over HTTPS into a
  same-directory partial file. It bounds bytes while downloading, verifies final
  length and SHA-256, syncs the file, and publishes with a no-replacement hard link.
  Existing files must already match; a mismatch stops acquisition rather than being
  overwritten. Partial files are removed on failure. Unix publication also syncs the
  parent directory after linking and removing the partial name.
- Parent directories and artifact leaves cannot be symbolic links. Fetch and offline
  validation resolve all files beneath one explicit artifact root. Hashing is
  streaming so future multi-gigabyte sources are not loaded into memory.
- Raw artifacts live under the ignored `data/source-cache/` by convention. Git stores
  manifests, acquisition code, citations, and tests—not the bulk source archive.
- A source snapshot can be cited by a later normalization release only after that
  pipeline binds its outputs to the snapshot digest and verified artifacts. Live world
  execution and replay never download sources.

The cache is a trusted, single-operator workspace. Component-by-component symlink
checks prevent accidental escape, but this implementation does not claim race-free
protection from a hostile process concurrently swapping paths. Such a cache must not
be shared with untrusted writers.

## First exact snapshot

The first committed manifest is Natural Earth’s 1:10m global land polygon theme at
repository tag `v5.1.2`, immutable commit
`f1890d9f152c896d250a77557a5751a93d494776`. The theme's own version file says
`5.1.1`; that is retained rather than relabeled because the `v5.1.2` release changed
cultural themes, not this physical layer.

Nine exact artifacts total 7,209,312 bytes. They include the five shapefile components,
theme documentation/version evidence, and repository license/version evidence. The
canonical manifest digest is
`21382550977608ef2f8e3f4f787a987d7c06848560fcd8902b6a44e7857b427a`.
Natural Earth states that its data is public domain, so the snapshot uses
`LicenseRef-Natural-Earth-Public-Domain` and retains immutable repository evidence plus
the official terms URL.

This is actual global land geometry, but it is generalized cartography at a nominal
1:10,000,000 scale. It is useful for proving acquisition and as coarse land/coastline
evidence; it cannot by itself define measurement-resolution coastlines, bathymetry,
elevation, habitat, materials, ecology, local terrain, or an S2 L10 canonical root.

## Verification

Acquire missing bytes into the ignored cache and immediately verify the complete set:

```bash
cargo run --locked -p civilization-data -- source fetch \
  data/source-snapshots/natural-earth-10m-land-v5.1.2.json \
  --artifact-root data/source-cache
```

Reverify the same bytes with all networking absent:

```bash
cargo run --locked -p civilization-data -- source validate \
  data/source-snapshots/natural-earth-10m-land-v5.1.2.json \
  --artifact-root data/source-cache
```

Tests pin canonical manifest bytes and digest, required provenance roles, exact total
length, streaming tamper detection across chunk boundaries, safe paths, symlink
rejection, and no-replacement publication.

## Consequences

The repository now contains its first exact, license-compatible, whole-Earth source
snapshot without committing raw data or claiming a scientific bundle. The next source
work must snapshot measurement-oriented relief, bathymetry, climate, soil,
hydrography, habitat, and taxon evidence; define deterministic normalization; and
construct content-addressed S2 roots. Canonical genesis remains blocked throughout.
