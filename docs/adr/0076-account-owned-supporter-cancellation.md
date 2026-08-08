# ADR 0076: Supporter cancellation is account-owned and refund-coupled

## Status

Accepted on 2026-08-08.

## Decision

An authenticated observer may cancel only an unmatched reservation whose immutable supporter
subject names that observer account. The HTTP operation requires the hardened session plus the
same double-submit and persisted-digest CSRF proof as Checkout. Pending-payment,
pending-moderation, and active reservations may be cancelled; matched and terminal reservations
fail closed.

Cancellation state and account identity are committed atomically to an append-only evidence table.
When signed payment evidence exists, the application immediately prepares or resumes a
`supporter_cancellation` refund using the durable workflow from ADR 0074. Retrying the HTTP request
returns the same outcome without issuing a second refund. The response reports only whether a
refund completed and does not expose the Stripe refund identifier.

## Consequences

- One account cannot enumerate or cancel another account's reservation.
- A process failure after cancellation but before refund completion is recoverable by retry.
- Unpaid Checkout reservations can be cancelled without contacting Stripe.
- Cancellation and refund remain observer-side facts and cannot alter a birth or canonical history.
