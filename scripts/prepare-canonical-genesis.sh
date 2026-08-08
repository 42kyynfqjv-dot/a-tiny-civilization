#!/usr/bin/env bash
set -euo pipefail

if (($# < 3 || $# > 4)); then
  echo "usage: $0 COMMITMENT.json RESOLUTION.json OUTPUT_DIRECTORY [SPECIES_LIMIT]" >&2
  exit 2
fi

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
commitment="$1"
resolution="$2"
output_directory="$3"
species_limit="${4:-32}"
data_executable="${ATINY_CIVILIZATION_DATA_EXECUTABLE:-${project_root}/target/release/civilization-data}"

if [[ ! -x "$data_executable" ]]; then
  echo "civilization-data executable is absent: $data_executable" >&2
  exit 2
fi
if [[ -z "${ATINY_LOCAL_OCCURRENCE_SOURCE_DIRECTORY:-}" ]]; then
  echo "canonical preparation requires ATINY_LOCAL_OCCURRENCE_SOURCE_DIRECTORY" >&2
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

ATINY_CIVILIZATION_DATA_EXECUTABLE="$data_executable" \
ATINY_REQUIRE_LOCAL_OCCURRENCE_EVIDENCE=1 \
  "${project_root}/scripts/prepare-provisional-genesis.sh" \
  "$world_seed" "$output_directory" "$species_limit"

echo "Prepared canonical world ${world_id} from the verified public seed ${world_seed}."
