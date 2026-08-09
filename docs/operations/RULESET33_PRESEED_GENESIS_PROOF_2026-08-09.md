# Ruleset-33 pre-seed genesis proof — 2026-08-09

Commit `0bfbc96` was built in release mode and exercised through the database-free canonical-genesis
proof before the already-published successor beacon round was revealed. To avoid previewing any
successor input, the proof reused the immutable ruleset-32 v24 world ID, seed, and portable input
bundle while explicitly selecting ruleset 33.

The proof verified 147,466 content-addressed full-Earth references totaling 10,164,215,509 bytes. It
then constructed tick-zero state with 88 organisms (24 human founders and 64 fauna), three material
instances, 15 scientific dataset commitments, and 183 canonical genesis events. Complete replay
matched the constructed snapshot.

Exact outputs:

- ruleset: `33`
- snapshot schema: `32` (the latest compatible schema; ruleset 33 adds no state field)
- batch hash: `8018d7ce54a48c3e9d81b5d365d8fa1219d84c075fdb55b5e8720810350619f6`
- state hash: `396a690e3096547feb26af60ee51d3c0fa1ff22033f6e22d7411c8214a8ad0c6`
- composition hash: `449ecf9e2956af072eaffbef4bd31c51160d4494d109a81eb5d7c485d187868f`
- portable genesis manifest hash:
  `76d54b0749bd9602c625c73d9f6eac78c21ca06865ece796976e49284e06a725`

This closes integration risk in the ruleset implementation only. It is not qualification or
admission of the unrevealed successor world; that world requires its own exact derivation, proof,
tick run, replay evidence, and public admission after the committed beacon resolves.

