# ADR 0104: provider schema closes the motor grammar

## Status

Accepted.

## Context

The cognition adapter already validated every returned primitive action and its optional motor
coordinate. Its provider-facing JSON Schema, however, described the action kind and coordinates as
independent fields. A small local model could therefore generate a structurally valid object such as
a move with no movement direction. The adapter correctly rejected it, but the provider had not been
given the strongest grammar it could enforce while generating.

## Decision

Adapter version `openai-compatible-bounded-cognition-v5` presents eleven mutually exclusive strict
JSON-Schema variants: one for each use-neutral primitive. Move requires exactly one direction 0–3;
apply-force requires exactly one contact region 0–7; emit-signal requires exactly one intensity 1–8;
all remaining coordinates are null. Every variant requires all four fields and rejects additional
properties.

The returned object still passes the independent Rust receipt validator. Provider-side constrained
generation is not trusted as canonical validation and cannot expand the action grammar.

## Consequences

Compatible models are prevented from producing invalid action/coordinate combinations during token
generation, reducing avoidable fallback without repairing or interpreting model output. Providers
that do not support the published strict schema fail normally and the recorded free-first ladder
continues. Replay consumes only the immutable accepted receipt and never calls a model.
