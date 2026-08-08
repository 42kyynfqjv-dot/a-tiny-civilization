# ADR 0073: Supporter labels have versioned automatic and human moderation

## Status

Accepted on 2026-08-08.

## Decision

Before any Stripe Checkout session is created, a deterministic versioned policy rejects obvious
profanity, simple separator/leet obfuscations, and labels that impersonate the project or its
operators. Matching is deliberately narrow to avoid substring false positives in real names and to
allow international names. Every admitted reservation immutably records the automatic policy
version that screened it.

Automatic admission is never publication approval. A verified payment moves the label only to
`pending_moderation`; a human must still review abuse, privacy, impersonation, advertising, and
multilingual edge cases before activation. Rejection never changes a canonical birth or organism.

## Consequences

- Obvious disallowed labels are rejected before payment rather than charged and refunded later.
- The policy remains reproducible and can evolve without silently reinterpreting historical rows.
- Legitimate names containing accidental substrings remain eligible for human review.
- Human moderation stays load-bearing because a compact deterministic screen cannot responsibly
  cover every language, confusable character, or contextual abuse case.
