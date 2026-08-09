# Ruleset-32 public observatory admission v3 — 2026-08-09

The observer-facing surface at source commit
`e856bfbf9f975ec60b9fc0526e4ee3d651a7173e` is accepted for the ruleset-32 experimental world.
This succeeds v2 after correcting canonical HTTPS detection behind the existing Cloudflare Tunnel.

The edge now recognizes Cloudflare's visitor-scheme signal and `X-Forwarded-Proto` on the
loopback-only trusted origin hop, preventing a same-URL redirect loop after TLS termination while
retaining the canonical plaintext redirect and its method-preserving 308 behavior. Ten rendered
edge tests, including both the direct-HTTP and tunneled-HTTPS cases, plus the complete web build
and lint pass. All v2 experience, provenance, policy, and supporter-isolation findings remain
unchanged.

The adjacent canonical JSON binds the review to the exact `web/` and `docs/policies/` trees and to
quality-world admission SHA-256
`1f6750a373c4d3029638091073361860ed974b3ebde7195510e986579aafd358`.
