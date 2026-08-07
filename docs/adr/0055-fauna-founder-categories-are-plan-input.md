# ADR 0055: Fauna founder categories are immutable population-plan input

## Status

Accepted and implemented on 2026-08-07 for provisional fauna population-plan schema
two.

## Context

The runner previously assigned every fauna founder the category `unspecified`. That
was honest about missing demographic evidence, but it made a female/male reproductive
profile impossible to apply and could never satisfy the observer-only promise to match
a supporter reservation to the next naturally committed animal birth of a requested
category. Assigning categories inside the runner would hide an outcome-shaping choice
outside the source-addressed genesis artifacts.

## Decision

- Population-plan schema two records a strictly ordered list of birth-category counts
  for every planned real species.
- Every category count is positive and their checked sum must exactly equal the
  species' initial individual count.
- The runner expands those categories in artifact order. It does not infer, alternate,
  randomize, or relabel them.
- The provisional generator may create one female and one male founder only as an
  explicit engineering assumption. Scientific review can replace the generator before
  a later world without rewriting this world's history.
- Schema-one artifacts remain canonical and continue to create `unspecified` founders,
  preserving archived integration-world behavior.
- Birth category remains canonical private mechanics. Public projections continue to
  withhold sex, reproduction detail, parentage, and explicit violence.

## Consequences

Animal reproduction can be exercised before genesis without leaking demographics in
from runtime code. Observer-side naming can match only an already committed eligible
birth, while supporter and payment code remain unable to influence whether or when the
birth occurs. The provisional assumptions remain visible and replaceable instead of
being mistaken for measured ecology.
