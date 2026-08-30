# ADR 0051: External cognition uses a versioned free-first route ladder

## Status

Accepted and implemented on 2026-08-07. The registry, strict response adapter,
stepwise worker, durable cost reservation, and immutable deadline admission are active
in code. Credentials remain deployment-only and no route is usable until configured.

## Context

External cognition is optional and world-total rather than one remote call per
organism. Multiple providers can contribute free allocations, but quotas, model
availability, commercial terms, and response compatibility can change independently.
A long provider list must not become an unbounded retry storm, a way to select an
unapproved paid model, or an infrastructure-dependent replay gap.

Some apparently free services explicitly limit hosted endpoints to development or
prototyping. Trial credit is also not recurring free capacity. Those routes may be
useful for manual development, but cannot silently enter a public world's production
ladder.

## Decision

- Route policy version one contains an ordered registry of at most 256 exact
  provider/model pairs. A valid provider slug is not sufficient for admission: every
  route remains code-reviewed and model-allowlisted.
- Billing classes are `free_allocation`, `trial_credit`, `development_only`, and
  `paid_approved`. A production registry rejects trial and development-only routes.
- The initial production candidate order is Cloudflare Workers AI GPT-OSS 20B/120B,
  Groq GPT-OSS 20B/120B, Cerebras Llama 3.1 8B and GPT-OSS 120B, OpenRouter's dynamic
  free router, two explicit OpenRouter GPT-OSS free variants, and finally the sole
  paid route.
  Missing credentials or a disabled provider produce a recorded skip; inclusion in
  source code does not enable a provider or replace a current terms review.
- Route policy version two makes OpenRouter's dynamic free router and the two
  pinned free GPT-OSS variants the first production-world attempts. The local
  models remain zero-cost fallbacks, other admitted free providers follow, and
  the paid DeepSeek route remains last and separately authorized. This order
  became active only for newly selected cognition requests; every earlier
  request retains its exact versioned registry and attempt receipts.
- Route policy version three preserves that exact order but canonically quarantines
  the two rejected pinned OpenRouter variants and the uninstalled local GPT-OSS
  route. They produce durable `skipped_disabled` attempts without a network call;
  ADR 0171 records the live evidence, compatibility boundary, and timeout correction.
- A separate development policy may additionally admit NVIDIA's hosted Nemotron
  development endpoint. It cannot validate as production policy and creates no
  private-soak or launch-wait requirement.
- One cognition job may make at most sixteen actual network attempts even when the
  registry eventually contains hundreds of routes. Cooldown, quota-exhausted,
  disabled, and unconfigured routes are skipped without a network call.
- Each visited route produces a normalized attempt status. The successful receipt
  binds the request hash, requested and resolved model, provider response ID/hash,
  token counts, rounded-up micro-dollar cost, adapter version, and exactly one
  use-neutral primitive action. Prose and unknown output fields are rejected.
- Free, trial, and development routes reject a nonzero provider-reported charge.
  OpenRouter's dynamic free route records the actual resolved model.
- The only approved paid route is `deepseek/deepseek-v4-flash` through OpenRouter. It
  must be last and requires explicit authorization for that job. The later worker may
  issue that authorization only after an atomic durable budget reservation; an HTTP
  adapter cannot authorize itself.
- Provider cooldown and quota state may affect a live attempt sequence, so the exact
  normalized sequence is an external input. It must be committed before a fixed
  simulation-tick deadline. Replay consumes the committed result or absence and never
  contacts a provider.
- Credentials remain host-side secrets. They are never stored in prompts, world
  events, snapshots, public projections, logs, or the repository.
- Before any candidate is enabled for a public world, its then-current hosted-service
  terms, commercial-production permission, retention policy, and quota behavior are
  recorded in an operator admission record. A model's open weights do not by itself
  grant production use of a provider's hosted endpoint.

## Consequences

The ladder can grow far beyond the handful of useful providers available today
without changing the canonical contract or attempting hundreds of calls for one job.
Free capacity is opportunistic rather than promised. Provider failure, missing
credentials, exhausted quotas, invalid output, and a denied paid reservation all
degrade to the deterministic local policy instead of pausing the world.

The route registry and normalized result are hashable. The worker persists every
route decision before or instead of dispatch, and PostgreSQL admits only a completed
result present at the fixed simulated-time deadline. A late response remains useful
for audit and billing but cannot replace the immutable local fallback.

## References

- [Cloudflare Workers AI pricing](https://developers.cloudflare.com/workers-ai/platform/pricing/)
- [OpenRouter free-model router](https://openrouter.ai/docs/guides/routing/routers/free)
- [OpenRouter free variants](https://openrouter.ai/docs/guides/routing/model-variants/free)
- [Groq rate limits](https://console.groq.com/docs/rate-limits)
- [Cerebras pricing](https://inference-docs.cerebras.ai/support/pricing)
- [Cerebras rate limits](https://inference-docs.cerebras.ai/support/rate-limits)
- [NVIDIA hosted NIM product terms FAQ](https://docs.api.nvidia.com/nim/docs/product)
