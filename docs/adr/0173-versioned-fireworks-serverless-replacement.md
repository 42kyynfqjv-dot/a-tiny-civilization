# ADR 0173: Versioned Fireworks serverless replacement

Status: accepted

## Context

Cancer research exploration policy 13 ended with the pinned Fireworks model
`accounts/fireworks/models/gpt-oss-20b`. Fireworks' current model catalog marks
that model as unavailable on serverless inference, and every live attempt was
returning HTTP 404. A paid fallback that can never execute silently reduces
research throughput while still consuming a durable route attempt.

The current serverless catalog offers
`accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b` at USD 0.05 per
million input tokens and USD 0.20 per million output tokens. A credentialed
smoke test confirmed the exact model identifier and strict JSON-schema output.
The model must receive `chat_template_kwargs.enable_thinking=false`; without
that model-native switch it can spend the bounded completion on a prose
reasoning preamble rather than the required JSON object.

## Decision

- Cancer research exploration policy 14 replaces only the paid tail with the
  pinned Nemotron serverless route.
- Policy 13 remains reconstructable byte-for-byte with the historical GPT-OSS
  route. Existing requests and receipts are never rewritten.
- Model-adapter version 17 sends the non-thinking template flag only for the
  new Nemotron route and retains the strict local response validator.
- Cost receipts derive from the exact requested model: the historical tariff
  remains USD 0.07/M input and USD 0.30/M output; the new tariff is USD 0.05/M
  input and USD 0.20/M output. Cached input is conservatively charged at the
  full input rate.
- Fireworks reconciliation schema 1 remains valid only for historical GPT-OSS
  evidence. Schema 2 admits either exact model, matches CSV rows by both model
  identity and timestamp, and recomputes the model-specific tariff in both the
  application and database trigger.
- Both routes remain inside the existing monthly Cancer research treasury hard
  stop. This change does not raise the budget.

## Verification

- A minimal live call to the exact serverless model returned the required JSON
  object when thinking was disabled.
- Unit tests cover payload flags, strict output, model identity fallback,
  model-specific prices, legacy schema compatibility, and simultaneous CSV
  rows for the two models.
- PostgreSQL migration 0061 admits the new route while retaining append-only
  reconciliation and fail-closed cardinality checks.

## Consequences

The paid fallback can execute again at a lower unit tariff. Model availability
is still external and can change; any future replacement requires another
route-policy and adapter version rather than mutating this decision.
