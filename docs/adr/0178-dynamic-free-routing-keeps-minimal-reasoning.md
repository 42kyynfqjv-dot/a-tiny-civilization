# ADR 0178: Dynamic free routing keeps minimal reasoning

Date: 2026-08-30

Status: Accepted

## Context

Adapter version 24 disabled reasoning on OpenRouter's dynamic free routes so a
hidden trace could not consume the complete bounded response before a forced tool
call. Live Cancer World traffic then exposed a capability mismatch: the router can
select a tool-capable model whose reasoning is mandatory, and that endpoint rejects
`reasoning.effort: none` with HTTP 400. OpenRouter's current documentation says that
dynamic router entries do not expose one fixed reasoning capability and that models
with mandatory reasoning must not receive `none`.

## Decision

- Both dynamic `openrouter/free` routes request `reasoning.effort: minimal` and
  `reasoning.exclude: true`. This is the smallest portable effort and does not ask a
  mandatory-reasoning endpoint to disable its required capability.
- The ordinary route retains its signed 64-token ceiling and forced one-action tool
  call. The Cancer route retains its signed 4,096-token ceiling but raises only its
  provider-side dynamic-free allowance from 1,536 to 2,560 tokens, leaving room for
  minimal reasoning plus the historically observed contribution tail.
- Provider-reported completion usage still includes reasoning and must remain within
  the signed request ceiling. Zero-cost validation, exact tool names, closed local
  deserialization, campaign validation, citation checks, deadlines, durable route
  attempts, and replay behavior are unchanged.
- Pinned routes keep their model-specific reasoning controls. The adapter contract
  identifier advances to version 25.

## Consequences

The dynamic router may choose either a reasoning or non-reasoning tool-capable free
model without receiving a contradictory disable request. A model can still time out,
hit the shared free quota, truncate, or return invalid arguments; each case continues
through the existing recorded ladder and cannot alter history without a valid receipt.

OpenRouter reasoning reference:
<https://openrouter.ai/docs/guides/best-practices/reasoning-tokens>.
