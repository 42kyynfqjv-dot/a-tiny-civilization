# ADR 0163: Hetzner primary with deterministic research continuity

Status: accepted

## Decision

Cancer World exploration route policy 11 uses the free Hetzner experimental
Inference API as its first route, pinned to `Qwen/Qwen3.6-35B-A3B-FP8` through
the OpenAI-compatible endpoint. The exact provider, requested model, resolved
model, response identity, response hash, and route-policy hash remain durable.
The service receives only `HETZNER_VLLM_API_KEY`; no other backend receives it.

Hetzner's platform is explicitly experimental and may change without a
production stability promise. Existing free routes therefore remain fallbacks.
Route policy 10 remains readable so completed research is permanently
verifiable.

Exploration policy 11 also includes a provider-independent
`systematic-screen-v1` route. When reasoning providers fail, it deterministically
enumerates the closed virtual-lab dimensions, preregisters plans, answers the
existing held-out benchmark with a deterministic baseline, and executes frozen
campaign replications. It is labelled as a systematic computational projection,
never as novel reasoning or biological evidence. This route costs nothing and
prevents provider availability from stopping basic screening, triage, and
falsification.

## Consequences

- Open-ended hypothesis quality still depends on a capable reasoning model.
- Basic research throughput continues without an LLM or paid inference.
- The deterministic route may produce deliberately plain artifacts; novelty,
  qualification, virtual-lab, and duplicate gates apply unchanged.
- Enabling the primary route requires a token created in Hetzner Experiments;
  an absent token causes an auditable unconfigured skip, not a service failure.
