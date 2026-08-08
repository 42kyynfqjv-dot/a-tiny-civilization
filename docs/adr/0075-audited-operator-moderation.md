# ADR 0075: Paid labels use an audited operator-only moderation queue

## Status

Accepted on 2026-08-08.

## Decision

Paid labels remain unavailable to birth matching until a human explicitly approves them. The
operator CLI lists the paid queue in stable oldest-first order, measures age from verified payment
rather than reservation creation, and exits unsuccessfully when any item exceeds a configured age.
This makes the same command usable by a human and a scheduled monitor without creating a public
administration route.

Approval and rejection atomically record one immutable decision containing the moderator identity,
the automatic policy version, and decision time. A retry by the same moderator is idempotent;
changed decision or identity evidence conflicts. Rejection then resumes the durable Stripe refund
workflow. If the process stops between rejection and refund completion, running the same command
again continues safely.

## Consequences

- No paid label can silently become active without durable human-review evidence.
- Queue staleness can page the operator using an ordinary nonzero process exit.
- The public observer router has no moderation or refund endpoint to attack.
- Operations must configure a stable `ATINY_MODERATOR_ID` and schedule the queue check before
  enabling purchases.
