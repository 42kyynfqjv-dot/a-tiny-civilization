# ADR 0078: Supporter payments require monitored moderation operations

## Status

Accepted on 2026-08-08.

## Decision

Production preflight refuses to enable Stripe Checkout unless a stable, restricted-character
`ATINY_MODERATOR_ID` accompanies the complete payment, webhook, and login configuration. Compose
passes that identity only to the observer API container, where the operator moderation command can
record it; it never enters the simulation services.

A checked-in oneshot service runs the oldest-first moderation queue check every fifteen minutes.
The check exits unsuccessfully when a paid item has waited longer than the configured threshold,
leaving a visible failed systemd unit for host alerting. The command does not make a decision or
refund automatically: a human still reviews and explicitly approves or rejects each label.

## Consequences

- A payment deployment cannot accidentally omit attributable moderation identity.
- Queue age is an operational health signal with reproducible limits.
- Enabling the timer and routing failed-unit alerts remain explicit owner deployment actions.
