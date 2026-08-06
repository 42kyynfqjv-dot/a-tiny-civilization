# ADR 0032: ERA5 evidence retains CDS ZIP containers before normalization

## Status

Accepted on 2026-08-06 for the acquired 1981-2010 ERA5 monthly-means evidence.
This governs source preservation and inspection only; it does not define a canonical
climate layer or enable full-Earth genesis.

## Context

The fixed CDS request asks for NetCDF data, but the first returned annual response is
a ZIP container rather than a standalone NetCDF file. Its two members separate the
monthly accumulated total-precipitation field from the instantaneous mean fields.
The observed 1981 members are `data_stream-moda_stepType-avgad.nc` and
`data_stream-moda_stepType-avgua.nc`. Both have 12 monthly samples on a 721 × 1440
global latitude/longitude grid.

Treating the HTTP response extension as the content type would mislabel immutable
source evidence and makes a later normalizer depend on accidental filesystem naming.
Extracting members into the raw source cache would also create an undocumented second
copy of upstream evidence that is not independently hashed by the snapshot manifest.

## Decision

- The retained upstream artifact is the exact ZIP response, with media type
  `application/zip`. A member's NetCDF format is a property verified during
  normalization, not the source artifact's media type.
- The acquisition helper publishes a `*.zip` target only after validating that it is
  a nonempty ZIP whose members are nonempty `*.nc` files and pass ZIP CRC checks.
  Existing paths and partial files always fail closed; no source artifact is replaced.
- A legacy initial acquisition used the `*.nc` filename for ZIP bytes. Once the full
  batch is complete, the migration tool hard-links each verified legacy file to its
  correct `*.zip` name before removing only the old directory entry. It never rewrites
  or drops the file's bytes and refuses every incomplete, non-ZIP, or conflicting case.
- The source-snapshot generator hashes the retained ZIP bytes and records them as
  `application/zip`, alongside separately retained official documentation, licence,
  and DOI version evidence.
- A climate normalizer must verify the archived member schema for every annual source
  file after source-snapshot verification and before reading values. It may use private
  scratch extraction, but must never mutate the retained ZIPs or elevate an extracted
  member into separately claimed upstream evidence.

## Consequences

The project accurately describes the scientific evidence it acquired. A future
normalizer must make the variable-to-member mapping, units, calendar, missing-value
policy, spatial sampling, 30-year aggregation, and output uncertainty explicit before
claiming a climate root. No observer or agent reads raw archive names or request
metadata.
