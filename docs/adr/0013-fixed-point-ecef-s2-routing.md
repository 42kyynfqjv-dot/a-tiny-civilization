# ADR 0013: Embodied address routing uses an exact ECEF geocentric ray

## Status

Accepted on 2026-08-06 as a private reference contract. This does not define a
durable position, movement rule, event, snapshot field, or canonical genesis path.

## Context

An S2 CellId is a hierarchical address, not a physical position. Embodied state needs
fixed-point physical coordinates, while the partition scheduler needs one exact answer
for which S2 address owns a position. Deferring that bridge would leave a source of
platform-dependent history between the two completed foundations.

The bridge also has a scientific ambiguity that must not remain implicit. WGS 84 ECEF
coordinates can be mapped to S2 using the geocentric ray from Earth's centre, or first
converted back to geodetic latitude. Those mappings differ because Earth is an
ellipsoid. Ingestion and runtime must never choose independently.

Persisting a provisional position now would be worse than leaving genesis blocked. It
would change event bytes, state hashes, snapshots, PostgreSQL records, movement
semantics, and every archived verifier before local physics exists.

## Decision

- `sim-engine` contains a private reference `EcefPositionMm` with signed integer
  millimetres in WGS 84 ECEF (EPSG:4978). Each component is restricted to
  ±7,000,000,000 mm, which encloses the WGS 84 ellipsoid with margin. The origin is
  invalid because it has no direction. This envelope is not terrain, height, habitat,
  or surface-contact validation.
- S2 receives the **geocentric ray** from Earth's centre through that ECEF point.
  Positive scaling therefore cannot change the address. Global source ingestion must
  eventually use this same bridge rather than conventional geodetic-latitude S2 unless
  a successor world explicitly adopts another version.
- Face selection and face-coordinate signs follow Google S2. Strict largest-component
  comparisons make exact ties prefer Z, then Y, then X; positive faces are 0–2 and
  negative faces are 3–5.
- The quadratic S2 projection is evaluated without floating point, division,
  trigonometry, or square roots. The router binary-searches exact rational boundaries
  obtained from `STtoUV(k / 2^level)`. Equality belongs to the higher-index half-open
  cell; `-1` selects zero and `+1` clamps to the final cell.
- `(face, i, j, level)` becomes a CellId through the standard S2 Hilbert orientations.
  Checked `i128` intermediates and checked shifts fail closed. Levels 0–30 are
  supported; L10, L14, L18, and L23 are the configured causal roles.
- The reviewed upstream revision is Google S2
  [`97d76747276147afb716b1c03863ae2b3e50ed65`](https://github.com/google/s2geometry/commit/97d76747276147afb716b1c03863ae2b3e50ed65).
  The relevant source-file SHA-256 values at that revision are:
  - `s2coords.h`: `ae003e685fa98a2c6a16978107053b3aa29458deb8d8dd120a9f0550e58f6277`;
  - `s2coords_internal.h`: `7a8726cbd45c57f75f3462d6db426da9840ffc90e63729e6f66d649f27ceb2e8`;
  - `s2cell_id.cc`: `7b569edce585a548fa22480f9dea425f05deef6bfc3c7c8e496863b5853ca8ca`.
- One checked-in fixture pins 68 addresses across all six faces, causal levels, leaf
  level, every cube-edge sign combination, all eight cube corners, exact projection
  boundaries, and ±1 mm boundary perturbations. Rust tests and a standard-library-only
  Python verifier independently consume the same fixture.
- Interior and face-tie expected values were generated independently with `nodes2ts`
  4.0.2 and recomputed from the reviewed S2 formulas. Exact rational-boundary cases
  are contract-derived rather than floating-point generator output.

## Deliberate non-decisions

- `EcefPositionMm` is not serializable, publicly exported, added to `OrganismState`, or
  used by `InitialOrganism`, `DomainEvent`, snapshots, PostgreSQL, or configuration.
- `location_id: Option<EntityId>` remains a legacy location reference and is not
  reinterpreted as a physical coordinate.
- ENU frames, EGM2008/geoid conversion, terrain contact, altitude, velocity, movement
  rounding, collision, perception range, and cross-patch integration remain later
  embodied-physics decisions.
- Configuration currently records an S2 revision and definition digest but does not
  yet prove that the compiled router supports them. Canonical genesis remains blocked
  until the application binds an accepted routing contract to the exact world bundle.
- The upstream file hashes above are documentary in this reference checkpoint. The
  global source acquisition step must retain and mechanically verify the exact source
  artifacts before those hashes can become canonical world inputs.

## Verification

The reference proves:

- every face and official tie rule yields the pinned CellId;
- direct L10/L14/L18/L23 routing agrees exactly through `ancestor()`;
- positive integer scaling preserves an address;
- exact quadratic-boundary equality selects the documented cell and a one-millimetre
  perturbation crosses only on the expected side;
- invalid origin, envelope overflow, and invalid levels fail closed; and
- Rust and Python produce identical sixteen-character CellIds for every fixture row.

## Consequences

The physical-to-hierarchical address bridge is now testable without claiming that an
organism has durable physical state. Conserved refinement can use the same exact cell
semantics in a private proof. Full-Earth genesis still returns
`PartitionedExecutionNotImplemented` until position persistence, movement, refinement,
durable scheduling, and the commit barrier are integrated through explicit schema
decisions.
