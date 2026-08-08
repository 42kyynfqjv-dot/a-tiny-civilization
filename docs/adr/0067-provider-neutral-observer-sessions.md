# ADR 0067: Observer accounts use provider subjects and hashed opaque sessions

## Status

Accepted on 2026-08-08.

## Context

Apple and Google account credentials are owner-controlled deployment handoffs, but Checkout must not
trust a supporter subject supplied by a browser. Email addresses can change and are not stable account
identifiers. Persisting bearer session secrets would turn a database read into immediate account
impersonation.

## Decision

The provider-neutral `observer-auth` boundary admits only an identity already verified by a specific
Apple or Google adapter. Identity is keyed by `(provider, subject)`; optional email and verification
state are mutable metadata and never identity keys.

Each login creates independent 256-bit operating-system-random session and CSRF secrets. Only their
SHA-256 digests enter PostgreSQL. Sessions have immutable account/provider/subject/creation/expiry
fields, explicit revocation, exact expiry enforcement, and append-preserved history. Account disablement
invalidates every session at authentication time. Raw secrets are redacted from diagnostics.

No public login route exists until its provider adapter verifies authorization state, nonce, token
signature, issuer, audience, and expiry. The adapter must use the server authorization-code flow over
HTTPS. Google identity uses `sub`, never email. Apple uses the configured Services ID as audience and
the exact registered return URL.

## Consequences

- Account/session implementation can be tested and deployed without Apple or Google credentials.
- Provider setup cannot be bypassed with a client-asserted email or subject.
- Authentication, sessions, supporter reservations, and Checkout remain outside canonical world
  history and runner dependencies.
