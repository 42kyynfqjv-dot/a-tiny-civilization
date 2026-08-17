# ADR 0160: GBM5K dependency data is isolated external evidence

Status: accepted for acquisition; not yet admitted to a production qualifier.

## Context

Cancer World's PDC000711 layer can say whether an exact target protein was
reported across patient-derived glioblastoma models. Presence is not functional
dependence. A stronger target triage axis asks whether genetic loss measurably
reduces fitness across heterogeneous patient-derived glioblastoma stem-cell
cultures.

MacLeod et al. report focused CRISPR-Cas9 fitness screens using the GBM5K guide
library across 30 patient-derived GSC cultures. The AACR Figshare supplement's
Table S4 is described as normalized Bayes Factor scores, essentiality calls, and
subtype scores from GBM5K and TKOv3 screens. AACR marks the dataset CC BY; CC BY
4.0 permits commercial reuse with attribution and notice of changes.

The repository has migrated or republished some AACR supplements under multiple
item identifiers. A human-readable file-size label or a title search is therefore
not a sufficient byte identity.

## Decision

The checked-in manifest
`data/cancer-research/aacr-gbm5k-dependency-source-v1.json` pins AACR Figshare
article `28183566`, version `1`, its dataset DOI, related article DOI, title,
license, intended use, and leakage boundary. Acquisition must resolve that exact
version from Figshare's public API and require:

- the exact article, version, title, dataset DOI, related article DOI, and CC BY
  4.0 license;
- exactly one non-link XLSX file with a safe name and bounded positive length;
- a Figshare HTTPS download URL bound to that exact file ID whose bytes match
  the API length.

The local acquisition snapshot is create-only and binds the exact API response,
file ID, file name, length, downloaded SHA-256, and source-set SHA-256.
Existing paths are verified, never refreshed in place. A different AACR item or
version requires a new manifest and method decision.

Figshare's documented public-file representation does not expose the
`supplied_md5` and `computed_md5` fields available on private-file endpoints. We
therefore do not pretend to possess a provider checksum: first acquisition is
bound by HTTPS, exact immutable-version metadata, file identity, and byte length;
the create-only snapshot's SHA-256 is authoritative for every later local
verification.

The admitted bytes are qualification-worker-only. They never enter Cancer World
prompts, Hindsight memory, cumulative research pages, campaign selection, or the
tissue simulator. After an artifact independently names exact molecular targets,
a later observer qualifier may open only exact-symbol dependency evidence and
persist an immutable provenance-bound result.

This evidence means genetic dependency in the published culture/screen context.
It does not mean that a target is druggable, that an inhibitor reaches a brain
tumor, that genetic knockout resembles pharmacologic inhibition, or that an
intervention is safe or effective. Cultures are not described as independent
patients unless source metadata proves their donor relationships.

## Consequences

- Cancer World gains a commercially reusable functional-genomics axis stronger
  than molecular presence alone.
- The evidence can falsify unsupported target stories without contaminating
  hypothesis generation.
- Culture-disjoint evaluation cannot be relabeled patient-disjoint by inference.
- GBM5K and TKOv3 values remain distinct until an explicit transformation proves
  their library and score semantics are compatible.
- Production qualification remains disabled until the acquired workbook is
  inspected, its sheets and labels are frozen, and exact parsing tests exist.

## Verification

- Manifest tests require the exact article/version, DOI pair, license, one-file
  policy, bounded XLSX constraint, and closed leakage flags.
- Offline schema tests must reject changed identity, permissive/missing license
  metadata, link-only files, multiple files, unsafe names, bounds violations,
  and non-HTTPS download URLs. The first live acquisition additionally retains
  and hashes the exact API response; it must not be described as fixture-tested
  until those provider bytes have actually been acquired.
- Download fixtures must reject changed length or file identity and must prove
  create-only resume behavior and retained SHA-256 verification.
- No research-worker or memory boundary may address the acquired source path.

## Primary references

- MacLeod et al., *Fitness Screens Map State-Specific Glioblastoma Stem Cell
  Vulnerabilities*: <https://pubmed.ncbi.nlm.nih.gov/39186687/>
- AACR Figshare Table S4 record:
  <https://aacr.figshare.com/articles/dataset/Table_S4_from_Fitness_Screens_Map_State-Specific_Glioblastoma_Stem_Cell_Vulnerabilities/28183566/1>
- CC BY 4.0 deed: <https://creativecommons.org/licenses/by/4.0/>
