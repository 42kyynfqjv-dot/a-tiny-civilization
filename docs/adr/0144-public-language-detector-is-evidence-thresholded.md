# ADR 0144: public language detection is evidence-thresholded

Date: 2026-08-09

Status: Accepted

## Context

Physical signal emissions are common and are not themselves evidence of language. Ruleset 33 can
produce private, socially learned associations between a signal form and a subsequently witnessed
primitive action or movement coordinate, but those associations do not contain words or meanings.
The public wiki needs a reproducible boundary that can recognize durable convergence without
promoting noise or feeding an observer interpretation back into the world.

## Decision

Detector version 2 is a disposable observer projection over committed
`OrganismSignalActionAssociationChanged` events. A signal-form/action pairing becomes a public
proto-lexicon candidate only when all of these conditions hold:

- at least 12 committed supporting evidence events;
- at least four distinct learners;
- at least three distinct signal sources;
- evidence spanning at least 288 simulation ticks (one configured simulation day); and
- the pairing accounts for at least 60 percent of all association evidence for that signal form.

Only person-to-person evidence is eligible. An emission followed by another emission is excluded:
it demonstrates continued signaling, not a referent, and cannot provide either evidence or the
denominator for meaning dominance. The presentation-policy exclusion is handled the same way.

Qualifying conventions with fewer than three **distinct physical meanings** are labelled
`proto_lexicon`. Three or more distinct meanings are labelled only a
`rudimentary_language_candidate`; many signal forms converging on the same behavior therefore do
not satisfy the boundary. The detector makes no claim of grammar, compositionality,
sentience, intention, or human-like language. Observer glosses name only the associated primitive
physical behavior and remain explicitly tentative. Violence-adjacent glosses are withheld under the
presentation policy.

Each entry cites its first and latest canonical event, tick and sequence, evidence count, learner
count, source count, dominance percentage, detector version, and thresholds. The projection and its
wiki/API presentation cannot be imported by the runner, cognition, Hindsight memory, perception,
or action-selection code.

## Consequences

- Repetition alone never becomes language.
- Existing and future worlds can be re-evaluated without a restart or canonical mutation.
- Threshold changes require a new detector/projection version and cannot rewrite earlier research.
- A world may signal forever without producing a qualifying convention.
