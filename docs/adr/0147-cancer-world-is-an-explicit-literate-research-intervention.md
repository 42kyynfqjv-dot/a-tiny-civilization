# ADR 0147: Cancer World is an explicit literate research intervention

Status: accepted for a fresh ruleset-38 experimental world; not yet activated.

## Decision

Cancer World is not a second open-ended genesis. Its manifest contains a canonical,
publicly disclosed `cancer_research` experiment commitment. Earth Genesis omits that
commitment and receives none of its capabilities or knowledge.

Cancer World begins with:

- adult glioblastoma as the immutable first-world research target; later fresh
  worlds may select pancreatic ductal adenocarcinoma or extensive-stage small-cell
  lung cancer without changing an active world's target;
- exactly 1,000 adult residents, with exactly 500 assigned an initial cancer
  condition by a seed-derived, birth-category-stratified rule fixed before genesis;
- fluent spoken and written English, interpersonal communication, teaching, and
  durable publication;
- abundant ordinary survival resources;
- cancer as the only modeled disease family;
- a private, non-graphic signal to every affected person conveying that abnormal
  growth exists, its rough body location and burden, and whether it is growing,
  stable, shrinking, spreading, or recurring;
- one overriding terminal objective for every affected person: permanently eliminate
  this abnormal growth and growths of its kind. Bodily maintenance, cooperation,
  communication, and experimentation remain available as instrumental subgoals;
- independently seeded research profiles spanning specialty, hypothesis prior,
  exploration tolerance, evidentiary threshold, replication preference, and
  willingness to challenge consensus. The shared objective does not install a shared
  theory or centrally authored research plan; and
- a preregistered evidence firewall: blind-discovery teams receive biological
  primitives, raw datasets, and returned observations but no live paper retrieval
  before freezing a hypothesis, predictions, and falsification test. Separate
  literature-audit and replication teams then assess novelty and prior evidence;
- a pinned `nvidia/nemotron-3-ultra-550b-a55b:free` exploration route using the
  dedicated Cancer World key. Provider failure records an unavailable input and
  never silently triggers paid work; and
- a distinct DeepSeek V4 Pro escalation route, with V4 Flash failure fallback,
  a $2.85 internal monthly stop, and the provider key's $3 monthly hard stop.
  Earth Genesis retains its unrelated free-first ladder.

The bootstrap supplies no cancer mutation, pathway, target, drug, experiment,
treatment, or cure conclusion. Those must enter through a versioned evidence corpus,
direct modeled observations, or recorded external assay results. Every model response,
memory retrieval, and assay input is recorded before it can affect canonical history;
replay never contacts those services.

## Why

Requiring Cancer World to rediscover language and literacy would spend nearly all of
the experiment on civilization bootstrapping rather than cancer research. Conversely,
giving every researcher an identical strategy would create correlated failure and
groupthink. A common objective with diverse strategies preserves purposeful search,
criticism, and replication.

The intervention is deliberately artificial and must never be presented as ordinary
human cognition or natural history. It tests whether a persistent, communicating,
memory-bearing research society can generate hypotheses worth external validation.
It cannot validate a treatment inside the simulation.

## Evidence boundary

The closest precedents cover components rather than this complete design:

- laboratory validation of LLM-generated breast-cancer hypotheses:
  <https://pubmed.ncbi.nlm.nih.gov/40462712/>;
- a multi-agent, lab-in-the-loop biological discovery system:
  <https://doi.org/10.1038/s41586-026-10652-y>;
- a multi-agent virtual therapeutic-research organization:
  <https://pubmed.ncbi.nlm.nih.gov/41808990/>; and
- an evaluation in which eight autonomous-research frameworks failed to complete a
  full literature-to-validated-result cycle:
  <https://www.biorxiv.org/content/10.64898/2026.01.05.697809v1.full>.

Cancer is modeled as a heterogeneous disease family, consistent with the National
Cancer Institute overview at
<https://www.cancer.gov/about-cancer/understanding/what-is-cancer>. Any future assay
bridge should prefer provenance-rich patient-derived models and organoids, including
the NCI PDMR and HCMI resources:
<https://dctd.cancer.gov/drug-discovery-development/reagents-materials/pdmr> and
<https://www.cancer.gov/ccg/research/functional-genomics/hcmi>.

## Consequences

- Ruleset 37 and event schema 34 introduce the experiment manifest. Event schema
  35 plus snapshot/state-hash schema 34 retain the exact initial affected cohort.
  Older manifests omit the field and preserve their published JSON and hashes.
- Ruleset 38, event schema 36, and snapshot/state-hash schema 35 add private,
  deterministic daily burden state. The first implementation uses bounded,
  target-specific growth-rate assumptions, seed-derived variation, clone-diversity
  bookkeeping, a burden-gated spread transition, and non-graphic mechanical
  mortality after a fixed terminal-burden threshold. These numbers are explicitly
  provisional engineering assumptions pending the later scientific-validation pass;
  they are not patient models, prognoses, clinical evidence, or treatment guidance.
- Published ruleset 37 semantics remain unchanged and replayable. New Cancer World
  genesis requires ruleset 38 exactly rather than retroactively changing ruleset 37.
- Bootstrap schema 2 fixes the 1,000-person initial population and 500-person
  affected cohort. The cohort cannot be hand-picked or changed after genesis.
- The research-language substrate must support claims, citations, hypotheses,
  criticism, experiment proposals, results, retractions, and papers as canonical
  events rather than unrecorded model prose.
- The scheduler must allocate paid research turns across independent profiles in
  simulation time while the wall-cost circuit breaker enforces the hard ceiling.
- The initial scheduler admits at most one research turn per simulated day. Normal
  days remain blinded free exploration; every seventh day may promote the newest
  successful blinded hypothesis into a paid DeepSeek challenge. If no successful
  hypothesis exists, that day remains free exploration. Promotion never bypasses
  the separate $2.85 monthly hard stop or the worker's paid-default-off control.
- Observer pages must label all Cancer World output as simulation-generated
  hypotheses, not medical guidance. No result becomes a cure claim without external
  reproducible biological and clinical validation.
- Ruleset 38 gives the fixed 1,000-person single-patch cohort a 50,000-event
  per-partition execution envelope. This changes no action, selection, or biology;
  it prevents the older 10,000-event public-world safety envelope from rejecting a
  valid deterministic Cancer World transition.
- The same dense research cohort retains at most 2,048 current perception addresses
  per resident instead of the public world's 256. Entries are still keyed and
  replaced rather than appended, so the larger social neighborhood stays bounded.
