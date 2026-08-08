# ADR 0064: Supporter payment evidence enters through a strict Stripe webhook boundary

## Status

Accepted.

## Context

Browser redirects are not payment evidence. Stripe can retry deliveries, deliver events out of
order, and temporarily sign one delivery with more than one active endpoint secret. Parsing JSON
before signature verification also changes the byte sequence Stripe signed.

Supporter labels are observer-only, but false payment admission would still create a financial and
moderation integrity failure. The simulation runner must remain unable to import payment code.

## Decision

The isolated `stripe-adapter` verifies the `Stripe-Signature` HMAC over the exact raw request body
before parsing JSON. It requires a positive timestamp tolerance, accepts any valid `v1` signature
during secret rotation, and rejects stale deliveries. It then admits only configured test or live
mode, exact minor-unit amount, exact lowercase currency, paid Checkout sessions, and a valid
reservation UUID in Checkout metadata.

The only entitlement-producing event types are `checkout.session.completed` and
`checkout.session.async_payment_succeeded`. Other correctly signed events and unpaid Checkout
sessions are acknowledgeable but cannot produce an entitlement. No API secret or webhook secret is
stored in source control or emitted through `Debug`.

This adapter is outside `application`, `sim-engine`, `world-domain`, and the runner. A later HTTP
composition may call it and an observer-side transactional payment store; canonical world history
may never depend on its result.

## Consequences

- A success redirect can never activate a reservation.
- Body mutation, cross-environment events, product substitution, and old signed replays fail closed.
- Stripe endpoint setup remains an external-account handoff, but its credential is not needed to
  finish or test the integration locally.
- Durable duplicate detection is a persistence concern layered after this pure verifier.
