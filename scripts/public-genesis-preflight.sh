#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
environment_file="${ATINY_PRODUCTION_ENV_FILE:-/etc/a-tiny-civilization-production.env}"
genesis_directory="${ATINY_CANONICAL_GENESIS_DIRECTORY:-}"
evidence_directory="${ATINY_QUALIFICATION_EVIDENCE_DIRECTORY:-}"
admission_file="${ATINY_QUALITY_ADMISSION_FILE:-${project_root}/docs/operations/QUALITY_WORLD_ADMISSION_RULESET30_2026-08-08.json}"
observatory_admission_file="${ATINY_PUBLIC_OBSERVATORY_ADMISSION_FILE:-${project_root}/docs/operations/PUBLIC_OBSERVATORY_ADMISSION_2026-08-08.json}"
runtime_root="${ATINY_RUNTIME_ARTIFACT_ROOT:-${project_root}/runtime-artifacts}"

usage() {
  echo "usage: $0 --genesis-directory PATH --evidence-directory PATH [--env-file PATH] [--admission-file PATH] [--observatory-admission-file PATH] [--runtime-root PATH]" >&2
  exit 2
}

while (($#)); do
  case "$1" in
    --env-file)
      environment_file="${2:-}"
      shift 2
      ;;
    --genesis-directory)
      genesis_directory="${2:-}"
      shift 2
      ;;
    --evidence-directory)
      evidence_directory="${2:-}"
      shift 2
      ;;
    --admission-file)
      admission_file="${2:-}"
      shift 2
      ;;
    --observatory-admission-file)
      observatory_admission_file="${2:-}"
      shift 2
      ;;
    --runtime-root)
      runtime_root="${2:-}"
      shift 2
      ;;
    *) usage ;;
  esac
done

if [[ -z "$genesis_directory" || -z "$evidence_directory" ]]; then
  usage
fi
for path in "$environment_file" "$genesis_directory" "$evidence_directory" "$admission_file" "$observatory_admission_file" "$runtime_root"; do
  if [[ "$path" != /* ]]; then
    echo "public-genesis preflight paths must be absolute: $path" >&2
    exit 2
  fi
done

"${project_root}/scripts/production-preflight.sh" --env-file "$environment_file"
"${project_root}/scripts/activate-qualified-canonical-world.sh" verify \
  "${project_root}/docs/operations/CANONICAL_SEED_COMMITMENT.json" \
  "${project_root}/docs/operations/CANONICAL_SEED_RESOLUTION.json" \
  "$genesis_directory" "$evidence_directory" "$admission_file"
"${project_root}/scripts/verify-public-observatory-admission.py" \
  --admission "$observatory_admission_file"
ATINY_CIVILIZATION_DATA_EXECUTABLE="${project_root}/target/release/civilization-data" \
  "${project_root}/scripts/verify-staged-runtime-artifacts.sh" "$runtime_root"

echo "Public-genesis preflight passed without creating a world, changing services, or deploying a site."
