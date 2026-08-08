# ADR 0071: Browser auth and Checkout are cookie-, CSRF-, and account-bound

## Status

Accepted on 2026-08-08.

## Decision

The observer API owns the complete browser composition around the provider-neutral account store.
OAuth attempts use ten-minute HTTP-only browser-binding and PKCE cookies. Production cookies use
the `__Host-` prefix, `Secure`, `Path=/`, and `SameSite=Lax`. A verified callback atomically consumes
the attempt once, creates independent session and CSRF secrets, persists only their digests, and
issues a 30-day HTTP-only session cookie plus a readable same-origin CSRF cookie.

Every state-changing account route requires the CSRF value to match both the cookie and request
header, then authenticates both digests in one database query. Checkout additionally derives the
supporter identity from that durable account. It is enabled only when Google sign-in, the fixed
Stripe product, and signed Stripe webhook admission are configured together. Missing integration
configuration leaves the routes disabled rather than partially functional.

HTTP tracing records request paths but never query strings, preventing OAuth authorization codes
and state values from entering routine access logs.

## Consequences

- JavaScript cannot read the session credential, while a cross-site request cannot prove CSRF.
- Duplicate cookie names, malformed secrets, expired sessions, consumed attempts, and partial
  production configuration fail closed.
- Checkout cannot collect payment unless the process can also admit its signed completion webhook.
- These accounts, sessions, reservations, and payments remain observer-side projections and never
  influence births or canonical simulation behavior.
