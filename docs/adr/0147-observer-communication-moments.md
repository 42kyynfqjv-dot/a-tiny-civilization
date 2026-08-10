# ADR 0147: the habitat exposes bounded communication moments

Date: 2026-08-10

Status: Accepted

## Context

After local interaction activated, the canonical world recorded human calls, direct hearing, and
private signal/action associations. The habitat projection retained only the primitive call action.
Observers therefore saw pulsing markers and anonymous signal forms but could not see the causal
relationship between one person calling, another hearing, and that listener later associating the
call with an observed action. Mobile CSS hid even the primitive activity panel.

## Decision

The observer habitat maintains a disposable, bounded projection of two canonical event classes:

- `heard_signal`: one organism directly received another organism's physical signal; and
- `associated_action`: one organism's private learning state associated that signal form with the
  source organism's subsequently observed primitive action.

The public habitat returns only person-to-person rows, keeps at most 512 recent rows, and serves at
most 64 per request. Violence-adjacent actions remain excluded. The lens groups hearing events into
plain-language moments, draws temporary acoustic and association links between visible people, and
keeps a compact version visible on mobile.

The projection says “pre-language” and explicitly distinguishes a physical call or learned pattern
from a word, meaning, intention, or detected language. It is observer-only, never simulation input,
and may be deleted and rebuilt without changing history.

## Consequences

- An observer can see that social and communication mechanics are active without reading raw event
  payloads.
- The display remains bounded independently of population and canonical event volume.
- The language archive remains the sole published threshold for conventions or language; the live
  lens cannot promote frequent signaling into a discovery.
