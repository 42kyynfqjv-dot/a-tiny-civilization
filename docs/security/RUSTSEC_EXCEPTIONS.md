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
