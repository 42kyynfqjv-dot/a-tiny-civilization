# ADR 0048: Reproductive physiology is private, deterministic, and birth-bound

## Status

Accepted on 2026-08-07. Ruleset-fourteen implementation follows this contract.

## Context

The world already retains bodies, survival pressure, primitive action, and bounded
trial-and-error, but canonical births are still manually supplied facts. Reproduction
must become a world-caused process without giving organisms concepts such as sex,
fertility, pregnancy, parenthood, heredity, or a scripted reproductive action. The
public observatory must never expose sexual mechanics or sensitive category and
partner detail.

## Decision

- Every ruleset-fourteen organism atomically pins a species-matching reproductive-
  physiology commitment at initialization or birth. It contains maturity,
  development, recovery, opportunity cadence, initiation probability, compatible
  engine categories, weighted offspring categories, evidence provenance, and the
  exact world tick duration used to interpret its tick-valued parameters.
- The current baseline supports two compatible, mature, living organisms occupying
  the same exact embodied patch. This is physiological eligibility, not agent
  knowledge, courtship, kinship, or a social institution.
- Opportunity phase, success, offspring category, pending-development identity, and
  offspring identity derive from versioned domain-separated hashes of world seed,
  simulation tick, and canonically ordered parent identities. Wall time, observer
  activity, payments, model output, and infrastructure timing are absent.
- Eligibility is grouped by exact patch, real species, participation tier, profile,
  and category. Stable adjacent matching within those ordered buckets avoids a
  whole-world all-pairs scan; partition capacity can still pause a dense transition
  before commit without changing which pairs were selected.
- A successful opportunity creates a private pending-development event. The
  developing parent and both parents' next available tick are committed. A due,
  viable development resolves only through an exactly bound ordinary birth event;
  unavailable developing parents resolve through a neutral private end code.
- The scheduler recomputes the complete reproductive suffix for every tick. Missing,
  added, changed, or reordered starts, endings, and births fail before commit. Manual
  births are rejected once this driver is active.
- A causal tick batch contains exactly one `TickAdvanced` event at index zero. A death
  batch contains none. When the last person dies, `WorldExtinct` and `WorldArchived`
  are the exact terminal suffix; the transient extinct state is never snapshot-valid.
- Organism initialization is valid only inside the atomic `WorldStarted` batch. A
  later tick-zero batch cannot disguise arbitrary population injection as genesis,
  and genesis requires at least one person.
- The offspring inherits the real species identity and the developing parent's exact
  source-bound metabolic, regulation, and reproductive commitments. Individual
  heritable variation is not fabricated by this checkpoint and remains a separate
  versioned layer.
- There is no biological or operational population cap. Capacity exhaustion retains
  the existing pause-at-a-committed-boundary behavior.
- Private start/end events and reproductive state are omitted from public timeline,
  organism, and finding projections. A successful birth uses only the existing
  restrained public sentence; category, parentage, location, profile, and mechanism
  remain withheld.
- Event schema sixteen and snapshot/state-hash schema seventeen isolate the boundary.
  Replay consumes committed events and never resamples physiology.
- The supported full-Earth initializer requires one canonical provisional organism
  body-profile plan for later bodily rulesets. It must cover every founder and selected
  fauna taxon, match world tick duration and any separately retained metabolic rate,
  and is pinned by digest in the world manifest before genesis.

## Consequences

Birth is now delayed, causal, replayable, and independent of supporters. The first
profile fixtures are explicit engineering assumptions, not scientific admission. ADR
0049 permits such profiles only in an openly labelled experimental world with their
assumptions pinned and published.
Learned courtship, partner preference, caregiving, asexual modes, litter size, loss
mechanics beyond an unavailable developing parent, and individual genetic variation
remain later rulesets.
