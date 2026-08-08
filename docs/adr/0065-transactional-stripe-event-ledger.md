# ADR 0065: Stripe events and reservation transitions commit atomically

## Status

Accepted on 2026-08-08.

## Context

Signature verification proves delivery authenticity, but Stripe retries events and can emit more
than one event for the same Checkout session. A process crash between marking a reservation paid and
recording the event would make retries ambiguous. The inverse order could retain false evidence for
a transition that rolled back.

## Decision

PostgreSQL owns an append-only `stripe_webhook_events` ledger. One transaction serializes the event
ID and Checkout session, rejects reuse with different evidence, locks the referenced pre-existing
reservation, records the exact amount/currency/mode/payload digest, and changes
`pending_payment` to `pending_moderation`. Unknown reservations and conflicts roll back completely.

Exactly one recorded payment may exist per reservation and per Checkout session. A later legitimate
event for the same reservation and session is retained as `duplicate_payment` without granting a
second entitlement. Correctly signed irrelevant or unpaid events are retained as `ignored`.

The observer API exposes only `POST /api/v1/supporters/stripe/webhook`, bounded to 64 KiB. The route
is absent in effect (404) unless an endpoint secret is configured. It accepts raw bytes, returns no
reservation or payment detail, and has no dependency path into the simulation runner.

## Consequences

- Retried, concurrent, and out-of-order delivery cannot double-entitle a supporter.
- A database failure produces no partial transition and can safely be retried.
- A paid reservation still requires independent moderation before it can match a later committed
  birth.
- Checkout creation, authenticated reservation creation, moderation UI, and refund policy remain
  later observer-product work; they do not weaken this admission boundary.
