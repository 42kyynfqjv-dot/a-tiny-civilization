# ADR 0074: Supporter refunds are durable, terminal, and idempotent

## Status

Accepted on 2026-08-08.

## Decision

The signed Stripe webhook ledger retains the PaymentIntent ID for every newly admitted payment.
An operator-only command may prepare a full refund only after the reservation reaches a compatible
terminal state: moderation rejection, supporter cancellation, or world expiry. Duplicate-charge
and service-failure reasons may be used across those terminal states. Matched and active
reservations fail closed.

Preparation commits an immutable refund intent before Stripe is contacted. Stripe receives the
PaymentIntent ID, reservation and reason metadata, and the stable idempotency key
`atiny-refund-{reservation_id}`. Its validated refund ID is then recorded exactly once. Retrying
after a process or network failure safely resumes either phase. Historical payments created before
PaymentIntent retention fail closed for manual review.

The workflow is a command-line operation, not an HTTP route. It belongs entirely to the observer
and payment side and cannot write canonical history.

## Consequences

- A crash after Stripe accepts a refund does not create a second refund on retry.
- Refund reason, payment identity, request time, and completion evidence cannot be deleted or
  rewritten.
- The operator cannot automatically refund a pending, active, or matched reservation by mistake.
- The Stripe secret stays in the operator process and is redacted from debug output.
