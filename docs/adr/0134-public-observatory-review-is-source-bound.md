# ADR 0134: Public observatory review is source-bound

## Decision

The public observatory receives a separate, machine-verifiable admission after the
canonical world mechanics are frozen. The admission pins one full Git source commit,
the exact `web/` and `docs/policies/` trees, the required public route set, five review
dimensions, and the SHA-256 of the experimental quality-world admission.

Its verifier requires the reviewed commit to remain an ancestor of the current checkout
and rejects every tracked or untracked difference inside the reviewed paths. Public-genesis
preflight and the production deployment helper both run that verifier. The admission always
contains `public_deployment_authorized: false`; passing review cannot substitute for the
deployment helper's literal operator confirmation or create a world.

## Consequences

Operational documentation, preflight code, and other non-reviewed tooling can continue to
improve without repeating the world qualification or silently changing the reviewed site.
Any observer or policy change requires a deliberate new review commit and admission. A good
web build alone is insufficient, while a reviewed site alone is also insufficient: the record
is cryptographically bound to the separately admitted world candidate.
