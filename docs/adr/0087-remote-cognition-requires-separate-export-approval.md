# ADR 0087: Remote cognition requires separate export approval

## Status

Accepted on 2026-08-08.

## Decision

A provider credential authorizes authentication and an optional spending cap authorizes cost; neither
authorizes sending private civilization state to that provider. The cognition worker therefore
refuses every configured external adapter unless `COGNITION_EXTERNAL_EXPORT_APPROVED=true` is also
set. Production preflight enforces the same independent flag.

The transferred payload can contain an inhabitant's bodily state, perceptions, learned action
values, and bounded recalled-memory context. It never contains observer account or supporter data,
but it remains a private simulation record and an external data transfer. The approval applies only
to configured providers in the versioned route registry; it does not relax paid-route authorization.

## Verification

Unit tests cover local-only, approved-provider, and unapproved-provider configurations. The runtime
worker was also started against the disposable qualification database with a non-secret test
provider setting and approval disabled: it refused before dispatch. Environment-file tests prove
that production preflight rejects a provider key without the separate flag.

## Consequences

- Local Hindsight, deterministic fallback, and replay require no external-export approval.
- Adding a key cannot silently begin private cognition transfer.
- The owner must approve the chosen provider transfer and disclose it before live qualification or
  public activation.
