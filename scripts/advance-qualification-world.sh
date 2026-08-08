#!/usr/bin/env bash
set -euo pipefail

# Advance a disposable host-run qualification world with the same pinned DE441
# evaluator inputs used by the container runtime. This wrapper never serves or
# initializes a world and the runner itself refuses APP_ENV=production.

if [[ $# -ne 2 ]]; then
  echo "usage: $0 WORLD_ID TICKS" >&2
  exit 2
fi
if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "DATABASE_URL is required" >&2
  exit 2
fi

world_id="$1"
ticks="$2"
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runner_executable="${ATINY_CIVILIZATION_RUNNER_EXECUTABLE:-${project_root}/target/release/civilization-runner}"
data_executable="${ATINY_CIVILIZATION_DATA_EXECUTABLE:-${project_root}/target/release/civilization-data}"
de441_directory="${ATINY_JPL_DE441_INPUT_DIRECTORY:-${project_root}/data/source-cache/jpl-de441}"

if [[ ! "${world_id}" =~ ^[0-9a-fA-F-]{36}$ || ! "${ticks}" =~ ^[1-9][0-9]*$ ]]; then
  echo "WORLD_ID must be a UUID and TICKS must be a positive decimal integer" >&2
  exit 2
fi
if [[ ! -x "${runner_executable}" || ! -x "${data_executable}" ]]; then
  echo "release civilization-runner and civilization-data binaries are required" >&2
  exit 2
fi
for source in de441_part-1.bsp de441_part-2.bsp; do
  if [[ ! -f "${de441_directory}/${source}" ]]; then
    echo "missing pinned DE441 source: ${de441_directory}/${source}" >&2
    exit 2
  fi
done

APP_ENV=development \
ATINY_CIVILIZATION_DATA_EXECUTABLE="${data_executable}" \
ATINY_JPL_DE441_INPUT_DIRECTORY="${de441_directory}" \
  "${runner_executable}" advance-qualification \
    --world-id "${world_id}" \
    --ticks "${ticks}"
