# ADR 0099: Signal-action association is private and temporal

Accepted on 2026-08-08 as ruleset 22.

## Context

Ruleset 21 provides varied physical sounds, but variation alone cannot become a convention unless
an organism can retain a relationship between what it heard and what it later witnessed. Encoding
messages, meanings, referents, vocabulary, or communicative goals would put language into the world
instead of allowing it to emerge.

## Decision

At most once per tick, the existing bounded social-attention process may create one private
association for an observer when all of these physical facts hold:

- on the preceding committed tick, the observer directly heard amplitude 1 through 8 from the
  selected co-located actor;
- on the current tick, the observer directly witnesses that same actor's primitive action; and
- both facts are already available through canonical local perception and attention rules.

The retained address is only `(signal_intensity, primitive_action_kind)`, with a bounded observation
count and positive tendency. It contains no actor relationship, word, message, intention, success,
object, material, use, or observer interpretation. The update is private and discarded by every
public projection.

When an organism has just heard an amplitude, prior associations for that amplitude weakly modify
its next action weights. Exploration remains nonzero, an association cannot create an action, and
the lowest stable source identity resolves simultaneous local sounds without host ordering.

## Version and enforcement

Ruleset 22 uses event schema 23 and state/snapshot schema 24; the deterministic policy draw advances
to version 7. Ruleset 21 and all earlier histories retain their bytes and behavior. Newborns start
without associations and learned state is not inherited.

Planning and commit independently reconstruct the selected actor, preceding direct sound, exact
association address, arithmetic, and at-most-one update. Missing, duplicate, self-directed,
nonlocal, wrong-amplitude, wrong-action, reordered, or fabricated updates fail before commit.
Replay consumes only recorded events. Observer projections explicitly discard the private update.

## Consequences

This is a substrate for a convention, not proof of communication. Random correlations may fade in
relevance while repeated situated correlations can influence behavior. Whether anything stable or
language-like arises remains history.
