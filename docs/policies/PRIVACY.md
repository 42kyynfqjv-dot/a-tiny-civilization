# Privacy notice

Status: pre-launch notice, effective when public accounts are enabled.

A Tiny Civilization is a public observatory. Anonymous visitors can read the world record without
an account. When someone signs in, the service stores the identity provider, that provider's stable
account identifier, an optional verified email address, and timestamps. It stores only hashes of
session, CSRF, OAuth state, nonce, PKCE, and browser-binding secrets. For supporter purchases it also
stores the submitted alias and eligibility choices, internal account ID, Stripe Checkout/session and
webhook references, amount/currency evidence, moderation state, and any eventual birth match. Stripe,
not this service, handles card and wallet credentials.

Infrastructure necessarily processes IP addresses, request metadata, security logs, and short-lived
operational diagnostics. Routine application traces record URL paths but not OAuth query strings,
authorization codes, cookie values, provider secrets, or payment credentials. We use this data to
authenticate observers, prevent abuse and fraud, fulfil supporter reservations, operate and secure
the service, comply with law, and preserve an auditable transaction history. We do not sell personal
data or use observer data to steer the simulation.

Relevant processors may include Apple or Google for sign-in, Stripe for payment, Cloudflare for DNS,
security and network delivery, and the hosting/database operators. Data may be processed in countries
other than the visitor's. Browser sessions expire after 30 days and can be revoked. Security and
transaction evidence is retained as long as reasonably needed for fraud, accounting, dispute, and
legal obligations. Public aliases and their provenance may remain in an archived world record; an
abusive or privacy-invasive alias can be hidden without rewriting canonical simulation history.

People may request access, correction of mutable account metadata, session revocation, or deletion
where applicable. Some payment, fraud, and public-record evidence may need to be retained or
de-identified. Contact `privacy@atinycivilization.com`; that mailbox and an operator response process
must exist before accounts are enabled. This notice must be updated with the legal operator identity,
jurisdiction-specific rights, and any additional processors before public account launch.
