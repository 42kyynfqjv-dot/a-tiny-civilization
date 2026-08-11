# ADR 0148: Cancer World research is cumulative and simulated evidence is explicit

Status: accepted for the live Cancer World research projection.

## Decision

Every successful blinded contribution is immutable prior art for later blinded
turns. The research worker builds a deterministic, content-addressed internal
catalogue from successful contributions, collapses near-duplicate titles without
deleting their receipts, and supplies bounded catalogue pages plus the newest full
hypothesis to later turns. Public admission never freezes an artifact: later work
may extend, challenge, replicate, supersede, or retract it.

Observer projections show one canonical entry for repeated work and retain an
expandable duplicate ledger. Deduplication changes neither the append-only job log
nor the separate Cancer World Hindsight bank.

Of the 500 unaffected founders, a seed-bound rank assigns exactly 167 to a durable
support-engineering cohort. One third of blinded turns draw from living members of
that cohort and alternate between diagnostic/laboratory instrument design and
treatment-machine design. Designs must state observables, controls, calibration,
failure modes, safety interlocks, and falsification tests. They may not claim a
device was built or produced an outcome.

Cancer World may eventually receive effectively unbounded *virtual* mouse cohorts,
cell and tissue assays, compound synthesis, dosing, pharmacokinetic/pharmacodynamic
simulation, toxicity testing, and manufacturing planning. The implementation must:

- use versioned, source-calibrated mechanistic models rather than an LLM inventing
  experimental measurements;
- preserve cohort seeds, parameters, model versions, assumptions, uncertainty, and
  complete result provenance;
- label every result `in_silico` and never describe a virtual organism as an exact
  biological replica;
- keep compute bounded and schedule unlimited logical cohorts in batches; and
- require an explicit human-approved adapter to a licensed laboratory before any
  physical experiment, animal study, compound synthesis, or manufacturing action.

Real laboratory observations, when available, enter as recorded external assay
inputs and remain distinguishable from simulations. Animal studies require the
applicable ethics and welfare review. Pharmaceutical manufacture and human use are
never autonomous simulation actions.

## Why

Research cannot compound if each model sees only one previous title, and duplicate
model generations are not independent evidence. Conversely, simulated biology is
useful for prioritizing hypotheses but cannot establish efficacy or safety. Keeping
the cumulative library and evidence-class boundary explicit makes the work more
useful without converting generated numbers into false wet-lab claims.

## Verification

- Rebuilding the public research projection from the immutable ledger produces the
  same canonical entries and duplicate groups.
- Rebuilding catalogue pages from the same successful blind contributions produces
  byte-identical memory inputs.
- A 500-person unaffected founder set yields exactly 167 stable engineering members.
- Structured-output schemas restrict engineering turns to the matching design kind.
- No simulated result can be serialized as an assay observation without its model
  identity and `in_silico` evidence class.
