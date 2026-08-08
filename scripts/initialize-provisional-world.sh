#!/usr/bin/env bash
set -euo pipefail

# Atomically initialize (or exactly resume) one ruleset-22 world from the
# artifacts produced by prepare-provisional-genesis.sh. The runner is not started.

if (( $# < 3 || $# > 4 )); then
  echo "usage: $0 WORLD_ID WORLD_SEED GENESIS_DIRECTORY [PREDECESSOR_WORLD_ID]" >&2
  exit 2
fi
if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "DATABASE_URL is required" >&2
  exit 2
fi

world_id="$1"
world_seed="$2"
genesis_directory="$(realpath "$3")"
predecessor_world_id="${4:-}"
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runner_executable="${ATINY_CIVILIZATION_RUNNER_EXECUTABLE:-${project_root}/target/release/civilization-runner}"

if [[ ! "${world_seed}" =~ ^[0-9]+$ ]]; then
  echo "WORLD_SEED must be an unsigned decimal integer" >&2
  exit 2
fi
if [[ ! -x "${runner_executable}" ]]; then
  echo "missing executable civilization-runner binary: ${runner_executable}" >&2
  exit 2
fi

cd "${genesis_directory}"
sha256sum --check --strict SHA256SUMS
cd "${project_root}"

arguments=(
  --database-url "${DATABASE_URL}"
  init-provisional-full-earth
  --world-id "${world_id}"
  --seed "${world_seed}"
  --composition data/provisional/full-earth-breadth-first-0.1.1.json
  --artifact-root .
  --provisional-land-origin-selection "${genesis_directory}/origin-selection.json"
  --provisional-origin-environment "${genesis_directory}/origin-environment.json"
  --fauna-range-candidates "${genesis_directory}/fauna-candidates.json"
  --fauna-seeded-selection "${genesis_directory}/fauna-selection.json"
  --fauna-origin-environment "${genesis_directory}/origin-environment.json"
  --fauna-population-plan "${genesis_directory}/fauna-population-plan.json"
  --provisional-organism-profile-plan "${genesis_directory}/organism-body-profile-plan.json"
  --provisional-material-resource-plan "${genesis_directory}/material-resource-plan.json"
  --tick-duration-seconds 300
  --max-events-per-partition-transition 10000
  --ruleset-version 22
)
if [[ -n "${predecessor_world_id}" ]]; then
  arguments+=(--predecessor-world-id "${predecessor_world_id}")
fi

"${runner_executable}" "${arguments[@]}"
"${runner_executable}" --database-url "${DATABASE_URL}" verify-world --world-id "${world_id}"
