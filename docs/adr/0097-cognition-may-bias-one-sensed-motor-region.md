# ADR 0097: Cognition may bias one sensed motor region

Accepted on 2026-08-08 for ruleset 20 and later.

## Decision

A successful bounded cognition result may include `contact_region`, an optional integer from zero
through seven. It is valid only with the use-neutral `apply_force` primitive. When present, the
existing cognition weight bonus applies only to the otherwise-valid force candidate with that
motor coordinate. It cannot create an action, select an unavailable object, change force, delay a
deadline, or directly change world state. If no matching candidate exists, it has no effect.

The coordinate has the same meaning as ruleset 20's direct `surface_region_N` touch readings: a
bounded physical location on the object. It conveys no glyph, character, writing, purpose, or
observer classification. The model prompt states that restriction and the strict response schema
permits only an integer from zero through seven or null.

## Compatibility

`contact_region` is optional and omitted when absent in canonical receipts and deadline inputs.
Historical cognition records therefore retain their exact serialized form and hashes. A missing
coordinate preserves the existing action-kind-wide bias. The model adapter version advances to
`openai-compatible-bounded-cognition-v2` so new provider receipts disclose the changed response
contract without invalidating recorded v1 evidence.

## Enforcement

- Domain and application validation reject an out-of-range region or a region paired with any
  primitive other than `apply_force`.
- Provider output is parsed with unknown-field rejection and then validated against the immutable
  request and allowlisted route.
- Engine tests prove that exactly the matching regional candidate receives the bonus and that the
  other seven force candidates remain unchanged.
- Replay consumes the recorded deadline input and never calls a model.
