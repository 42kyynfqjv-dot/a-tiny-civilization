# ADR 0131: Public deployment needs literal confirmation

## Status

Accepted on 2026-08-08.

## Context

The deployment helper is intentionally root-only and heavily preflighted, but its previous command
shape differed from a read-only host check only by the script name. On a host whose web origin is
already connected to a public tunnel, an accidental invocation can immediately replace public
content even when it creates no canonical history.

## Decision

`deploy-production-app.sh` requires the exact `--confirm-public-deployment` argument. It has no
environment-variable default or alternate spelling. Argument validation occurs before reading
secrets, building images, or changing Compose state. Repository checks pin the confirmation
boundary.

## Consequences

Root access and valid configuration remain necessary but are no longer sufficient to deploy. This
is an operator intent check, not public-launch authorization and not a substitute for the read-only
genesis preflight, activation evidence, observed-first-tick gate, or edge-access decision.
