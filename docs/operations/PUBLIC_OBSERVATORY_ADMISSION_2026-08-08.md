# Public observatory admission — 2026-08-08

The observer-facing surface at source commit
`cf5f045be50d71efb223c58a877682febe7cd866` passes the pre-deployment observatory review.
The adjacent canonical JSON record binds the review to the exact `web/` and
`docs/policies/` trees and to quality-world admission SHA-256
`ba8c5abc0e3e005bbf0e21e4d473d33ce1902fda2a9ee6649e4427912b279dde` for world
`b3ea736d-7a5a-5161-a74b-fa8c4302d333`.

The reviewed surface includes:

- a live-world landing page and deterministic, evidence-linked finding aids;
- a separate read-only observer wiki;
- public privacy, terms, supporter-naming, and restrained-presentation routes;
- a supporter panel that remains closed when integrations are absent and cannot affect births;
- same-origin API proxying, hardened response headers, and no-store authenticated responses; and
- eight individually reported server-render and proxy contract tests under Node 24.19.0.

Run the read-only verifier with:

```bash
./scripts/verify-public-observatory-admission.py \
  --admission docs/operations/PUBLIC_OBSERVATORY_ADMISSION_2026-08-08.json
```

The verifier rejects a changed reviewed tree, untracked files in that tree, incomplete routes,
missing evidence, a changed world admission, or any attempt to turn this review into deployment
authorization. Provider credentials, legal operator identity, production activation, live tick-one
verification, and the literal public-deployment confirmation remain separate.
