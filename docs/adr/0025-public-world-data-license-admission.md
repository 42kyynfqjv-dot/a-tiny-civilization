# ADR 0025: Canonical public-world evidence must permit commercial public reuse

## Status

Accepted on 2026-08-06. This constrains source admission; it does not assert that every
otherwise-admitted source is scientifically sufficient for a canonical layer.

## Context

The repository is Apache-2.0 and the observatory may accept supporter payments for
observer-only naming. Those payments must never affect the civilization, but they do
mean the public project cannot safely depend on a source that expressly prohibits
commercial use or redistribution.

Climate data is the first concrete pressure test. [WorldClim v2.1](https://www.worldclim.org/data/worldclim21.html)
is valuable reference material, but its [published terms](https://worldclim.org/about.html)
limit data to academic and other non-commercial use and prohibit redistribution without
permission. It is therefore not an eligible input to a canonical public-world bundle.
The project may cite it in research notes, but it must not download, normalize, or ship
it as world evidence.

## Decision

- A source snapshot, released bundle, and each released source record reject an
  explicit non-commercial license expression. The validator recognizes SPDX-style
  `-NC` and textual `noncommercial` / `non-commercial` restrictions case-insensitively.
- Rejection happens before a source snapshot can become canonical bytes. This makes a
  later review, fetch, or normalizer unable to treat an obviously incompatible source
  as eligible merely because it was hash-pinned correctly.
- The rule is intentionally narrow. It does not claim that every other license is
  compatible; license evidence, a license URL, and legal review still remain required.
  It only turns an unambiguous incompatibility into a deterministic failure.
- Openly licensed climate candidates, including [CHELSA-BIOCLIM+](https://www.chelsa-climate.org/datasets/chelsa_bioclim)
  (whose publisher reports CC0 1.0 for the current data release), may be evaluated in
  a separate source snapshot with exact artifacts, terms, version evidence, and
  documented limits. The initial CHELSA January snapshot is subsequently pinned under
  [ADR 0026](0026-evidence-bound-source-release-locators.md); it remains far short of
  a complete climate layer.

## Verification

Unit tests reject `CC-BY-NC-4.0` in a raw source snapshot and a released bundle, and
reject a textual `LicenseRef-Non-Commercial` source record. Existing Natural Earth
public-domain and NOAA CC0 manifests remain canonical and valid.

## Consequences

The project can retain a genuine public and supporter-facing observatory without
quietly making the world data license-incompatible. Climate-source selection remains
open, but WorldClim is explicitly excluded from the canonical pipeline.
