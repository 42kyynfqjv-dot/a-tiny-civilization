# ADR 0172: Episodic memory and saturated social bookkeeping are bounded

Date: 2026-08-30

Status: Accepted

## Context

The ordinary world's canonical perception memory keeps only the latest reading at a
bounded physical address. The Hindsight projection retained that address only on its
first appearance, so later recalls became thousands of ticks stale even while the
world kept observing changes. Separately, social action values are bounded at 128,
but the engine continued emitting one event per observation after saturation. Those
events changed only an unused observation counter and had become a major share of
both worlds' history volume.

The public language detector also exposed only its current rolling result. A real
threshold crossing could therefore disappear from the public record when old
evidence left the window. Habitat communication stored compositional associations
but discarded their preceding physical form.

## Decision

- Ruleset 45 stops emitting a social-value transition when the observed action's
  existing social value is already 128. Action selection reads the value, not the
  observation count, so the organism's behavior is unchanged. Unsaturated values
  continue to learn and emit their exact existing transitions.
- The running ruleset-42 ordinary world activates this change at tick 6,000. The
  same rule activates for the running ruleset-38 Cancer World at tick 35,000 because
  its saturated representation has the identical state-neutral invariant. Cancer
  biology and research scheduling are otherwise untouched.
- Ruleset 45 replaces first-address-only external retention with a deterministic
  episodic projection for ordinary people. It considers new addresses, changed
  acoustic forms, materially changed readings, and periodic refreshes. Candidate
  ordering is canonical and tick-scoped.
- Episodic delivery retains at most one item per person and eight items across the
  whole transition. The global bound is independent of population size. Cancer
  World's collective research memory and experiment worlds are excluded.
- Public language detector version 6 reports both `current_stage` and the durable
  highest `stage` ever attained. Milestones are recorded in the same projection
  transaction that evaluates their evidence. Evaluation occurs on new evidence
  and on the exact ticks where prior evidence changes window half or expires, so
  a quiet-tick crossing is not missed. Its rolling window is anchored to the
  projected canonical tick, not the most recent language event.
- Habitat communication retains and returns the original ordered signal sequence.
  Historical atomic rows remain one-element sequences; compositional associations
  expose both physical forms. A database reconciliation trigger covers either
  ordering of the independently checkpointed habitat and language projections.
- Public memory views accept both the strict legacy direct-observation payload and
  the strict episodic-v2 payload, so the observer stream does not freeze when the
  new retention policy activates.

## Replay and activation

Ruleset-42 history before tick 6,000 and Cancer ruleset-38 history before tick 35,000
retain their original counter-only events and hashes. New ruleset-45 ordinary worlds
use suppression and episodic memory from genesis. The event schema and snapshot shape
do not change; only post-boundary transition selection changes.

Hindsight retention and observer projections remain one-way effects outside canonical
state. Replaying history never contacts Hindsight and never recomputes a provider
response. Retrying the same uncommitted transition produces the same bounded retain
identities and ordering.

## Consequences

Later cognition can retrieve recent direct episodes without multiplying the outbox by
population or sensory width. Saturated social learning stops dominating event volume
while preserving action weights. Public observers can distinguish a historical
language milestone from present evidence and can see inhabitant-produced ordered
calls without the projection flattening them. Language evidence and milestones are
sequence-bounded by the cursor returned with the same response, preventing a
concurrent projector commit from leaking future evidence under an older cursor.

Detector milestones begin when version 6 is deployed. Migration 0058 carries forward
the one version-5 proto-lexicon crossing documented for the ordinary world, but only
when the immutable evidence still matches its exact world, tick range, two-form
meaning, counts, participants, dominance, persistence, and lift signature. It is a
no-op for every other world or evidence shape; no broad historical inference occurs.
