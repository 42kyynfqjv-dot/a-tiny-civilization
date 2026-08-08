# Owner-controlled production handoffs

This checklist distinguishes code that is ready to be configured from integrations
that are not yet implemented. It deliberately contains no credentials. Store a value
only in the protected environment or secret manager of the production host, never in
Git, chat, screenshots, or the browser-visible observatory.

No screen share is needed for any of these actions. The owner can complete each
dashboard step independently and provide only confirmation that it is done. When an
implemented service needs a value, its deployment runbook will name a secure secret
destination and the value can be entered directly there.

## Required before the full-Earth climate release

### Copernicus Climate Data Store / ERA5

CHELSA is a kilometre-scale land-surface climatology. It cannot supply the ocean
component of a full-Earth climate baseline. The owner must create or use a Copernicus
Climate Data Store account, accept the ERA5 monthly-single-level dataset terms, and
create a personal CDS API key. The required retained request will be frozen in a
versioned source manifest before download; it covers January–December 1981–2010 and
includes at least the global near-surface air-temperature, sea-surface-temperature,
and sea-ice-cover fields needed to distinguish land, open ocean, and ice evidence.

Put the key directly in the host's protected scientific-acquisition environment (not
the running observatory environment), then report only `CDS API access ready`. Do not
paste the key into chat or Git. ERA5's monthly single-level product is globally
complete at a declared 0.25-degree regridded resolution and is distributed under CC-BY;
see the [ERA5 dataset record](https://cds.climate.copernicus.eu/datasets/reanalysis-era5-single-levels-monthly-means?tab=documentation)
and [CDS API setup guidance](https://cds.climate.copernicus.eu/how-to-api).

The request contract is now checked in but remains an operator action. From the
protected acquisition environment, install `cdsapi>=0.7.7`, review the exact request
without network access, then run the same command without `--dry-run` only after the
dataset terms have been accepted:

```bash
python3 scripts/acquire-era5-monthly-climate.py \
  --output-directory data/source-cache/era5-monthly-1981-2010 \
  --dry-run
```

It requests separate, no-replacement yearly ZIP responses containing NetCDF members
for global near-surface temperature, precipitation, 10-metre wind, sea-surface
temperature, and sea ice. The raw downloads remain outside Git and are admitted only
after exact hashes, terms evidence, and retrieval metadata are frozen in a source
snapshot.

### Copernicus satellite land cover

The existing CDS account and API credential can also acquire the pinned global 2022
`satellite-land-cover` `v2_1_1` response selected in
[ADR 0034](../adr/0034-copernicus-land-and-soil-source-composition.md). CDS requires
separate acceptance of this dataset's licences. The authenticated request probe fails
closed with `required licences not accepted` until the owner visits the
[land-cover licence manager](https://cds.climate.copernicus.eu/datasets/satellite-land-cover?tab=download#manage-licences)
and accepts every required term. No new API key or account is required.

After acceptance, report only `CDS land-cover licences accepted`. Acquisition will
then observe the actual response media type and byte length before publishing an
immutable source artifact; it will not assume that the response container matches the
dataset's internal NetCDF format.

## Required before public genesis

### Public policy identity and contacts

Before accounts or payments are enabled, insert the legal operator identity and
jurisdiction-specific terms into `docs/policies/`, complete appropriate legal review,
and activate monitored `privacy@atinycivilization.com` and
`support@atinycivilization.com` mailboxes. The technical policy drafts, restrained
world-presentation rules, and supporter refund conditions are checked in; these owner
identity/contact steps cannot be inferred by the application.

### Cognition provider

Install at least one supported provider key directly in
`/etc/a-tiny-civilization-production.env`. The production ladder is free-first across
configured Cloudflare Workers AI, Groq, Cerebras, and OpenRouter routes. Keep
`COGNITION_PAID_ENABLED=false` for a free-only launch; enabling it requires OpenRouter
and is bounded by the canonical three-dollar monthly hard stop. Hindsight itself runs
keylessly on the private application network.

Report only which provider is configured. The application preflight checks presence
and pairing without printing the value.

### Cloudflare DNS and tunnel

The current host-managed Cloudflare Tunnel satisfies the network handoff when it maps
only the intended public hostname to the loopback web origin. A Docker-managed tunnel
instead requires `CLOUDFLARE_TUNNEL_TOKEN` and
`ATINY_REQUIRE_COMPOSE_TUNNEL=1`. In either form, do not route PostgreSQL, the runner,
migrations, Hindsight, cognition workers, or the observer API publicly.

Keep DNS and the site owner-only until the genesis gates in
[Production](PRODUCTION.md) pass. Add Cloudflare Access before exposing any future
administration route.

Cloudflare documents the remotely managed tunnel setup and token lifecycle in its
[Tunnel documentation](https://developers.cloudflare.com/tunnel/setup/) and
[token guidance](https://developers.cloudflare.com/tunnel/advanced/tunnel-tokens/).

## Explicitly deferred for the first genesis

### Server and offsite backups

The implementation choice is now fixed: WAL-G 3.0.8 sends client-side-encrypted WAL and
base backups to Cloudflare R2. In the Cloudflare account:

1. Activate R2 and create a private bucket dedicated to production backups.
2. Create an Object Read & Write token scoped only to that bucket. Record its access
   key ID and secret directly in the host's protected environment.
3. Generate the independent WAL-G encryption key directly into protected storage with
   `openssl rand -hex 32`; escrow it outside both this host and the Cloudflare account.
4. Set the `R2_*` and `WALG_LIBSODIUM_KEY` variables named in
   [Backup and restore](BACKUP_RESTORE.md), then run production preflight.
5. Take the first base backup and record an isolated restore drill when this deferred
   resilience phase is enabled.

Bucket creation, billing, retention duration, credential lifecycle, and encryption-key
escrow remain owner decisions. The application never creates or prints those secrets.
Production preflight accepts no backup settings, but any backup or restore command sets
a strict requirement and refuses to run without the complete configuration.

## Owner activation for implemented observer integrations

### Stripe supporter purchases

The repository now has reservation persistence, strict idempotent webhook admission, and an
authenticated CSRF-protected Checkout-creation endpoint. The route stays indistinguishable from a
missing route until observer sign-in, the Stripe webhook, and the fixed Stripe product are all
configured. A live payment UI is still intentionally absent.

The repository also includes an operator-only, durable full-refund command. It prepares immutable
intent before Stripe is called, uses a reservation-scoped idempotency key, and records the returned
refund ID. Newly admitted payments retain signed PaymentIntent evidence; older rows without it fail
closed for manual review. The operator-only moderation queue and immutable decision ledger are
implemented. Before enabling payments, choose a stable non-secret `ATINY_MODERATOR_ID`, schedule
the queue command in monitoring, and establish who responds to a stale-queue alert.
The repository includes a fifteen-minute systemd timer for this check, and production preflight now
refuses a Stripe-enabled configuration without the moderator identity.

```bash
civilization-api moderation-queue --limit 100 --max-age-minutes 60
civilization-api moderate --reservation-id UUID --decision approve
civilization-api moderate --reservation-id UUID --decision reject
```

The queue command exits nonzero when any paid item has exceeded the age threshold. Approval needs
no Stripe credential. Rejection records immutable evidence first and then requires
`STRIPE_SECRET_KEY` to complete or resume the idempotent full refund.

The authenticated API also supports account-owned cancellation at
`POST /api/v1/supporters/{reservation_id}/cancel`. It requires the same session and CSRF proof as
Checkout, refuses matched or foreign-account reservations, and automatically refunds paid
cancellations. No additional owner credential is required beyond the existing Stripe setup.
The corresponding private `GET /api/v1/supporters/reservations` route gives the signed-in supporter
their own lifecycle and refund status without exposing Stripe or moderation identifiers.

To activate that product, the owner will need to:

1. Create and verify a Stripe account; decide price, currency, refund/transfer policy,
   tax posture, and the moderation/fulfilment policy before enabling live mode.
2. Create a restricted live API key and a webhook endpoint secret with only the needed
   permissions. Place them directly in the production secret store as
   `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, and `STRIPE_SUPPORTER_PRICE_ID`.
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

At activation time, configure the Stripe endpoint secret only in the deployment secret store; never
paste it into repository files. Set the exact product amount/currency and test/live mode alongside
it. The endpoint must subscribe only to `checkout.session.completed` and
`checkout.session.async_payment_succeeded` for that Checkout product. Signed events are admitted at
`POST /api/v1/supporters/stripe/webhook`; the endpoint acts as 404 while the secret is absent.

### Google sign-in

The strict server-side Google OIDC adapter and hardened browser callback are implemented
and contract-tested. Its production return path is fixed as
`https://atinycivilization.com/api/v1/auth/google/callback`. Create a
Google OAuth **Web application** client, add the exact production homepage, JavaScript
origin, and that redirect URI, and enter the client ID and secret directly in the
production secret store. The redirect URI must exactly match, including scheme, case,
and trailing slash.

Google requires a production OAuth app to have a public homepage, terms, and privacy
policy on an owned domain. See [Google's web-server OAuth guide](https://developers.google.com/identity/protocols/oauth2/web-server)
and [OAuth policy](https://developers.google.com/identity/protocols/oauth2/policies).

### Sign in with Apple

The strict server-side Sign in with Apple adapter and form-POST callback are implemented
and contract-tested. It needs an Apple Developer account with an eligible primary App ID,
a Services ID associated with that App ID, the domain `atinycivilization.com`, the exact
return URL `https://atinycivilization.com/api/v1/auth/apple/callback`, and a signing key.
Store the Services ID, team ID, key ID, and PKCS#8 private key in the production secret
store; never commit the private key.

Apple's web configuration requires a Services ID associated with a primary App ID and
registered website/return URLs. See [Configure Sign in with Apple for the web](https://developer.apple.com/help/account/capabilities/configure-sign-in-with-apple-for-the-web).

### LLM and Hindsight

Both code paths are implemented and required by the owner's launch decision. Hindsight
runs keylessly on the private application network. The cognition worker has a versioned
free-first route ladder, immutable request/attempt/result/deadline records, a sixteen-call
network cap, and replay that never contacts a provider. Configure at least one supported
provider in the protected production environment; keep `COGNITION_PAID_ENABLED=false`
unless the bounded paid tail is deliberately authorized.

## What to report back

For now, it is enough to report the completion state, not the secret:

- `CDS API access ready` when the full-Earth climate normalizer is ready for its
  pinned acquisition;
- `CDS land-cover licences accepted` after the dataset-specific terms are accepted;
- `Cloudflare tunnel created` (and hostname chosen),
- `backup destination selected`,
- `Stripe account ready` when supporter payments are ready to build,
- `Google OAuth client ready` / `Apple Services ID ready` only after callback paths are
  implemented.

I will then verify the non-secret configuration and provide the next narrow deployment
step. A secret itself should be typed by the owner directly into the production secret
store, not copied through this conversation.
