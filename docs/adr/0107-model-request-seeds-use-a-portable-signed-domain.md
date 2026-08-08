# ADR 0107: Model request seeds use a portable signed domain

Date: 2026-08-08

Status: Accepted

## Context

Model inference is observer-side input preparation, but the same strict request should remain
portable across the free-first provider ladder. The adapter originally interpreted the first eight
request-ID bytes as an unsigned 64-bit `seed`. Ollama's OpenAI-compatible endpoint decodes this
field into Go's signed `int`; a canonical-candidate request whose UUID began with a high bit was
therefore rejected before inference. A fixed synthetic localhost probe reproduced the exact HTTP
400 response without exposing world context.

## Decision

- Derive the model request seed from the first four request-ID bytes.
- Clear the high bit, then map zero to one, yielding the closed domain 1 through 2,147,483,647.
- Keep request UUID selection, cognition deadlines, recorded receipts, and canonical fallback
  behavior unchanged.
- Continue validating model output independently; a portable request seed does not relax the
  strict motor-action schema.

## Consequences

Every configured OpenAI-compatible route receives a deterministic positive seed representable by
common signed integer decoders. Existing recorded cognition attempts remain immutable evidence;
future requests use the corrected adapter and are still incorporated only at their fixed deadline.
