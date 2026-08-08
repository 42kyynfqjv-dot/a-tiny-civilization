# ADR 0072: Sign in with Apple uses a strict server-side code flow

## Status

Accepted on 2026-08-08.

## Decision

Apple observer sign-in uses Apple's web authorization-code flow with `form_post`, a browser-bound
state, and an independent nonce. The API accepts the callback only as a bounded form body. It
exchanges Apple's five-minute single-use code using a freshly generated five-minute ES256 client
secret signed by the configured Apple PKCS#8 key. Routine debugging never includes that key.

The exchanged ID token must use RS256 and exactly one matching Apple RSA key. Verification requires
Apple's exact issuer, the configured Services ID as audience, a strictly future expiry, a bounded
issue time, the attempt nonce, and a valid stable subject. Email remains optional mutable metadata.
The shared attempt is then consumed exactly once before a provider-neutral browser session is issued.

## Consequences

- Apple and Google share the same account/session boundary without treating email as identity.
- Apple callbacks cannot enter access logs as query secrets because they use a bounded POST body;
  the global trace layer records paths only in either case.
- Partial Apple configuration fails process preflight, while absent configuration leaves Apple
  routes disabled.
- Apple account configuration is the only owner-side prerequisite remaining for this adapter.
