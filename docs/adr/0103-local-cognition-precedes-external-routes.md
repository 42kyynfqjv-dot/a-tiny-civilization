# ADR 0103: Loopback cognition precedes external routes

Accepted on 2026-08-08.

## Context

External cognition receives private direct perceptions and recalled life-local memory. The worker
correctly requires explicit export approval whenever an external adapter is configured, but that
boundary should not prevent a real model running on the same host from serving the world at zero
network disclosure and zero provider cost.

## Decision

The production route registry begins with `local_openai/gpt-oss-20b` as a free route. Operators may
configure it only through `LOCAL_COGNITION_BASE_URL`. The runner accepts uncredentialed `http` URLs
whose host is exactly `127.0.0.1`, `::1`, or `localhost`; credentials, queries, fragments, HTTPS, and
non-loopback hosts are rejected. The adapter sends a fixed non-secret placeholder bearer value for
OpenAI-compatible servers that ignore authentication. HTTP redirects are disabled so a loopback
server cannot redirect private context off-host.

Loopback adapters are excluded from the external-provider count and therefore do not require
`COGNITION_EXTERNAL_EXPORT_APPROVED`. Every genuinely external adapter retains that fail-closed
requirement. The local route uses the same strict request, JSON response schema, zero-cost receipt,
deadline, immutable latch, and replay boundary as external routes.

## Consequences

A locally hosted compatible GPT-OSS 20B service can close model-path qualification without sharing
world state or Hindsight memories with another party. If no local service is configured or it is
unavailable, the ladder proceeds to configured approved free providers and finally the separately
authorized paid tail; deterministic local behavior remains the terminal fallback.
