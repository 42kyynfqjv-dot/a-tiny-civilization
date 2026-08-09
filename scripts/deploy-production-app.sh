#!/usr/bin/env bash
set -euo pipefail

# Deploy the application containers on the single production host. This deliberately
# excludes the optional Docker tunnel and backup profiles: this host currently runs
# its Cloudflare tunnel as a separate system service. Hindsight and both asynchronous
# workers are part of the application deployment because cognition is a genesis gate.

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
    --confirm-public-deployment)
      confirmed=1
      shift
      ;;
    *)
      echo "usage: $0 [--env-file /absolute/path/to/production.env] --genesis-directory /absolute/path --evidence-directory /absolute/path [--runtime-root /absolute/path] --confirm-public-deployment" >&2
      exit 2
      ;;
  esac
done
if ((confirmed != 1)); then
  echo "deployment requires the literal --confirm-public-deployment argument" >&2
  exit 2
fi
if [[ -z "$genesis_directory" || -z "$evidence_directory" ]]; then
  echo "deployment requires the exact qualified genesis and evidence directories" >&2
  exit 2
fi
if [[ "$quality_admission" != /* || ! -f "$quality_admission" || -L "$quality_admission" ]]; then
  echo "deployment requires an absolute, regular quality-admission file" >&2
  exit 2
fi
if [[ "$runtime_root" != /* || ! -d "$runtime_root" || -L "$runtime_root" ]]; then
  echo "deployment requires an absolute, existing, non-symlink runtime root" >&2
  exit 2
fi

if (( EUID != 0 )); then
  echo "run this deployment helper as root; it reads a root-protected environment file" >&2
  exit 2
fi
if [[ ! -f "$environment_file" || -L "$environment_file" ]]; then
  echo "production environment file is absent or unsafe: $environment_file" >&2
  exit 2
fi
if [[ "$environment_file" != /* ]]; then
  echo "production environment file must be an absolute path" >&2
  exit 2
fi

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi

compose_args=(--env-file "$environment_file" -f compose.yaml -f compose.hindsight.yaml)
export ATINY_RUNTIME_ARTIFACT_ROOT="$runtime_root"
cd "$project_root"

# Production mutations must use one exact committed checkout. The source-bound admissions protect
# their qualified trees; this closes the remaining path through dirty operations or Compose files.
"${project_root}/scripts/verify-production-checkout.sh"

# Refuse a half-deployment when the legacy development stack still owns one of the exact host
# ports. Existing containers from this production Compose project are safe for an in-place update.
"${project_root}/scripts/production-port-preflight.sh"

# Deployment remains a separate literal authorization, but it cannot begin until the exact
# candidate evidence, quality-world admission, reviewed observer tree, production configuration,
# and staged runtime closure all pass the same composed read-only gate used by the runbook.
ATINY_QUALITY_ADMISSION_FILE="$quality_admission" \
  "${project_root}/scripts/public-genesis-preflight.sh" \
  --env-file "$environment_file" \
  --genesis-directory "$genesis_directory" \
  --evidence-directory "$evidence_directory" \
  --runtime-root "$runtime_root"
"${project_root}/scripts/provision-runtime-volumes.sh"

"${compose_command[@]}" "${compose_args[@]}" config --quiet
"${compose_command[@]}" "${compose_args[@]}" build migrate api projector runner web

# Bring up only the private persistence/cognition foundation first. Public-serving and canonical
# processes must not start until this database is already bound to the exact admitted world. A
# first genesis is prepared privately with db+migrate, activated through the qualified wrapper,
# and only then passed to this deployment command.
"${compose_command[@]}" "${compose_args[@]}" up -d db migrate local-cognition hindsight

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
for _ in $(seq 1 60); do
  if world_rows="$(
    "${compose_command[@]}" "${compose_args[@]}" exec -T db sh -c \
      "psql -U \"\$POSTGRES_USER\" -d \"\$POSTGRES_DB\" -F '|' -Atc \
        \"SELECT id::text,ruleset_version::text,current_tick::text,current_sequence::text,status::text
          FROM worlds ORDER BY id\""
  )"; then
    break
  fi
  sleep 1
done
{
  if [[ -n "$world_rows" ]]; then
    printf '%s\n' "$world_rows"
  fi
} | "${project_root}/scripts/validate-production-world-state.py" \
  --mode require-running \
  --expected-world-id "$expected_world_id" \
  --expected-ruleset "$expected_ruleset" \
  >/dev/null

"${compose_command[@]}" "${compose_args[@]}" up -d \
  api projector runner memory-worker cognition-worker
# Avoid recreating API dependencies with accidental Compose defaults while updating the
# public web container. Never use --remove-orphans: the application and any separately
# managed tunnel may span more than one Compose profile.
"${compose_command[@]}" "${compose_args[@]}" up --no-deps -d web

"${project_root}/scripts/backend-status.sh" --env-file "$environment_file" --wait-seconds 60

# A release is not successful merely because processes answer. Require the real canonical world to
# cross tick one, drain its initial Hindsight delivery, expose current privacy-safe projections,
# and replay exactly before checking the public edge.
"${project_root}/scripts/verify-live-genesis.sh" \
  --env-file "$environment_file" \
  --world-id "$expected_world_id" \
  --wait-seconds 300

# A service-level health check cannot prove that the public read model is current,
# nonempty, and privacy-safe. If this deployment contains a running world, resolve
# its committed cursor from PostgreSQL and exercise the public API contract through
# the loopback-only host binding before reporting success.
running_world_rows="$(
  "${compose_command[@]}" "${compose_args[@]}" exec -T db sh -c \
    "psql -U \"\$POSTGRES_USER\" -d \"\$POSTGRES_DB\" -F '|' -Atc \
      \"SELECT id::text,current_sequence::text FROM worlds WHERE status='running' ORDER BY id\""
)"
running_worlds=()
if [[ -n "$running_world_rows" ]]; then
  mapfile -t running_worlds <<<"$running_world_rows"
fi
if ((${#running_worlds[@]} != 1)); then
  echo "deployment lost its single running world after service startup" >&2
  exit 1
fi
IFS='|' read -r running_world_id running_world_sequence <<<"${running_worlds[0]}"
if [[ "$running_world_id" != "$expected_world_id" ]]; then
  echo "deployment world identity changed after service startup" >&2
  exit 1
fi
api_origin="$("${compose_command[@]}" "${compose_args[@]}" port api 8080)"
if [[ ! "$api_origin" =~ ^127\.0\.0\.1:[0-9]{1,5}$ ]]; then
  echo "observer API must publish exactly one IPv4 loopback port, found: ${api_origin:-none}" >&2
  exit 1
fi
"${project_root}/scripts/observer-candidate-smoke.sh" \
  "http://${api_origin}" "$running_world_id" "$running_world_sequence"

"${project_root}/scripts/verify-public-edge.sh" https://atinycivilization.com
"${project_root}/scripts/install-production-backend-monitor.sh" \
  --env-file "$environment_file" \
  --confirm-production-monitor-install
