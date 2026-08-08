# ADR 0088: Provider contract probes are fixed and synthetic

## Status

Accepted on 2026-08-08.

## Decision

`civilization-runner probe-openrouter-free` exercises the real OpenAI-compatible adapter and
OpenRouter dynamic free route using one code-defined synthetic request. The command does not accept
a database URL, read a world, contact Hindsight, or include perceptions, learned values, memories,
or non-default bodily state. Its synthetic IDs and request bytes are stable across invocations.

The probe applies the production response schema, resolved-model recording, usage bounds, and free
route cost check. It prints only the resolved model, token counts, and billed micro-USD; it does not
print credentials, provider response identifiers, or raw response content. This connectivity probe
does not grant or require approval to export live cognition under ADR 0087.

## Verification

The first live probe used OpenRouter's `openrouter/free` route. It resolved to
`google/gemma-4-26b-a4b-it:free`, returned a valid bounded action, reported 412 prompt tokens and 8
completion tokens, and passed the exact zero-cost check. Unit tests prove the command is
database-free and that its request contains no readings, action values, recalled memories, or
non-default bodily needs.

## Consequences

- Provider credentials and protocol compatibility can be checked without disclosing civilization
  history.
- A successful probe does not qualify live cognition semantics or authorize external export.
- A free router changing its resolved model remains visible in the probe evidence and in any later
  immutable live receipt.
