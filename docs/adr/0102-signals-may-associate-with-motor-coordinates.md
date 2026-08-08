# ADR 0102: Signals may associate with motor coordinates

Accepted on 2026-08-08 as ruleset 25.

## Context

Ruleset 22 lets an organism associate a directly heard amplitude with a subsequently witnessed
primitive action, but all four ruleset-23 movement directions collapse into generic `move`. A sound
can therefore bias movement frequency but cannot participate in learned directional coordination.

## Decision

Ruleset 25 versions private signal associations so a witnessed move retains its exact adjacent
motor coordinate. A later direct hearing of that amplitude weakly adjusts only the matching move
candidate. Non-movement actions remain associated without a motor coordinate. The state is bounded
to 112 addresses: eight amplitudes multiplied by ten non-movement primitives plus four movement
coordinates.

Legacy schema-one associations remain byte-compatible and generic. New schema-two associations are
required only by ruleset 25. Planning, event validation, commit, snapshots, and replay independently
bind the amplitude, action, and optional motor coordinate.

## Consequences

No symbol, token, word, message, intention, compass direction, destination, or inferred meaning is
stored. The association is life-local, non-heritable, and explicitly discarded by public
projections. Ruleset-25 event and snapshot schema 26 provide a minimal substrate from which repeated
sound-guided local coordination could emerge without scripting communication.
