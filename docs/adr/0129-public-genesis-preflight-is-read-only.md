# ADR 0129: The composed public-genesis preflight is read-only

## Status

Accepted on 2026-08-08.

## Context

Production configuration, candidate evidence, quality admission, and staged runtime inputs each had
strict validators, but an operator had to remember to run them as separate commands. The first
canonical write is too consequential to rely on an informal checklist, while a readiness check must
not itself make genesis or deploy a site.

## Decision

`public-genesis-preflight.sh` composes the exact production environment validator, qualified
activation's verify-only mode, and the complete staged-runtime verifier. It requires explicit
absolute paths to the genesis and qualification bundles, defaults only the checked-in quality
admission and runtime root, and performs no database write, Compose build/up, or deployment call.

A repository regression gate checks that all three validators remain present and rejects known
mutating commands in the composed script.

## Consequences

There is now one repeatable command that can truthfully say the host and candidate are ready for a
separate deliberate activation. Passing it grants no scientific status and makes no public change.
