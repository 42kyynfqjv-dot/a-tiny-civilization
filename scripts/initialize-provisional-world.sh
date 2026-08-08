#!/usr/bin/env bash
set -euo pipefail

# Atomically initialize (or exactly resume) one ruleset-26 world from the
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
migration_executable="${ATINY_CIVILIZATION_MIGRATION_EXECUTABLE:-${project_root}/target/release/civilization-api}"
data_executable="${ATINY_CIVILIZATION_DATA_EXECUTABLE:-${project_root}/target/release/civilization-data}"

if [[ ! "${world_seed}" =~ ^[0-9]+$ ]]; then
  echo "WORLD_SEED must be an unsigned decimal integer" >&2
  exit 2
fi
if [[ ! -x "${runner_executable}" ]]; then
  echo "missing executable civilization-runner binary: ${runner_executable}" >&2
  exit 2
fi
if [[ ! -x "${migration_executable}" ]]; then
  echo "missing executable civilization-api binary: ${migration_executable}" >&2
  exit 2
fi

cd "${genesis_directory}"
sha256sum --check --strict SHA256SUMS
cd "${project_root}"

modeled_candidates="${genesis_directory}/fauna-modeled-range-candidates.json"
occurrence_evidence="${genesis_directory}/local-fauna-occurrence-evidence.json"
if [[ -e "${modeled_candidates}" || -e "${occurrence_evidence}" ]]; then
  if [[ ! -f "${modeled_candidates}" || ! -f "${occurrence_evidence}" ]]; then
    echo "local-occurrence corroboration requires both modeled candidates and occurrence evidence" >&2
    exit 2
  fi
  if [[ ! -x "${data_executable}" ]]; then
    echo "missing executable civilization-data binary: ${data_executable}" >&2
    exit 2
  fi
  rederive_directory="$(mktemp -d)"
  rederived_candidates="${rederive_directory}/fauna-candidates.json"
  trap 'rm -f "${rederived_candidates}"; rmdir "${rederive_directory}"' EXIT
  "${data_executable}" derive corroborated-fauna-candidates \
    --candidates "${modeled_candidates}" \
    --occurrence-evidence "${occurrence_evidence}" \
    --output "${rederived_candidates}"
  if ! cmp -s "${rederived_candidates}" "${genesis_directory}/fauna-candidates.json"; then
    echo "fauna candidates do not match the independently rederived occurrence intersection" >&2
    exit 1
  fi
fi

# A canonical database is intentionally empty before first genesis. Apply the
# repository's idempotent migration set before the exclusive-world guard queries
# it; subsequent identical invocations are safe.
"${migration_executable}" --database-url "${DATABASE_URL}" migrate

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
  --ruleset-version 26
)
if [[ "${ATINY_REFUSE_OTHER_WORLDS:-0}" == "1" ]]; then
  arguments+=(--refuse-other-worlds)
elif [[ "${ATINY_REFUSE_OTHER_WORLDS:-0}" != "0" ]]; then
  echo "ATINY_REFUSE_OTHER_WORLDS must be 0 or 1" >&2
  exit 2
fi
if [[ -n "${predecessor_world_id}" ]]; then
  arguments+=(--predecessor-world-id "${predecessor_world_id}")
fi

"${runner_executable}" "${arguments[@]}"
"${runner_executable}" --database-url "${DATABASE_URL}" verify-world --world-id "${world_id}"
