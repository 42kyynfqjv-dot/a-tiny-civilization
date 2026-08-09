#!/usr/bin/env bash
set -euo pipefail

# Prepare only the private PostgreSQL foundation needed for canonical activation. This command
# cannot start the runner, observer API, web origin, tunnel, or asynchronous workers, and it does
# not create a world. Public deployment remains a separate literal-confirmation operation.

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
environment_file="${ATINY_PRODUCTION_ENV_FILE:-/etc/a-tiny-civilization-production.env}"
genesis_directory="${ATINY_CANONICAL_GENESIS_DIRECTORY:-}"
evidence_directory="${ATINY_QUALIFICATION_EVIDENCE_DIRECTORY:-}"
quality_admission="${ATINY_QUALITY_ADMISSION_FILE:-${project_root}/docs/operations/QUALITY_WORLD_ADMISSION_RULESET32_2026-08-09.json}"
runtime_root="${ATINY_RUNTIME_ARTIFACT_ROOT:-${project_root}/runtime-artifacts}"
confirmed=0

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
    --runtime-root)
      runtime_root="${2:-}"
      shift 2
      ;;
    --confirm-private-database-preparation)
      confirmed=1
      shift
      ;;
    *)
      echo "usage: $0 [--env-file /absolute/path/to/production.env] --genesis-directory /absolute/path --evidence-directory /absolute/path [--runtime-root /absolute/path] --confirm-private-database-preparation" >&2
      exit 2
      ;;
  esac
done
if ((confirmed != 1)); then
  echo "private database preparation requires the literal --confirm-private-database-preparation argument" >&2
  exit 2
fi
if [[ -z "$genesis_directory" || -z "$evidence_directory" ]]; then
  echo "private database preparation requires the exact qualified genesis and evidence directories" >&2
  exit 2
fi
if [[ "$quality_admission" != /* || ! -f "$quality_admission" || -L "$quality_admission" ]]; then
  echo "private database preparation requires an absolute, regular quality-admission file" >&2
  exit 2
fi
if [[ "$runtime_root" != /* || ! -d "$runtime_root" || -L "$runtime_root" ]]; then
  echo "private database preparation requires an absolute, existing, non-symlink runtime root" >&2
  exit 2
fi
if ((EUID != 0)); then
  echo "run this preparation helper as root; it reads a root-protected environment file" >&2
  exit 2
fi
if [[ ! -f "$environment_file" || -L "$environment_file" || "$environment_file" != /* ]]; then
  echo "production environment file is absent or unsafe: $environment_file" >&2
  exit 2
fi

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi
compose_args=(--env-file "$environment_file" -f compose.yaml -f compose.hindsight.yaml)
export ATINY_RUNTIME_ARTIFACT_ROOT="$runtime_root"
cd "$project_root"

"${project_root}/scripts/verify-production-checkout.sh"
"${project_root}/scripts/production-port-preflight.sh"
ATINY_QUALITY_ADMISSION_FILE="$quality_admission" \
  "${project_root}/scripts/public-genesis-preflight.sh" \
  --env-file "$environment_file" \
  --genesis-directory "$genesis_directory" \
  --evidence-directory "$evidence_directory" \
  --runtime-root "$runtime_root"
"${project_root}/scripts/provision-runtime-volumes.sh"

"${compose_command[@]}" "${compose_args[@]}" config --quiet
"${compose_command[@]}" "${compose_args[@]}" build migrate
"${compose_command[@]}" "${compose_args[@]}" up -d db migrate

expected_world_id="$(python3 -c '
import json
import sys
with open(sys.argv[1], encoding="utf-8") as source:
    value = json.load(source).get("world_id")
if not isinstance(value, str):
    raise SystemExit("qualification evidence world identity is absent")
print(value)
' "${evidence_directory}/evidence.json")"
expected_ruleset="$(python3 -c '
import json
import sys
with open(sys.argv[1], encoding="utf-8") as source:
    value = json.load(source).get("ruleset_version")
if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
    raise SystemExit("quality admission ruleset is absent")
print(value)
' "$quality_admission")"

world_rows=""
database_ready=0
for _ in $(seq 1 60); do
  if world_rows="$(
    "${compose_command[@]}" "${compose_args[@]}" exec -T db sh -c \
      "psql -U \"\$POSTGRES_USER\" -d \"\$POSTGRES_DB\" -F '|' -Atc \
        \"SELECT id::text,ruleset_version::text,current_tick::text,current_sequence::text,status::text
          FROM worlds ORDER BY id\""
  )"; then
    database_ready=1
    break
  fi
  sleep 1
done
if ((database_ready != 1)); then
  echo "private production database did not become migration-ready within 60 seconds" >&2
  exit 1
fi

validated_state="$({
  if [[ -n "$world_rows" ]]; then
    printf '%s\n' "$world_rows"
  fi
} | "${project_root}/scripts/validate-production-world-state.py" \
      --mode allow-empty \
      --expected-world-id "$expected_world_id" \
      --expected-ruleset "$expected_ruleset")"
if [[ "$validated_state" == *'"status":"empty-ready-for-qualified-activation"'* ]]; then
  echo "Private production database is migration-ready and empty for qualified activation of ${expected_world_id}."
  exit 0
fi
echo "Private production database already contains the exact running qualified world; no world state changed."
