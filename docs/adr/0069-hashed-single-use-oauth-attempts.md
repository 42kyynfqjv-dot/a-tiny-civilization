# ADR 0069: OAuth attempts are browser-bound, hashed, expiring, and single-use

## Status

Accepted on 2026-08-08.

## Decision

Each Apple or Google login attempt generates four independent 256-bit random values: authorization
state, OIDC nonce, PKCE verifier, and a pre-authentication browser binding. State and nonce are sent
to the provider, the PKCE challenge is base64url SHA-256 with method `S256`, and the raw browser
binding/verifier remain only in short-lived secure browser cookies. PostgreSQL stores only SHA-256
digests plus provider and expiry.

A callback can load an attempt only when both state and browser-binding digests match, the attempt is
unconsumed, and its expiry is strictly in the future. After provider token and nonce verification, an
atomic transition consumes it exactly once. Attempt history cannot be updated again or deleted.

## Consequences

- A state copied from another browser cannot complete login.
- Database reads do not reveal usable state, nonce, verifier, or browser-binding values.
- Callback replay, expired attempts, and concurrent duplicate callbacks fail closed.
- Provider adapters share this mechanism and still must validate their own signatures, issuers,
  audiences, authorization codes, and token expiries.
