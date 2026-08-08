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
refuses replacement. The resolution binds both `world_seed` and `world_id`. The offline
`seed verify` command recomputes both and prints them only after checking the complete
commitment/resolution pair. Do not transform, truncate, preview, or reroll either value.

## 3. Bind genesis

Use the checked wrappers so neither value can be mistyped or operator-selected:

```bash
read -r WORLD_ID WORLD_SEED < <(
  target/release/civilization-data seed verify \
    --commitment docs/operations/CANONICAL_SEED_COMMITMENT.json \
    --resolution docs/operations/CANONICAL_SEED_RESOLUTION.json
)
./scripts/acquire-inaturalist-origin-observations.py \
  --latitude-e7 236449522 \
  --longitude-e7 -1034974258 \
  --radius-kilometers 75 \
  --output-directory "/var/lib/a-tiny-civilization/sources/$WORLD_ID-local-fauna"
ATINY_LOCAL_OCCURRENCE_SOURCE_DIRECTORY="/var/lib/a-tiny-civilization/sources/$WORLD_ID-local-fauna" \
  ./scripts/prepare-canonical-genesis.sh \
  docs/operations/CANONICAL_SEED_COMMITMENT.json \
  docs/operations/CANONICAL_SEED_RESOLUTION.json \
  "/var/lib/a-tiny-civilization/genesis/$WORLD_ID" 32

DATABASE_URL=... ./scripts/activate-qualified-canonical-world.sh activate \
  docs/operations/CANONICAL_SEED_COMMITMENT.json \
  docs/operations/CANONICAL_SEED_RESOLUTION.json \
  "/var/lib/a-tiny-civilization/genesis/$WORLD_ID" \
  "$QUALIFICATION_EVIDENCE_DIRECTORY" \
  --confirm-experimental-genesis
```

Retain both public seed artifacts beside the genesis checksums and launch evidence. If any later
gate fails, fix the gate and rerun against the same seed and derived world ID; never select another
seed to obtain a more interesting outcome.

The coordinates above are the exact geographic centre of the already committed L10 origin. The
bounded observation query is presence corroboration only; it is not an abundance, native-status,
or habitat-suitability source. Canonical preparation refuses to proceed without it, and
initialization independently rederives the range/occurrence intersection.
