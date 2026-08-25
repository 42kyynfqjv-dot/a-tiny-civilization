#!/usr/bin/env bash
set -euo pipefail

# Atomically initialize (or exactly resume) one current regular world from the
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
ruleset_version="${ATINY_LAUNCH_RULESET_VERSION:-42}"

if [[ ! "${world_seed}" =~ ^[0-9]+$ ]]; then
  echo "WORLD_SEED must be an unsigned decimal integer" >&2
  exit 2
fi
if [[ ! "${ruleset_version}" =~ ^[1-9][0-9]*$ ]]; then
  echo "ATINY_LAUNCH_RULESET_VERSION must be a positive decimal integer" >&2
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

rederive_directory="$(mktemp -d)"
rederived_candidates=""
rederived_climate=""
rederived_climate_normals=""
cleanup() {
  if [[ -n "${rederived_candidates}" ]]; then rm -f "${rederived_candidates}"; fi
  if [[ -n "${rederived_climate}" ]]; then rm -f "${rederived_climate}"; fi
  if [[ -n "${rederived_climate_normals}" ]]; then rm -f "${rederived_climate_normals}"; fi
  rmdir "${rederive_directory}"
}
trap cleanup EXIT

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
  rederived_candidates="${rederive_directory}/fauna-candidates.json"
  "${data_executable}" derive corroborated-fauna-candidates \
    --candidates "${modeled_candidates}" \
    --occurrence-evidence "${occurrence_evidence}" \
    --output "${rederived_candidates}"
  if ! cmp -s "${rederived_candidates}" "${genesis_directory}/fauna-candidates.json"; then
    echo "fauna candidates do not match the independently rederived occurrence intersection" >&2
    exit 1
  fi
fi

origin_climate="${genesis_directory}/origin-climate-evidence.json"
if [[ ! -f "${origin_climate}" ]]; then
  echo "provisional origin climate evidence is required" >&2
  exit 2
fi
if [[ ! -x "${data_executable}" ]]; then
  echo "missing executable civilization-data binary: ${data_executable}" >&2
  exit 2
fi
rederived_climate="${rederive_directory}/origin-climate-evidence.json"
"${data_executable}" derive provisional-origin-climate-evidence \
  --origin-selection "${genesis_directory}/origin-selection.json" \
  --source-snapshot data/source-snapshots/era5-single-levels-monthly-means-1981-2010.json \
  --artifact-root data/source-cache \
  --output "${rederived_climate}"
if ! cmp -s "${rederived_climate}" "${origin_climate}"; then
  echo "origin climate evidence does not match the independently rederived ERA5 source values" >&2
  exit 1
fi

origin_climate_normals="${genesis_directory}/origin-climate-normals.json"
if [[ ! -f "${origin_climate_normals}" ]]; then
  echo "provisional origin climate normals are required" >&2
  exit 2
fi
rederived_climate_normals="${rederive_directory}/origin-climate-normals.json"
"${data_executable}" derive provisional-origin-climate-normals \
  --evidence "${origin_climate}" \
  --output "${rederived_climate_normals}"
if ! cmp -s "${rederived_climate_normals}" "${origin_climate_normals}"; then
  echo "origin climate normals do not match the independently rederived ERA5 evidence summaries" >&2
  exit 1
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
  --composition data/provisional/full-earth-breadth-first-0.1.2.json
  --artifact-root .
  --provisional-land-origin-selection "${genesis_directory}/origin-selection.json"
  --provisional-origin-environment "${genesis_directory}/origin-environment.json"
  --provisional-origin-climate-evidence "${origin_climate}"
  --provisional-origin-climate-normals "${origin_climate_normals}"
  --fauna-range-candidates "${genesis_directory}/fauna-candidates.json"
  --fauna-seeded-selection "${genesis_directory}/fauna-selection.json"
  --fauna-origin-environment "${genesis_directory}/origin-environment.json"
  --fauna-population-plan "${genesis_directory}/fauna-population-plan.json"
  --fauna-ecology-profile-set data/derived-cache/eltontraits-ecology-v2.json
  --fauna-ecology-plan "${genesis_directory}/fauna-ecology-plan.json"
  --provisional-organism-profile-plan "${genesis_directory}/organism-body-profile-plan.json"
  --provisional-material-resource-plan "${genesis_directory}/material-resource-plan.json"
  --tick-duration-seconds 300
  --max-events-per-partition-transition 10000
  --ruleset-version "${ruleset_version}"
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
