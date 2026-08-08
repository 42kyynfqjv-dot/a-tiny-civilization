# ADR 0017: Supporter reservations remain observer-only

## Status

Accepted on 2026-08-06 and extended by ADRs 0064–0065. The queue and transactional
Stripe webhook admission are implemented without account UI or a public checkout-creation endpoint.

## Decision

- A reservation names a world, opaque external supporter subject, observer label,
  person-or-animal target, optional cited animal species, and requested birth category.
  These request fields are immutable after creation.
- The lifecycle is `pending_payment` → `pending_moderation` → `active` → `matched`,
  with explicit rejected, supporter-cancelled, and extinct-world expired terminal paths.
  A verified idempotent webhook supplies the payment reference; browser redirects do
  not activate a reservation. A human moderator independently approves before it is
  eligible, covering abuse, privacy, impersonation, advertising, and unsafe content.
- Matching receives an already-committed canonical birth reference and atomically
  consumes the oldest matching active reservation. It only stores an external label
  link; it never inserts or changes a world event, organism, state, or schedule.
- Unique birth-event and organism links prevent duplicate fulfillment. Matched and
  payment evidence are immutable. After the observer projector sees an immutable
  archived lifecycle state, it idempotently expires every unmatched reservation
  (including pending payment and moderation) while preserving matched history.
- This lives behind `observer-projection`, which does not depend on the engine. The
  runner has no direct dependency on the reservation port or a payment/auth crate.

## Consequences

The first site can truthfully show a disabled supporter preview. Checkout creation,
Apple Pay/Google Pay presentation, auth, moderation tools, and refund/transfer policy
can be added without giving them a path into canonical history. Observer projections
must call matching only after consuming durable committed birth events.
