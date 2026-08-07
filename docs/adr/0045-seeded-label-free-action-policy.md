# ADR 0045: Baseline action selection is seeded, situated, and label-free

## Status

Accepted on 2026-08-07. Ruleset-eleven implementation follows this contract.

## Context

The early embodied driver cycles through four actions. That proves scheduling and
replay, but it is a test cadence rather than an organism policy: every life repeats the
same small pattern and bodily needs cannot change exploration. A replacement must
produce diverse action histories without introducing a technology tree, object uses,
observer influence, wall-clock randomness, or an external-model dependency.

## Decision

- Ruleset eleven chooses one primitive action from a deterministic weighted candidate
  set derived only from the world seed, organism identity, simulated tick, durable
  age and bodily pressure, embodied position, and physically local entity identities.
- Candidates contain only the closed bodily grammar. They never contain food, water,
  tool, weapon, shelter, writing, invention, relationship, or cultural labels.
- Energy and hydration pressure increase the weight of undirected oral/material
  exploration only when an object is already locally reachable. Fatigue increases the
  weight of rest. A drive changes exploration probability; it never identifies a
  solution or guarantees relief.
- Object candidates are selected from canonical identity order. Until the scheduler
  has a general collision resolver, only the lowest living organism identity at one
  exact patch may initiate an unheld-object transition in that tick. Existing holders
  may still act on what they hold. This is a disclosed deterministic conflict rule,
  not ownership knowledge inside the world.
- The selected action and all resolved physical effects are ordinary canonical events.
  Replay consumes those events and never samples again. Observer load, page views,
  supporter state, infrastructure latency, and model output cannot enter the stream.
- This baseline policy is not cognition completion. Learned policies, imitation,
  beliefs, and bounded external cognition may later propose candidates through their
  own versioned and recorded boundary.

## Consequences

Lives can diverge and respond to bodily state while remaining bit-for-bit replayable.
The policy can attempt biting, chewing, or swallowing without knowing that a material
is beneficial. Material ingestion, toxicity, and learned action-value updates remain
separate causal checkpoints.
