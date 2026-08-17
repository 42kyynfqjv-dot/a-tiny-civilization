# ADR 0158: Cancer tissue refinement is preregistered and resource-bounded

Status: accepted for Cancer World tissue-refinement method 1.

## Context

ADR 0152 reserved a third virtual-lab tier for the few hypotheses that survive
the complete adversarial campaign in ADR 0151. A scalar method-2 screen cannot
express spatially uneven exposure, competition among heterogeneous clone classes,
or resource-poor pockets. Those omissions can make a superficially favorable
average hide resistant enrichment or an untreated invasive edge.

A tissue model does not become biologically or clinically validated merely by
adding cells and a lattice. The current plans contain no source-calibrated drug,
device, tumor-line, immune, stromal, pharmacodynamic, or patient parameters. The
new engine must expose a sharper falsification surface without inflating the
evidence class or taking compute from the ordinary world.

## Decision

Cancer World adds a separate, observer-side tissue-refinement method. It does not
replace or mutate virtual-lab methods 1 or 2. Historical rows retain their exact
schemas and hashes.

Only a complete campaign survivor is eligible. Its root method-2 result must
support the plan, at least three distinct campaign results must support it, no
campaign result may falsify it, and every request, artifact, plan, and result
hash must be distinct and valid. Durable selection must load the complete
campaign; callers may not hide an adverse result by supplying a subset.

The method derives one immutable protocol from those rows. The language model and
operator have no numeric tuning input. The protocol freezes:

- the campaign, artifact, plan, root-result, and follow-up result hashes;
- one field family selected from the plan modality: diffusive exposure, a
  deterministic device field, or a resection mask;
- a two-dimensional 16–32 by 16–32 lattice, no more than 1,024 sites;
- no more than 2,048 initial population units and a hard 4,096-unit capacity;
- no more than 256 time steps and 16 compact snapshots per scenario; and
- exactly three preregistered structural-assumption scenarios: lower-field,
  nominal, and upper-field.

The pure engine uses integer millionths and deterministic integer rounding only.
It represents three inherited phenotype compartments, oxygen and nutrient
proxies, local treatment or device exposure, simple consumption, division,
death, and neighbor migration. It records resource use, termination reason,
compact scenario checkpoints, and the spread across the three field assumptions.
Per-cell trajectories are not durable output; the frozen protocol, method
version, and result hash make them reproducible.

Exposure durations longer than 256 hours are explicitly truncated in the
protocol, never silently compressed. Cell-capacity exhaustion and numerical
invariant rejection are result states, not favorable outcomes. The result calls
itself an `uncalibrated_deterministic_tissue_projection` and contains four fixed
limitations. It may not claim treatment efficacy, safety, cure, an animal result,
or clinical meaning.

Execution is asynchronous and one job at a time. The application work-slot state
machine rejects a second active job. ADR 0161 supplies the durable atomic database
claim and separate resource-bounded process required before this tier is enabled
on a host. The canonical runner process never executes it.

## Capability boundary

This implements **structural spatial tissue refinement**: a bounded lattice,
heterogeneous phenotype compartments, oxygen/nutrient fields, modality-appropriate
exposure fields, deterministic migration, assumption scenarios, and explicit
failure states.

It does **not** implement source-calibrated intervention response, molecularly
resolved clone lineages, de-novo mutation, reversible state plasticity, longitudinal
relapse, vasculature, blood-brain-barrier transport within the lattice, immune or
stromal populations, detailed radiation/thermal/electric physics, combination
schedules, multi-organ toxicity, real mice, real patients, or wet-lab evidence.
Those remain missing or require separate source-qualified methods under ADR 0153.

## Verification

- Re-deriving a protocol and re-running it yields identical bytes and hashes.
- Incomplete, inconclusive-only, falsified, duplicated, legacy-method, or
  provenance-invalid campaigns cannot enter the tier.
- A changed protocol is rejected even if it is otherwise within resource caps.
- Every clone sum, field, fraction, lattice-site update count, checkpoint count,
  grid dimension, cell count, and step count is validated against its ceiling.
- Every scenario records a final checkpoint and an explicit termination state.
- One work slot cannot acquire a second job until the first is completed or
  explicitly released as failed.
- A method-2 result has the same canonical hash before and after refinement.

## Consequences

The new tier can reveal spatial failure modes that the scalar screen cannot, but
it remains a hypothesis-prioritization instrument. Its added detail is not a
percentage of tumor realism and must not be presented as one. Durable claims,
bounded scheduling, and compact observer presentation are supplied by ADR 0161;
source calibration remains separate and absent.
