# ADR 0066: Checkout creation is server-side and reservation-idempotent

## Status

Accepted on 2026-08-08.

## Context

A browser must not choose the price, create its own payment evidence, or manufacture the metadata
that later binds a payment to a reservation. Network ambiguity can also cause a caller to retry a
Checkout request after Stripe created the first session.

## Decision

The isolated Stripe adapter creates hosted one-time Checkout sessions with a configured Stripe Price
ID. It sends quantity one, success/cancel URLs fixed by server configuration, and the reservation UUID
as client reference, Checkout metadata, and PaymentIntent metadata. The Stripe secret is used only as
a bearer credential and is redacted from `Debug`.

Every request uses `atiny-checkout-{reservation_id}` as its Stripe idempotency key. Reservation
creation itself is idempotent only when every immutable request field matches; reuse of a reservation
UUID with changed world, account subject, label, target, species, or birth category is a conflict.

This checkpoint deliberately exposes no public Checkout route. A later route must first derive the
supporter subject from a verified server-side account session and must persist the returned Checkout
session against the reservation. It may not accept a subject, Price ID, amount, currency, or return
URL from the browser.

## Consequences

- Ambiguous network retries cannot intentionally create a second Checkout session within Stripe's
  idempotency window.
- Wallet availability, including Apple Pay and Google Pay, remains Stripe-hosted presentation rather
  than simulation or payment-evidence logic.
- Live account credentials are not needed for local contract tests.
