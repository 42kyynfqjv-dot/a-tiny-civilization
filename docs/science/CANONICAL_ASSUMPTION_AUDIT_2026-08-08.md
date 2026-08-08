# Canonical ruleset-30 assumption audit — 2026-08-08

This is a machine-derived audit of the immutable v16 genesis directory for world
`b3ea736d-7a5a-5161-a74b-fa8c4302d333`. It does not alter or scientifically admit that
world. The audit first verifies the complete genesis `SHA256SUMS` manifest, whose digest is
`36f92754e0e50c7bfc018c303f57b670f0320ba01452d013a5b9820afb27d4d9`, and then checks
cross-file species coverage before counting causal evidence classes.

Run it with:

```bash
scripts/audit-canonical-science.py \
  /absolute/path/to/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset30-v16
```

## Result

The candidate is mechanically qualified but scientifically assumption-heavy in the parts that
control life:

- all 33 body profiles use engineering assumptions for metabolic rate, physiological regulation,
  reproductive physiology, and heritable dispositions;
- adult body mass is a source-addressed literature approximation for 5 profiles and an engineering
  assumption for 28;
- 30 of 66 category-specific maturity commitments are literature approximations and 36 are
  engineering assumptions;
- ecology traits cover 23 of 32 fauna species. The nine uncovered species are emitted by exact
  scientific name in the audit report;
- the glucose and water reservoirs, their replenishment, and all 66 species-specific oral-transfer
  responses are engineering assumptions;
- the silicon-dioxide object has a real PubChem identity but its local availability remains part of
  the explicitly provisional material plan.

This is why the quality admission says `scientific_admission: false`. The audit makes the boundary
quantitative and regression-testable instead of relying on prose.

## Improvement order

1. Replace uniform assumed metabolic power with source-linked observations or an explicit,
   validated taxon/body-mass/temperature transformation. Metabolism directly controls survival and
   therefore population history.
2. Replace the universal glucose reservoir and universal oral responses with physical biomass,
   species diet, and water pathways. A real chemical identity is not evidence of local abundance or
   organism response.
3. Improve species-specific regulation and reproduction only where retained evidence supports the
   parameters; keep every remaining fallback explicit.
4. Expand ecology and adult-mass coverage, then validate the coupled causal model rather than
   treating independently sourced traits as automatically compatible.

## Newly identified compatible evidence

The 2025 FmrBT release is a useful next metabolic source. Its versioned Zenodo record is openly
licensed under CC BY 4.0 and contains 4,567 field-metabolic observations spanning 719 identified
species, with body mass and ambient temperature. The retained record is
[`10.5281/zenodo.16894769`](https://doi.org/10.5281/zenodo.16894769), paired with the
[`Scientific Data` methods and validation paper](https://doi.org/10.1038/s41597-025-05868-y).

An exact-name inspection against this world's 32 selected fauna found direct rows for only two
species: `Junco phaeonotus` and `Melanerpes formicivorus`. That makes FmrBT valid additional
evidence, not a complete solution. It must be content-addressed, parsed by a portable deterministic
pipeline, unit-normalized, and distinguished from basal or standard metabolic measurements before
it can affect a future ruleset.
