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
