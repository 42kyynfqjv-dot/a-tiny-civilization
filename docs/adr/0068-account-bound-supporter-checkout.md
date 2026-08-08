# ADR 0068: Supporter Checkout is bound to an authenticated observer account

## Status

Accepted on 2026-08-08.

## Decision

The `supporter-application` service is the only composition that may create a reservation and a
Stripe Checkout session together. Its caller supplies an authenticated durable observer session and
a naming request without any supporter subject, price, currency, or return URL. The service derives
the immutable supporter subject as `account:{account_id}` and never copies an Apple/Google provider
subject into supporter records.

Reservation creation, remote Checkout creation, and Checkout correlation are independently
idempotent on the reservation UUID. A retry returns the stored Checkout session without another
network call. Reusing the UUID from another account or with changed naming fields fails. A reservation
that has progressed past pending payment but lacks Checkout correlation fails closed.

No HTTP route invokes this service until an Apple or Google callback can produce a verified server
session and the request can prove the independent CSRF secret. The simulation runner cannot depend
on this crate.

## Consequences

- Browsers cannot assert who purchased a reservation or choose its payment product.
- Provider-private subjects remain confined to the identity table.
- Network retry and account-switch tests prove no duplicate Checkout and no canonical events.
