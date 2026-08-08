# ADR 0077: Supporter account history is private and minimized

## Status

Accepted on 2026-08-08.

## Decision

The authenticated observer API exposes an oldest-independent, bounded list of only the caller's
supporter reservations. Ownership is derived from the verified account UUID rather than an email or
provider subject. The response contains the requested public label, target, world, birth category,
lifecycle timestamps, match evidence, and coarse refund state.

It deliberately omits the internal supporter subject, Stripe event reference, PaymentIntent ID,
refund ID, and moderation identity. The list requires a valid unexpired session but no CSRF token
because it is read-only. Cancellation remains a separate CSRF-protected state change.

## Consequences

- The eventual account UI can explain pending payment, review, matching, cancellation, and refund
  progress without broad database access.
- One observer cannot enumerate another observer's reservations.
- Payment-provider identifiers and internal moderation evidence do not cross the public API.
