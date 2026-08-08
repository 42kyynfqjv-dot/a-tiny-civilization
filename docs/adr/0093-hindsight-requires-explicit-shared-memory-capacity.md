# ADR 0093: Hindsight requires explicit shared-memory capacity

Accepted on 2026-08-08.

## Context

The ruleset-19 qualification backlog exposed a deterministic local operations failure: Hindsight's
embedded PostgreSQL attempted to resize a POSIX shared-memory segment to about 509 MiB during
retain, while Docker supplied its default 64 MiB `/dev/shm`. Every retain correctly remained
pending, but retrying could never make progress.

## Decision

The pinned Hindsight service receives an explicit 1 GiB `shm_size`. This is process-local scratch
capacity, not persistent memory state; the existing named PostgreSQL and model-cache volumes remain
unchanged. The repository gate checks the image pin, keyless extraction mode, disabled document-text
storage, and shared-memory capacity together so a later compose edit cannot silently restore the
64 MiB default. The image supervisor receives a 900-second cold-start allowance because loading the
retained local vector state and offline models exceeded its five-minute default on this host. A
matching 900-second model-initialization timeout prevents Hindsight's internal engine guard from
ending earlier. A 30-second compose stop grace period matches the image's documented
embedded-PostgreSQL shutdown.

The host's named model cache already contains both pinned local models. Runtime therefore defaults
Hugging Face and Transformers to offline mode. Without it, `transformers` performs a metadata-only
`adapter_config.json` lookup even when weights are cached; restricted egress can hold model
initialization until timeout. An operator preparing a genuinely empty cache may explicitly set
`HINDSIGHT_HF_OFFLINE=0`, populate it, then return production runtime to offline mode.

Memory delivery remains asynchronous and replay-external. Hindsight failure cannot alter or block a
canonical tick, and undelivered records remain in the durable outbox for retry.
