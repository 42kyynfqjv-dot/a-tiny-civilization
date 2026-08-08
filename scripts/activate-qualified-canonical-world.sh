#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
data_executable="${ATINY_CIVILIZATION_DATA_EXECUTABLE:-${project_root}/target/release/civilization-data}"
expected_ruleset="${ATINY_LAUNCH_RULESET_VERSION:-32}"

usage() {
  echo "usage: $0 verify COMMITMENT.json RESOLUTION.json GENESIS_DIRECTORY EVIDENCE_DIRECTORY QUALITY_ADMISSION.json" >&2
  echo "       $0 activate COMMITMENT.json RESOLUTION.json GENESIS_DIRECTORY EVIDENCE_DIRECTORY QUALITY_ADMISSION.json --confirm-experimental-genesis" >&2
  exit 2
}

mode="${1:-}"
case "$mode" in
  verify)
    [[ $# -eq 6 ]] || usage
    ;;
  activate)
    [[ $# -eq 7 && "${7:-}" == "--confirm-experimental-genesis" ]] || usage
    ;;
  *) usage ;;
esac

commitment="$2"
resolution="$3"
genesis_directory="$4"
evidence_directory="$5"
quality_admission="$6"
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

evidence_source_commit="$(python3 -c '
import json
import sys
with open(sys.argv[1], encoding="utf-8") as source:
    value = json.load(source).get("source_commit")
if not isinstance(value, str):
    raise SystemExit("evidence source commit is absent")
print(value)
' "${evidence_directory}/evidence.json")"

genesis_manifest_digest="$(sha256sum -- "${genesis_directory}/SHA256SUMS")"
genesis_manifest_digest="${genesis_manifest_digest%% *}"
evidence_manifest_digest="$(sha256sum -- "${evidence_directory}/SHA256SUMS")"
evidence_manifest_digest="${evidence_manifest_digest%% *}"
if [[ ! "$genesis_manifest_digest" =~ ^[0-9a-f]{64}$ \
      || ! "$evidence_manifest_digest" =~ ^[0-9a-f]{64}$ ]]; then
  echo "candidate checksum manifest digest has an unexpected representation" >&2
  exit 1
fi
"${project_root}/scripts/verify-quality-world-admission.py" \
  --admission "$quality_admission" \
  --world-id "$world_id" \
  --expected-ruleset "$expected_ruleset" \
  --genesis-sha256s-sha256 "$genesis_manifest_digest" \
  --evidence-sha256s-sha256 "$evidence_manifest_digest" \
  --qualified-source-commit "$evidence_source_commit"

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
