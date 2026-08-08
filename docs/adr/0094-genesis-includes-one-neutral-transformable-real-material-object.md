# ADR 0094: Genesis includes one neutral transformable real-material object

Accepted on 2026-08-08.

## Context

The first 1,000-tick ruleset-19 qualification history contained two real-material instances but no
surface traces. Inspection showed this was not random quiet: both instances were replenishing
glucose/water reservoirs. Reservoir transfer is a different physical mechanism and reservoirs are
not graspable instances, so the new neutral transformation substrate was unreachable in the actual
genesis path.

## Decision

Provisional material-resource plan schema 2 distinguishes replenishing sources from finite material
objects. A source's reservoir commitment is now optional. A non-reservoir source must have no oral
transfer profile, preventing the plan from smuggling a food or use label into the object.

The provisional derivation adds one 100,000 mg material instance identified as PubChem compound
24261, silicon dioxide, at the founder patch. Its identity is real and cited. Its local occurrence,
mass, shape, and placement remain explicit engineering assumptions pending scientific admission.
The plan does not call it stone, quartz, flint, a tool, or an artifact, and supplies no purpose or
affordance. Organisms can only encounter it through the existing physical perception/action grammar.

Schema-1 plans remain byte-compatible: their mandatory reservoir objects deserialize as present and
serialize to the same JSON shape. New derivations use schema 2 and canonical source ordering.

## Qualification consequence

A ruleset-19 qualification is not complete merely because 1,000 ticks replay. It must demonstrate
that its configured genesis makes the ruleset's transformation path reachable and that any resulting
observer artifact has exact canonical provenance. The first reservoir-only qualification remains a
useful immutable negative test; the corrected qualification uses a newly derived bundle and a new
database/world rather than editing that history.

The corrected disposable world reached tick 1,000 at sequence 1,018 with genesis replay equal to
snapshot plus tail. Its free policy produced 12 grasps, 12 releases, and 11 surface-trace changes;
the observer projection filed one cited silicon-dioxide object whose trace reached 29 units. A live
localhost API read returned the same object with first provenance at tick 70/sequence 73 and latest
provenance at tick 735/sequence 749. No observer input or scripted action was used.
