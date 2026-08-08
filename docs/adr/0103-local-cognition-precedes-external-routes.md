# ADR 0103: Loopback cognition precedes external routes

Accepted on 2026-08-08.

## Context

External cognition receives private direct perceptions and recalled life-local memory. The worker
correctly requires explicit export approval whenever an external adapter is configured, but that
boundary should not prevent a real model running on the same host from serving the world at zero
network disclosure and zero provider cost.

## Decision

The production route registry begins with `local_openai/qwen2.5:1.5b`, followed by
`local_openai/gpt-oss-20b`, as free routes. The first is an Apache-2.0, 986 MB quantized model that
fits the current CPU host and is used only for the tiny strict JSON action choice; the second is the
preferred larger local route when future hardware can sustain it. Operators may
configure it only through `LOCAL_COGNITION_BASE_URL`. A host-run worker accepts uncredentialed
`http` URLs whose host is exactly `127.0.0.1`, `::1`, or `localhost`; the production Compose worker
also accepts only the exact internal service authority `local-cognition:11434`. Credentials,
queries, fragments, HTTPS, redirects, and every other host are rejected. The service has no
published port in Compose and shares only the private backend network. The adapter sends a fixed non-secret placeholder bearer value for
OpenAI-compatible servers that ignore authentication. HTTP redirects are disabled so a loopback
server cannot redirect private context off-host.

Loopback adapters are excluded from the external-provider count and therefore do not require
`COGNITION_EXTERNAL_EXPORT_APPROVED`. Every genuinely external adapter retains that fail-closed
requirement. The local route uses the same strict request, JSON response schema, zero-cost receipt,
deadline, immutable latch, and replay boundary as external routes.

## Consequences

A locally hosted compatible Qwen2.5 1.5B or GPT-OSS 20B service can close model-path qualification without sharing
world state or Hindsight memories with another party. If no local service is configured or it is
unavailable, the ladder proceeds to configured approved free providers and finally the separately
authorized paid tail; deterministic local behavior remains the terminal fallback.
