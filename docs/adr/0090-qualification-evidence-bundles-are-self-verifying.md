# ADR 0090: Qualification evidence bundles are self-verifying

## Status

Accepted.

## Decision

Before genesis authorization, `scripts/create-qualification-evidence.sh` packages the exact
seed-bound genesis derivations, their original checksum manifest, the passing ADR-0089 world report,
and the clean source commit into a new immutable directory. A root `SHA256SUMS` covers every bundled
file. The command refuses an existing destination, a dirty worktree, invalid genesis checksums, a
failed qualification report, or an uncommitted source identity.

The bundle deliberately contains no canonical event payloads. The qualification report exposes
counts and status only; public history commitments remain the separate, bounded audit interface.
The evidence document has no creation timestamp, host name, or filesystem path, so identical inputs
produce identical bytes.

## Consequences

Launch review can retain and independently verify one compact artifact rather than trusting terminal
history. The source commit binds the operator binaries and rules, while the nested genesis manifest
binds the seed-derived inputs. Scientific admission and provider-export approval remain separate
gates and cannot be implied by a mechanically passing bundle.
