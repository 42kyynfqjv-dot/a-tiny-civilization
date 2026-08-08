# ADR 0080: Deployment validates the exact production environment file once

## Status

Accepted on 2026-08-08.

## Decision

`production-preflight.sh` accepts an optional absolute env-file path, loads only literal shell-style
assignments without evaluating them as code, rejects duplicate keys, and requires that the file is a
regular non-symlink owned by root or the invoking operator with no group or other access.

The production deployment helper delegates all configuration policy to that preflight using the
same env file it passes to Compose. It no longer carries a second, incomplete set of regular-expression
checks. Provider, authentication, payments, moderation, tunnel, backup, image-pinning, and Compose
validation therefore have one fail-closed implementation.

## Consequences

- Manual preflight and the deployment helper cannot drift on which combinations are accepted.
- A root deployment never executes the environment file as shell code.
- Production values containing expansion syntax must be quoted as literals; duplicate assignments
  are errors rather than precedence rules.
