# ADR 0155: The language detector uses recent persistent evidence

## Status

Accepted and implemented on 2026-08-13 as observer detector version 4. It changes no
canonical state and requires no world restart.

## Context

Detector version 3 accumulated every positive association since genesis. Early random
exploration could therefore dilute a later convention forever. It also returned an
empty archive until every convention threshold passed, hiding real attempts and making
an active world look silent.

## Decision

- Evaluate only the latest 1,152 simulated ticks, four times the minimum 288-tick
  evidence span.
- Retain the existing event, learner, source, dominance, background-margin, and lift
  gates.
- Require evidence and at least 55 percent dominance in both halves of the rolling
  window before publishing a stable convention.
- Publish at most five deterministic `emerging_patterns`: the strongest behavioral
  mapping per form with at least four events, two learners, and two human sources.
  Each reports how many gates pass and whether recent consistency is strengthening,
  stable, or weakening.
- Emerging patterns are explicitly not dictionary entries or language. The projection
  cannot feed them back into the world.

## Consequences

Later convergence can become visible without erasing genesis evidence, while a short
burst cannot be mislabeled durable. Visitors can see that inhabitants are learning
even when the scientifically conservative language stage remains `undetected`.
