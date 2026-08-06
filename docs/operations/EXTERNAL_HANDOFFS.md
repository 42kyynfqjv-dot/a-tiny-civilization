# Owner-controlled production handoffs

This checklist distinguishes code that is ready to be configured from integrations
that are not yet implemented. It deliberately contains no credentials. Store a value
only in the protected environment or secret manager of the production host, never in
Git, chat, screenshots, or the browser-visible observatory.

No screen share is needed for any of these actions. The owner can complete each
dashboard step independently and provide only confirmation that it is done. When an
implemented service needs a value, its deployment runbook will name a secure secret
destination and the value can be entered directly there.

## Required before a public deployment

### Server and backups

Choose the Ubuntu host and an owner-controlled offsite object-storage account, bucket
or prefix, retention period, and encryption-key ownership. Create a least-privilege
credential limited to the backup prefix. Then install and configure either pgBackRest
or WAL-G for continuous WAL archiving and scheduled base backups. Record an isolated
restore drill using the acceptance criteria in
[Backup and restore](BACKUP_RESTORE.md).

This is an owner decision because it creates billing, retention, and recovery
obligations. It cannot be safely guessed or created by the application.

### Cloudflare DNS and tunnel

1. Create a **separate remotely managed production tunnel** in the Cloudflare account
   that owns `atinycivilization.com`; keep staging separate.
2. Add only the intended public hostname and map it to `http://web:3000` inside the
   Docker network. Do not publish or route PostgreSQL, the runner, migrations, or the
   observer API.
3. Put the tunnel token directly in the host's protected environment file as
   `CLOUDFLARE_TUNNEL_TOKEN`, then run the production preflight. Rotate the token if it
   is ever exposed.
4. Keep DNS and the site owner-only until the canonical-world launch gates in
   [Production](PRODUCTION.md) are met. Add Cloudflare Access before exposing any
   future administration route.

Cloudflare documents the remotely managed tunnel setup and token lifecycle in its
[Tunnel documentation](https://developers.cloudflare.com/tunnel/setup/) and
[token guidance](https://developers.cloudflare.com/tunnel/advanced/tunnel-tokens/).

## Do later, when the corresponding feature is implemented

### Stripe supporter purchases

The repository does **not** yet accept payments or create supporter reservations.
When that product is implemented, the owner will need to:

1. Create and verify a Stripe account; decide price, currency, refund/transfer policy,
   tax posture, and the moderation/fulfilment policy before enabling live mode.
2. Create a restricted live API key and a webhook endpoint secret with only the needed
   permissions. Place them directly in the production secret store as
   `STRIPE_SECRET_KEY` and `STRIPE_WEBHOOK_SECRET`.
3. Register every production payment domain and subdomain in Stripe. Stripe handles
   Apple Pay merchant validation, but Apple Pay and Google Pay still require the
   domains to be registered for embedded Elements or Checkout.
4. Use Stripe's Payment Element or Express Checkout Element so eligible Apple Pay and
   Google Pay customers see their wallets. This is a Stripe payment choice, not an
   Apple or Google social-login setup.

Stripe requires payment-method domain registration for Apple Pay and Google Pay, and
handles Apple Pay merchant validation; see its
[domain-registration guide](https://docs.stripe.com/payments/payment-methods/pmd-registration)
and [Payment Element documentation](https://docs.stripe.com/payments/payment-element).

### Google sign-in

Google sign-in is not implemented yet. Once the account service has a defined HTTPS
callback path, create a Google OAuth **Web application** client, add the exact
production homepage, JavaScript origin, and redirect URI, and enter the client ID and
secret directly in the production secret store. The redirect URI must exactly match
the configured value, including scheme, case, and trailing slash; do not configure it
until the application has settled on the callback.

Google requires a production OAuth app to have a public homepage, terms, and privacy
policy on an owned domain. See [Google's web-server OAuth guide](https://developers.google.com/identity/protocols/oauth2/web-server)
and [OAuth policy](https://developers.google.com/identity/protocols/oauth2/policies).

### Sign in with Apple

Sign in with Apple is not implemented yet. It needs an Apple Developer account with an
eligible primary App ID, a Services ID associated with that App ID, the intended domain
and exact return URL, and a signing key. Create those only after the HTTPS callback
path is defined. Store the Services ID, team ID, key ID, and private key in the
production secret store; never commit the private key.

Apple's web configuration requires a Services ID associated with a primary App ID and
registered website/return URLs. See [Configure Sign in with Apple for the web](https://developer.apple.com/help/account/capabilities/configure-sign-in-with-apple-for-the-web).

### LLM and Hindsight

Neither is required for the deterministic simulation, and no current path calls an
LLM. Leave all corresponding variables empty. If an optional cognition or memory
adapter is introduced later, its responses and retrieval results must be recorded as
versioned input events before they can affect a canonical world; its key belongs only
in the production secret store.

## What to report back

For now, it is enough to report the completion state, not the secret:

- `Cloudflare tunnel created` (and hostname chosen),
- `backup destination selected`,
- `Stripe account ready` when supporter payments are ready to build,
- `Google OAuth client ready` / `Apple Services ID ready` only after callback paths are
  implemented.

I will then verify the non-secret configuration and provide the next narrow deployment
step. A secret itself should be typed by the owner directly into the production secret
store, not copied through this conversation.
