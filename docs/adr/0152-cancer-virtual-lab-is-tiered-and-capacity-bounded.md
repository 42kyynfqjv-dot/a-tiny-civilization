# ADR 0152: Cancer virtual-lab fidelity is tiered and capacity-bounded

Status: accepted for Cancer World virtual-lab method 2.

## Context

The first virtual lab was a deliberately uncalibrated scalar filter. It made a
closed plan executable, but it could hide two central glioblastoma failure modes:
poor brain exposure and treatment selection for a resistant subpopulation. It
also would be wasteful to run a tissue-scale model for roughly one thousand
ordinary research turns per day on the shared eight-core, sixteen-gigabyte host.

Glioblastoma is locally infiltrative, has no clean surgical margin, and requires
agents to reach disease behind a blood-brain barrier. NCI identifies effective
BBB penetration as a central development constraint:
<https://dctd.cancer.gov/research/networks/gtn>. Published GBM models likewise
show that heterogeneous drug delivery and subpopulation sensitivity can jointly
control response:
<https://pmc.ncbi.nlm.nih.gov/articles/PMC6249307/> and
<https://pmc.ncbi.nlm.nih.gov/articles/PMC7472531/>. FDA PBPK guidance requires
the quality, relevance, and reliability of a model to be judged for its intended
use rather than treating simulation as interchangeable with clinical PK:
<https://www.fda.gov/regulatory-information/search-fda-guidance-documents/physiologically-based-pharmacokinetic-analyses-format-and-content-guidance-industry>.

## Decision

Cancer World uses a fidelity ladder:

1. **Structural multiscale screen.** Every closed plan receives an inexpensive,
   deterministic screen. Method 2 records sensitive, drug-tolerant, and resistant
   clone fractions before and after exposure; delivered exposure and target
   engagement; resistance selection; and, for drug-like interventions in an
   orthotopic subject, systemic exposure, BBB penetration, and unbound brain
   exposure. These values are dimensionless structural assumptions. They are not
   compound-, patient-, or model-line-specific measurements.
2. **Adversarial campaign.** Only a novel model-supported root receives distinct
   preregistered follow-up tests. The immutable campaign stopping rules in ADR
   0151 remain authoritative.
3. **Tissue-scale refinement.** Only campaign survivors may later enter an
   asynchronous one-job-at-a-time multicellular worker. This worker must have
   explicit CPU, memory, wall-time, and output-size limits and cannot run in the
   canonical runner process.
4. **Source-calibrated qualification.** A finalist can advance only after its
   compound, device, subject, and endpoint parameters are bound to cited datasets,
   calibration data are separated from held-out validation data, and the intended
   use is declared. No language model may invent missing numeric parameters.
5. **External validation.** Wet-lab, animal, and clinical evidence remain outside
   the simulation and are never inferred from a model tier.

Method 1 results remain immutable and readable. Method 2 writes parallel
content-addressed result rows and the observer projection reads only the current
method. Compact summaries are retained; large per-cell trajectories are
reproducible from frozen inputs and code versions rather than duplicated into
PostgreSQL.

The shared Hindsight and local-cognition containers receive default CPU, memory,
and process ceilings. A backlog may take longer to drain, but cannot claim the
entire host. Genesis and public observer availability take priority over Cancer
World throughput.

## Current capability boundary

Method 2 adds structural clone selection and a structural PK/BBB screen. It does
not yet implement mutation, genomic lineages, reversible plasticity, longitudinal
relapse, compound-specific ADME and transporters, spatial tissue, immune and
stromal populations, combination scheduling, multi-organ toxicity, or detailed
device physics. The public capability manifest must describe these two new
layers as abstracted, not available or validated.

Potential later engines must preserve the project's licensing and auditability.
PhysiCell is a commercially usable BSD-3-Clause multicellular engine and is a
candidate for the resource-capped tissue tier: <https://physicell.org/Downloads.html>.
Open Systems Pharmacology provides mature PBPK/QSP tooling under GPLv2; it is a
reference and possible separately operated qualification tool, not silently
linked into the current permissive Rust artifacts:
<https://www.open-systems-pharmacology.org/faq/>.

## Verification

- Re-executing one method-2 plan produces the identical result and checksum.
- Baseline and post-exposure clone fractions each sum to one million parts.
- Resistant selection equals the post-minus-baseline resistant fraction.
- Only orthotopic drug-like plans receive a PK/BBB readout, and unbound brain
  exposure cannot exceed systemic exposure multiplied by BBB penetration.
- Larger cohorts reduce sampling uncertainty while model/extrapolation uncertainty
  remains visible.
- Method-1 rows still validate against their historical schema and calibration
  marker.
- Container limits leave CPU and memory headroom for the runner, database, API,
  projector, and web service.
