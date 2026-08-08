#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
composition="data/provisional/full-earth-breadth-first-0.1.1.json"

cd "$project_root"

python3 - "$composition" <<'PY'
import hashlib, json, pathlib, sys

path = pathlib.Path(sys.argv[1])
raw = path.read_bytes()
if len(raw) != 9842 or hashlib.sha256(raw).hexdigest() != "f43b5e02b6dc5660390a2f3162967d2d18ab95a29652cf07c7ed763fa8c6bb18":
    raise SystemExit("active provisional composition bytes changed")
value = json.loads(raw)
physiology = next(item for item in value["world_components"] if item["kind"] == "fauna_physiology_evidence")
release = physiology["release"]
if release["artifact_path"] != "data/derived-cache/fauna-physiology-catalog-v2.json":
    raise SystemExit("active composition does not pin normalized fauna physiology v2")
PY

if [[ "${ATINY_VERIFY_FULL_PROVISIONAL_CLOSURE:-0}" == "1" ]]; then
  data_executable="${ATINY_CIVILIZATION_DATA_EXECUTABLE:-${project_root}/target/release/civilization-data}"
  if [[ ! -x "$data_executable" ]]; then
    echo "missing civilization-data executable: $data_executable" >&2
    exit 2
  fi
  validation="$($data_executable validate-provisional --artifact-root . "$composition")"
  grep -qx 'composition: full-earth-breadth-first@0.1.1' <<<"$validation"
  grep -qx 'artifacts: 147466 (10164215509 bytes verified)' <<<"$validation"
elif [[ "${ATINY_VERIFY_FULL_PROVISIONAL_CLOSURE:-0}" != "0" ]]; then
  echo "ATINY_VERIFY_FULL_PROVISIONAL_CLOSURE must be 0 or 1" >&2
  exit 2
fi

for source in \
  apps/runner/src/main.rs \
  scripts/prepare-provisional-genesis.sh \
  scripts/initialize-provisional-world.sh \
  scripts/stage-provisional-runner-artifacts.sh; do
  if ! grep -qF "$composition" "$source"; then
    echo "active genesis entry point does not pin composition 0.1.1: $source" >&2
    exit 1
  fi
done
if grep -qF 'full-earth-breadth-first-0.1.0.json' \
  apps/runner/src/main.rs \
  scripts/prepare-provisional-genesis.sh \
  scripts/initialize-provisional-world.sh \
  scripts/stage-provisional-runner-artifacts.sh; then
  echo "an active genesis entry point still selects composition 0.1.0" >&2
  exit 1
fi

grep -q -- '--ruleset-version 20' scripts/initialize-provisional-world.sh
echo "Ruleset-20 provisional genesis pins one verified composition and artifact revision."
