# ADR 0063: The web edge owns browser security and live cache policy

## Status

Accepted on 2026-08-08.

## Context

Cloudflare reaches only the observatory web process. That process renders pages and
proxies the read-only observer API, making it the consistent browser security boundary.
Live world pages or JSON cached as static content could falsely appear stalled, and a
future authenticated observer response must never become shared cache content.

## Decision

Every web response receives a restrictive same-origin content-security policy,
frame denial, MIME sniffing denial, strict referrer and feature policies, HSTS,
cross-origin opener/resource isolation, origin-agent clustering, and legacy
cross-domain-policy denial.

Observer API responses and dynamic documents receive `Cache-Control: no-store`.
Content-hashed framework assets retain their generated immutable caching; optimized
images retain their own bounded response policy. The observer API remains read-only
and reachable from the public internet only through this web proxy.

The rendered-worker test exercises both a document and a proxied API response and pins
the critical headers. Cloudflare may add stricter edge protections but must not remove
or relax the origin policy.

## Consequences

- The live observatory favors freshness over HTML edge caching.
- Public telemetry polling cannot be served as a stale shared API object.
- Inline script/style allowances remain narrowly required by the current React/vinext
  build; removing them requires a tested nonce or hash pipeline, not a production edit.
