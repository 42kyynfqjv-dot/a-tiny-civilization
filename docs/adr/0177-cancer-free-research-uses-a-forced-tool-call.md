# ADR 0177: Cancer free research uses a forced tool call

Date: 2026-08-30

Status: Accepted

## Context

Cancer World's current exploration policy starts with OpenRouter's maintained
dynamic-free route. Live receipts showed two distinct failure modes before the paid
fallback: some selected endpoints returned null content after consuming the bounded
completion on hidden reasoning, while others copied JSON-Schema vocabulary such as
`additionalProperties` into the answer instead of producing a research contribution.
The adapter correctly rejected both, but each rejection needlessly increased paid
Fireworks usage.

OpenRouter's dynamic router can filter for tool-call support. The adapter already
accepts exactly one tool call named `bounded_cancer_research_contribution` and passes
its arguments through the same deny-unknown-fields deserializer, campaign checks,
citation allowlist, zero-cost check, receipt hashing, and durable attempt boundary used
for message content.

## Decision

- Only the current Cancer `openrouter/free` exploration route advertises one function,
  forces that exact function through `tool_choice`, and omits `response_format`.
- The function carries the full turn-specific schema. The system prompt no longer
  duplicates that schema, preventing a model from copying it as the contribution and
  reducing prompt waste.
- The route requests `reasoning: { effort: "none", exclude: true }` so hidden reasoning
  cannot consume the 1,536-token free-route ceiling before tool arguments are emitted.
- OpenRouter must select an endpoint supporting every requested parameter. Missing,
  renamed, zero, or multiple tool calls; malformed arguments; unknown fields; invented
  citations; non-zero reported cost; and contract-invalid contributions still fail
  closed and continue through the existing deterministic ladder.
- Pinned historical free routes retain their previous prompt-based request shape.
  Paid Fireworks, deterministic screening, campaign scheduling, research memory,
  virtual experiments, canonical world history, and replay are unchanged.
- The adapter contract identifier advances to version 24.

## Consequences

After the provider's daily free quota resets, compatible free endpoints have a more
reliable path to a locally verified contribution and should reduce avoidable paid
fallbacks. Tool forcing cannot make a hypothesis scientifically valid; it changes only
the transport shape. Every accepted artifact continues through duplicate detection,
literature novelty audit, deterministic model testing, and adversarial campaigns under
their existing boundaries.

