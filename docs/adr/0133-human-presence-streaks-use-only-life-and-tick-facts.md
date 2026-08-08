# ADR 0133: Human-presence streaks use only life and tick facts

## Decision

`public-finding-v2` rebuilds findings from sequence zero and emits a human-presence
streak at ticks `100`, `1,000`, `10,000`, and each later power-of-ten multiple of
one hundred. A milestone is emitted only when the same committed batch advances to
that exact tick and projection-local introduction/ending facts show at least one
living person after the entire batch is applied.

The source is the batch's `TickAdvanced` event. The finding contains no organism
identity, location, reproductive detail, mortality mechanism, cognition fact, or
observer label. It states only that at least one person remained present through the
recorded tick. It never describes persistence as a settlement, tradition, culture,
achievement, intention, or historical importance.

## Consequences

Quiet histories gain sparse, factual return points without narration or a hidden
significance model. The logarithmic cadence stays bounded over arbitrarily long runs.
Changing from v1 to v2 gives existing archives an explicit rebuild cursor; v1 rows
remain immutable evidence and are not rewritten. More specific behavioral streaks
still require canonical events that directly establish their public-safe facts.
