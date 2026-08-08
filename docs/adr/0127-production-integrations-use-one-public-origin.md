# ADR 0127: Production integrations use one exact public origin

## Status

Accepted on 2026-08-08.

## Context

Pairing an OAuth client ID with a secret or a Stripe key with a Price ID proves only that a
configuration is syntactically complete. A copied staging callback, attacker-controlled redirect,
test-mode payment key, or mistyped public hostname can still pass that shallow check and make a
production deployment unsafe or misleading.

## Decision

- Configured Google and Apple sign-in must use their exact HTTPS callback paths at
  `atinycivilization.com`. Production preflight rejects every other callback before Compose starts.
- Configured Stripe Checkout must use a structurally live secret key, signed-webhook secret, and
  Price ID; `STRIPE_LIVE_MODE` must be true.
- Stripe success and cancellation redirects must be the exact HTTPS routes at the same public
  origin. Currency remains configurable as a lowercase ISO-style three-letter code, and the
  positive amount remains bounded to seven decimal digits.
- When accounts and payments are absent, these checks do not force them on. The read-only
  observatory can launch without either integration.
- The API still verifies OAuth state, nonce, PKCE, browser binding, exact redirect identity, signed
  Stripe webhook bytes, event mode, and fixed product details at runtime. Static preflight is an
  additional deployment boundary, not a replacement.

## Consequences

Production credentials cannot silently activate a staging or foreign-domain flow. Test-mode Stripe
work remains possible in a non-production environment, while a public production deployment is
unambiguous about whether real supporter purchases are enabled.
