# ADR 0171: Ordinary cognition quarantines demonstrated-dead routes

Date: 2026-08-30

Status: Accepted

## Context

Live ordinary-world telemetry showed that three admitted routes consumed ladder
positions without producing a usable receipt: both pinned OpenRouter GPT-OSS free
slugs were consistently rejected, and the local `gpt-oss-20b` model was not
installed. OpenRouter's maintained dynamic-free route also returned outputs that
could not consistently satisfy provider-side JSON-schema negotiation, even though
the adapter already enforces the closed action object locally. The CPU Qwen route
occasionally succeeded but its 45-second circuit breaker was shorter than observed
prefill time.

Removing or reordering routes would make durable, partially completed version-two
attempt prefixes disagree with the worker registry. Leaving them active would keep
repeating a known failure.

## Decision

- Ordinary route policy version three retains the exact version-two ordered route
  list and adds a canonical `quarantined_routes` list. It marks the pinned
  `openai/gpt-oss-20b:free`, pinned `openai/gpt-oss-120b:free`, and uninstalled local
  `gpt-oss-20b` routes as `skipped_disabled` without dispatching them.
- The quarantine is part of the registry's canonical hash. It is fixed by validation
  for ordinary production and development registries; callers cannot silently add or
  remove entries. Cancer research registries carry no quarantine and are unchanged.
- Version-two ordinary registries remain reconstructible by policy version. Their
  empty quarantine field is omitted during serialization, preserving the legacy
  canonical representation. Because version three does not change route identities
  or order, a durable attempt prefix created before deployment remains valid.
- The ordinary `openrouter/free` request no longer asks OpenRouter to negotiate
  `response_format`. It still asks for the same closed JSON object, rejects prose or
  unknown fields in the local typed parser, records the selected model when reported,
  and records an explicit uncertainty marker if the dynamic router omits it.
- The adapter contract identifier advances to version 17. The ordinary per-request
  circuit breaker becomes 180 seconds. Production preflight budgets one Hindsight
  recall plus all sixteen permitted network attempts inside the 60-tick deadline and
  requires the 3,600-second claim lease to outlive that bound.

## Replay and activation

Completed version-two results and canonical cognition deadline inputs remain
immutable and replay never contacts a model. Unfinished requests do not currently
persist a route-policy version; after deployment they complete under version three.
Their existing durable route attempts remain an exact prefix because the ordered
route list is unchanged. New results record the version-three registry hash and the
explicit disabled skips.

This is an ordinary-world worker policy deployment. It is not coupled to a simulation
ruleset activation tick and does not alter deterministic fallback behavior.

## Consequences

Known-dead routes cannot consume network time, the maintained dynamic-free route can
use models that lack provider-side schema support without relaxing the local safety
contract, and the viable CPU route has time to finish. The longer wall timeout does
not delay canonical history: the fixed simulation deadline still admits the recorded
result or the deterministic unavailable outcome exactly once.

## Live-probe amendment: disable reasoning for the bounded free action

A later fixed synthetic probe returned a successful OpenRouter envelope with null
message content. OpenRouter documents that legacy `include_reasoning: false` only
excludes reasoning from the returned message; it does not disable reasoning. With
the ordinary action contract's deliberately tiny output allowance, a randomly
selected reasoning model can therefore consume the completion without emitting a
final motor action.

Adapter contract version 19 replaces that legacy flag only for the ordinary
`openrouter/free` route with `reasoning: { effort: "none", exclude: true }`, using
OpenRouter's documented unified control:
<https://openrouter.ai/docs/guides/best-practices/reasoning-tokens>.
Cancer research keeps its separate reasoning policy. The request still omits
provider-side schema negotiation, and the same closed local action parser, zero-cost
check, provider identity, response hash, deadline, and replay rules remain unchanged.

## Live-probe amendment: request router-filtered JSON mode

The version-19 canary returned final message content, proving the reasoning control
repaired the null-content failure, but the randomly selected free model answered in
prose and the local typed parser correctly rejected it. OpenRouter now documents that
the free router filters its candidate pool for requested capabilities, including
structured output. Adapter contract version 20 therefore requests the broadly
supported `response_format: { type: "json_object" }` mode for ordinary
`openrouter/free` calls. It deliberately does not delegate the exact action schema to
the provider: the local deny-unknown-fields parser remains the authoritative safety
boundary. Cancer research routing and all canonical simulation rules remain
unchanged.

Provider capability reference: <https://openrouter.ai/openrouter/free>.

## Live-probe amendment: force one typed tool call

Two version-20 canaries returned JSON-mode content that still failed the exact local
action contract. Adapter contract version 21 replaces JSON mode on only the ordinary
dynamic-free route with one forced `select_bounded_primitive_action` function call,
disables parallel calls, and accepts exactly one call with that name. OpenRouter
documents both forced tool choice and standardized tool-call arguments; the dynamic
router can therefore exclude models without tool support. The returned argument
string still passes through the same deny-unknown-fields `BoundedAction` parser and
all receipt, zero-cost, deadline, input-log, and replay checks. A malformed, missing,
renamed, or multiple tool call remains an unavailable cognition result and cannot
alter canonical history.

Tool-calling reference:
<https://openrouter.ai/docs/guides/features/tool-calling>.
