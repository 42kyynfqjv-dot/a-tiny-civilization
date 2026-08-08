# Public canonical seed procedure

This procedure selects one world without previewing alternatives. It uses the cryptographically
verifiable League of Entropy quicknet beacon and the exact rules in ADR 0105.

## 1. Publish the future commitment

Choose one round at least ten minutes beyond the currently verified beacon and write a new artifact:

```bash
cargo run --release --locked -p civilization-data -- \
  seed commit --round ROUND --output docs/operations/CANONICAL_SEED_COMMITMENT.json
git add docs/operations/CANONICAL_SEED_COMMITMENT.json
git commit -m 'commit canonical world seed beacon'
git push origin main
```

The command itself verifies the observed beacon through two independent relays. Confirm the pushed
commit is publicly visible before the target Unix second printed by the command. Never run the
genesis preparation tool with candidate values before resolution.

## 2. Resolve exactly that commitment

After the committed target time:

```bash
cargo run --release --locked -p civilization-data -- \
  seed resolve \
  --commitment docs/operations/CANONICAL_SEED_COMMITMENT.json \
  --output docs/operations/CANONICAL_SEED_RESOLUTION.json
git add docs/operations/CANONICAL_SEED_RESOLUTION.json
git commit -m 'resolve canonical world seed beacon'
git push origin main
```

The resolver verifies matching Protocol Labs and Cloudflare responses, the pinned BLS public key,
the beacon signature, randomness derivation, target timing, and the project seed derivation. It
refuses replacement. The `world_seed` field is a decimal string suitable for
`prepare-provisional-genesis.sh`; do not transform, truncate, preview, or reroll it.

## 3. Bind genesis

Use the resolved decimal string once, with a newly chosen world UUID and a new genesis directory.
Retain both public seed artifacts beside the genesis checksums and launch evidence. If any later
gate fails, fix the gate and rerun against the same seed; never select another seed to obtain a more
interesting outcome.
