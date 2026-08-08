# ADR 0132: Production container inputs are digest-pinned

## Status

Accepted on 2026-08-08.

## Context

Cloudflared, Ollama, and the WAL-G PostgreSQL base were already pinned by immutable manifest digest.
The application builder/runtime, web builder, primary PostgreSQL service, and Hindsight still used
mutable tags. A registry could therefore serve different bytes under the same checked-in source
commit during a later production rebuild.

## Decision

The multi-architecture OCI indexes resolved on 2026-08-08 are committed for Rust 1.97.1 Bookworm,
Debian Bookworm slim, Node 24.19.0 Bookworm slim, PostgreSQL 17 Alpine, and Hindsight 0.8.6. Tags
remain beside each digest for human readability; Docker resolves the digest. A repository gate pins
the exact values and rejects any unpinned `FROM` instruction in the production Dockerfiles.

Locally built application, web, and WAL-G service names remain local image outputs rather than
registry inputs. A future dependency upgrade must deliberately resolve, review, test, and commit a
new digest.

## Consequences

The same source checkout can no longer silently acquire newer base or service image bytes. Package
installation inside a Docker build is still an upstream snapshot concern; digest-pinning closes the
container identity boundary without claiming bit-for-bit reproducible `apt` output.
