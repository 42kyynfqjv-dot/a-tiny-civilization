# ADR 0098: Acoustic variation precedes meaning

Accepted on 2026-08-08 as ruleset 21.

## Context

Local emitted signals have been replayable physical sound since ruleset 9, but the autonomous
policy emitted only amplitude one. A channel with no selectable variation cannot support a learned
convention. Adding words, tokens, messages, intended meanings, or a language objective would script
the outcome that the experiment is meant to leave open.

## Decision

Ruleset 21 gives `emit_signal` eight otherwise-equivalent intensity candidates, numbered one
through eight. These are bounded motor magnitudes using the existing physical action intensity and
`signal_amplitude` sound perception. They are not phonemes, words, symbols, meanings, or messages.
Recipients retain the same direct source-bound numeric perception already admitted by ruleset 9.

A successful bounded cognition result may optionally choose `signal_intensity` from one through
eight, only when it chooses `emit_signal`. The ordinary cognition bonus then applies to that exact
candidate. Cognition cannot invent a ninth value, create a signal when emission is unavailable,
address a recipient, or write meaning into history. A missing value retains the legacy
action-kind-wide bias.

## Version boundary

The policy draw version advances to six and new provisional worlds default to ruleset 21. Event,
state, and snapshot structures do not change: action intensity and sound amplitude were already
canonical fields, so ruleset 21 continues to use schema 22 events and schema 23 state/snapshots.
Ruleset 20 replay and hashes are unchanged.

The model adapter advances to `openai-compatible-bounded-cognition-v3`; historical receipts omit
the optional field and retain their exact bytes. Domain and application validation reject zero,
values above eight, or a signal intensity paired with another primitive.

## Consequences

The world now contains physical acoustic variation that memory and later learning may associate,
but the engine still contains no vocabulary or interpretation. Whether repeated magnitudes become
conventional is an outcome. More realistic frequency, duration, hearing, attenuation, and
species-specific production remain source-backed later mechanics rather than hidden assumptions.
