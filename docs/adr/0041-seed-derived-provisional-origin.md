# ADR 0041: Seed-derived provisional origin

## Decision

The first embodied patch of a provisional world is not chosen by an operator.
`civilization-data derive provisional-land-origin-selection` reads the canonical
Natural Earth land-reference tile tree, verifies every referenced tile byte-for-byte,
and ranks every source-confirmed L10 land patch with a domain-separated SHA-256 digest
of the committed world seed and the land-reference root digest. The lowest rank wins.

The selected L10 patch is then refined to the configured embodied-patch level with
another domain-separated deterministic digest sequence. The emitted canonical artifact
includes the seed, source root digest, eligible count, L10 patch, embodied patch, and
both schema and policy versions. Inspection recomputes the entire scan and rejects a
changed source, candidate set, seed, rank, or descendant.

## Consequences

- Genesis location is reproducible from published inputs rather than curated for an
  attractive biome or observer preference.
- The land reference is a generalized coastline cross-check, so this selection is
  explicitly **not** a habitat-suitability, freshwater, abundance, or survivability
  claim. Its embodied descendant may need later higher-resolution admission checks.
- A runner may consume only a canonical selection whose seed and land-reference
  digest match the world’s immutable provisional composition.
- `civilization-data derive provisional-origin-environment` writes a canonical,
  no-replacement evidence artifact by joining that exact L10
  selection to the composition-pinned Copernicus observed-land-cover and CHELSA
  twelve-phase temperature tiles. `inspect provisional-origin-environment` derives
  the same view without writing it. Both validate the layer roots and selected tile
  bytes before emitting the complete retained evidence. The artifact remains explicitly
  **evidence only**: neither a land-cover class nor temperature normal establishes
  habitat suitability, occurrence, abundance, or an initial fauna population.
