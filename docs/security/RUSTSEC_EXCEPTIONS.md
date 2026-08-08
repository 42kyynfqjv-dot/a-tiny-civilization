# RustSec audit exceptions

## RUSTSEC-2023-0071 — `rsa` timing side channel

`Cargo.lock` contains `rsa 0.9.10` beneath SQLx's optional MySQL package. A locked optional package
is visible to `cargo audit`, but this PostgreSQL-only workspace enables SQLx with
`default-features = false` and the `postgres` feature; neither `sqlx-mysql` nor `rsa` has a compiled
dependency path.

The CI exception is therefore narrow and executable:

- only advisory `RUSTSEC-2023-0071` is ignored;
- `cargo tree --locked --target all -i rsa@0.9.10` must produce no dependency path before the audit
  is allowed to run; and
- if any workspace feature later makes RSA reachable, the audit script fails before applying the
  exception.

The advisory has no patched release. Removing or replacing this exception requires either SQLx lock
resolution that omits the unused optional package or an upstream constant-time RSA fix. The project
does not use RSA private-key operations through this package; Apple client assertions use the
separately configured elliptic-curve signing path.

Rust license and source policy is independently enforced by pinned `cargo-deny 0.20.2` using
`deny.toml`. The allowlist contains only commercially usable SPDX choices, and dependencies from an
unknown registry or Git source fail the build. Web dependencies undergo the same fail-closed check;
licenses with attribution, file-level copyleft, or relinking obligations are enumerated at exact
package versions in `WEB_LICENSE_REVIEW.json`. The project does not publish the CI container images,
but this review remains mandatory before that distribution model can change.
