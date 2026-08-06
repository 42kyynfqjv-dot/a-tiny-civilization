# ADR 0020: Observer finding aids are deterministic and evidence-bounded

## Decision

`public-finding-v1` consumes only checksum-verified, ordered committed batches. It
stores immutable public findings with exact source event, sequence, tick, and
`world_fact` provenance. It currently recognizes factual first occurrences and
population records. Population state is projection-local and derives only from
individual introduction and ending events.

`Streak` is part of the versioned finding vocabulary but has no emitted findings yet.
The current canonical event grammar cannot establish repeated co-action, persistence
at a site, or retained-object history. Emitting a streak before that evidence exists
would be editorial invention, not a finding.

## Consequences

The site can surface a useful, auditable signal without an LLM narrator or a hidden
importance heuristic. Later behavioral events can add deterministic streak detectors
under a new projection version without rewriting these findings or world history.
