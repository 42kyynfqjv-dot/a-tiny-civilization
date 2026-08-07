# ADR 0052: Cognition selection is canonical and recall is locally re-admitted

## Status

Accepted and implemented on 2026-08-07. Ruleset 16 request selection, local Hindsight
recall admission, provider execution, immutable deadline latching, and canonical
result-or-absence consumption are exercised end to end.

## Context

A model request prepared from runner state would let infrastructure choose who thinks,
what the model sees, and how long it has to answer. Accepting Hindsight metadata at
face value would also let an external index inject content that was never delivered
from canonical history. Either path would make replayable history untrustworthy.

## Decision

- Ruleset 16, event schema 18, snapshot schema 19, and state-hash schema 19 own
  external-cognition request selection and pending deadline state.
- `CognitionRequestSelected` is a causal event. It is not a projection effect. Its
  request identity is derived from world, organism, selected tick, and fixed ordinal.
- Only one world-total request may be pending. The caller selects a living organism;
  the ruleset fixes every other field: a 60-tick simulated-time response window, the
  current bodily pressures and action values, at most the 32 most recent direct
  readings restored to canonical address order, a use-neutral recall query, 512 recall
  tokens, and 32 model-output tokens.
- Applying a selection reconstructs those inputs from canonical state and rejects any
  mismatch. Observer text, supporter state, wall time, provider availability, and
  arbitrary runner prompts cannot enter the selection.
- Hindsight recall remains an untrusted external result. Before recalled text can be
  used for cognition, PostgreSQL joins each document to a successfully delivered local
  memory-outbox row and exactly compares world, life, bank, document identity, source
  sequence/tick/ordinal, content, and context. Results are deterministically ordered,
  deduplicated, and capped.
- Canonical recall outcomes contain only normalized bounded fields plus hashes. Raw
  provider JSON and errors remain outside world history.
- A paid model response that omits explicit provider-reported cost is invalid; missing
  cost is never interpreted as zero on the paid route.

## Consequences

Selection, snapshots, genesis replay, and deadline fallback are deterministic before
any credential or worker exists. A forged prompt or recalled document fails closed.
Valid model output can add only a fixed preference bonus to an already legal primitive
action; it cannot invent an action, fact, object, concept, or state mutation.
