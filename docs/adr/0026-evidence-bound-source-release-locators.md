# ADR 0026: Evidence-bound release locators support split scientific distribution

## Status

Accepted on 2026-08-06. This extends pre-normalization source acquisition; it does not
make a source snapshot a normalized layer or authorize canonical genesis.

## Context

Some publishers distribute a versioned data object and its immutable catalog or DOI
evidence from different official locations. Requiring the same release substring in
every URL would reject that reproducible evidence pattern; removing all URL binding
would permit a floating data object to masquerade as a release.

CHELSA-BIOCLIM+ v2.1 is the first case. Its global climate raster lives in an official
object store under `V.2.1`, while its dataset DOI `10.16904/envidat.332` and technical
specification live at a DOI-scoped location. Both must be retained and independently
hash-pinned.

## Decision

- `evidence_bound_release` is a third source-snapshot locator policy.
- Every `data` artifact URL MUST contain the declared `upstream_release`.
- At least one retained `version_evidence` artifact URL MUST contain the declared
  `upstream_revision`. Documentation and license artifacts remain individually
  SHA-256/length pinned even when their hosting path does not repeat either locator.
- The policy still requires HTTPS, all four artifact roles, canonical bytes, explicit
  non-commercial-license rejection, safe local paths, no-replacement acquisition, and
  offline revalidation. It is not a generic exception for unversioned downloads.
- The first manifest using the policy is `chelsa-bioclim-plus-v2.1-tas-january-1981-2010`:
  one CC0 global January 1981–2010 near-surface mean-temperature NetCDF raster, its
  metadata/license evidence, and DOI-hosted technical/version evidence. Its manifest
  SHA-256 is `339fc85f4c2be97aacaa182b6f1cee6abd036ce8f7381d29be5f9f0a9694828b`.

## Verification

Unit tests accept the split pattern only when both links are present, reject a data URL
without the release, and reject a missing DOI/revision-bearing version-evidence URL.
The committed CHELSA manifest is canonical, pins four artifacts totaling 105,303,650
bytes, and `source fetch` followed by offline `source validate` verifies each byte.

## Consequences

The project can pin a real open climate input without weakening exact provenance. This
single January normal is deliberately insufficient for a climate layer: the next data
work must inspect its grid/units/missing-value semantics, acquire the remaining annual
variables and months as needed, and specify an honest S2 normalization policy.
