# ADR 0151: Cancer research campaigns are adversarial and lineage-bound

Status: accepted for the live Cancer World research projection.

## Decision

Cancer World no longer promotes the newest hypothesis on a long fixed interval.
Every fifth turn in each independent program is instead available to the oldest
eligible unresolved campaign. A blind artifact becomes eligible only when:

- it contains a closed virtual-experiment plan;
- the current deterministic virtual-lab method supports its prediction; and
- the current observer-side overlap audit reports a new combination or no close
  match, rather than known overlap or an audit error.

The original contribution hash is stored as `frozen_candidate_hash` on every
campaign request. This existing immutable field is the lineage edge; campaign
state is reconstructed from the append-only request, result, novelty, and virtual
experiment records rather than maintained in a mutable status table.

An active campaign receives up to five preregistered adversarial model tests. The
test sequence varies subject abstraction, intensity, exposure, endpoint, and
modality in a deterministic order. Every required plan is frozen in a
content-addressed campaign directive before the model is called. The model may
explain the test but cannot change its plan or invent its result. Repeating an
identical deterministic plan does not count as replication.

One no-material-effect or concerning-tradeoff result falsifies the campaign at
this model layer. Three supporting results with no falsifying result allow it to
survive the replication round. Five tests without either condition are
inconclusive. A final synthesis turn receives that computed outcome as immutable
input and may explain, but not upgrade, it. Paid escalation is permitted only for
a survived-round synthesis and remains inside the existing treasury circuit
breaker.

## Evidence boundary

These are distinct adversarial tests within the same uncalibrated deterministic
engine. They are not statistically independent biological replications. A
survived campaign is better prioritized for external work; it is not a validated
treatment, device, animal result, clinical result, or cure.

The public research projection therefore publishes the campaign lineage and a
lab-capability manifest. The manifest currently identifies closed plans and
campaign scheduling as available; subject systems and intervention response as
abstracted; and tumor evolution, resistance, PK/PD, blood-brain-barrier transport,
spatial microenvironment, immune dynamics, combination interactions, whole-organ
toxicity, and detailed device physics as missing. Biological and clinical
validation always requires a real licensed laboratory and applicable oversight.

## Why

A stream of individually plausible ideas does not create cumulative research.
Promising work needs a durable branch, adverse tests, a stopping rule, and an
outcome the generating model cannot award itself. Binding each branch to immutable
provenance also makes the public console a faithful view rather than a second
source of research truth.

## Verification

- Rebuilding from the same immutable rows produces the same campaign roots,
  counts, outcomes, and public summaries.
- Every adversarial plan has a distinct canonical hash from its root and earlier
  tests.
- Structured output rejects a campaign response that changes its required plan.
- A campaign is falsified on the first adverse model result, survives only after
  three supports without an adverse result, and becomes inconclusive after five
  otherwise nonterminal tests.
- Missing, failed, or still-running model and virtual-lab jobs do not manufacture
  a test result and do not stall ordinary blind research.

