# ADR 0070: Google sign-in uses a strict server-side OIDC code flow

## Status

Accepted on 2026-08-08.

## Decision

Google observer sign-in uses the authorization-code flow with PKCE `S256`, a browser-bound state,
and an independent OpenID Connect nonce. The server exchanges the single-use code and verifies the
returned ID token against Google's RSA key set. Verification accepts only `RS256`, exactly one
matching key ID, either documented Google issuer, the configured client ID as audience and authorized
party, a strictly future expiry, a bounded issue time, the attempt's nonce, and a valid stable subject.

Provider HTTP clients reject redirects, enforce HTTPS outside loopback tests, bound response bodies,
and never expose the client secret through their debug representation. Email is optional mutable
metadata; the stable Google `sub` claim is the external identity key. The adapter returns a verified
provider-neutral identity but cannot create a browser session or affect canonical world history.

## Consequences

- State, browser binding, nonce, PKCE, signature, issuer, audience, time, and subject are all checked
  before an external identity can be admitted.
- Authorization-code and key-set responses are treated as untrusted bounded inputs.
- An end-to-end fake-provider contract test proves the exact exchange while negative tests cover
  nonce, audience, expiry, algorithm, and verifier substitution.
- Google credentials and the production callback remain deployment secrets/configuration; no
  provider credential is stored in Git or in the browser-visible application.
