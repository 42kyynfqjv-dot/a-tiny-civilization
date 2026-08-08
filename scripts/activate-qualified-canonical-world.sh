#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
data_executable="${ATINY_CIVILIZATION_DATA_EXECUTABLE:-${project_root}/target/release/civilization-data}"
expected_ruleset="${ATINY_LAUNCH_RULESET_VERSION:-30}"

usage() {
  echo "usage: $0 verify COMMITMENT.json RESOLUTION.json GENESIS_DIRECTORY EVIDENCE_DIRECTORY" >&2
  echo "       $0 activate COMMITMENT.json RESOLUTION.json GENESIS_DIRECTORY EVIDENCE_DIRECTORY --confirm-experimental-genesis" >&2
  exit 2
}

mode="${1:-}"
case "$mode" in
  verify)
    [[ $# -eq 5 ]] || usage
    ;;
  activate)
    [[ $# -eq 6 && "${6:-}" == "--confirm-experimental-genesis" ]] || usage
    ;;
  *) usage ;;
esac

commitment="$2"
resolution="$3"
genesis_directory="$4"
evidence_directory="$5"
if [[ ! "$expected_ruleset" =~ ^[1-9][0-9]*$ ]]; then
  echo "ATINY_LAUNCH_RULESET_VERSION must be a positive integer" >&2
  exit 2
fi
if [[ ! -x "$data_executable" ]]; then
  echo "civilization-data executable is absent: $data_executable" >&2
  exit 2
fi

identity="$($data_executable seed verify --commitment "$commitment" --resolution "$resolution")"
read -r world_id world_seed extra <<<"$identity"
if [[ -n "${extra:-}" \
      || ! "$world_id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ \
      || ! "$world_seed" =~ ^[0-9]+$ ]]; then
  echo "verified public seed identity has an unexpected representation" >&2
  exit 1
fi

"${project_root}/scripts/verify-launch-candidate-evidence.py" \
  --world-id "$world_id" \
  --genesis-directory "$genesis_directory" \
  --evidence-directory "$evidence_directory" \
  --expected-ruleset "$expected_ruleset" \
  --minimum-tick 1000

if [[ "$mode" == "verify" ]]; then
  echo "Qualified canonical world ${world_id} is ready for a separate deliberate activation."
  exit 0
fi
if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "DATABASE_URL is required only for activate mode" >&2
  exit 2
fi

"${project_root}/scripts/initialize-canonical-world.sh" \
  "$commitment" "$resolution" "$genesis_directory"
echo "Activated qualified experimental genesis for ${world_id} at tick zero; no public deployment was performed."
