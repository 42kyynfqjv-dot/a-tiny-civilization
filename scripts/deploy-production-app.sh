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
    --confirm-public-deployment)
      confirmed=1
      shift
      ;;
    *)
      echo "usage: $0 [--env-file /absolute/path/to/production.env] --genesis-directory /absolute/path --evidence-directory /absolute/path --confirm-public-deployment" >&2
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
cd "$project_root"

# Deployment remains a separate literal authorization, but it cannot begin until the exact
# candidate evidence, quality-world admission, reviewed observer tree, production configuration,
# and staged runtime closure all pass the same composed read-only gate used by the runbook.
"${project_root}/scripts/public-genesis-preflight.sh" \
  --env-file "$environment_file" \
  --genesis-directory "$genesis_directory" \
  --evidence-directory "$evidence_directory" \
  --runtime-root "${project_root}/runtime-artifacts"
"${project_root}/scripts/provision-runtime-volumes.sh"

"${compose_command[@]}" "${compose_args[@]}" config --quiet
"${compose_command[@]}" "${compose_args[@]}" build migrate api projector runner web
"${compose_command[@]}" "${compose_args[@]}" up -d \
  db migrate local-cognition hindsight api projector runner memory-worker cognition-worker
# Avoid recreating API dependencies with accidental Compose defaults while updating the
# public web container. Never use --remove-orphans: the application and any separately
# managed tunnel may span more than one Compose profile.
"${compose_command[@]}" "${compose_args[@]}" up --no-deps -d web

"${project_root}/scripts/backend-status.sh" --env-file "$environment_file" --wait-seconds 60

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
if ((${#running_worlds[@]} > 1)); then
  echo "deployment contains more than one running world" >&2
  exit 1
fi
if ((${#running_worlds[@]} == 1)); then
  IFS='|' read -r running_world_id running_world_sequence <<<"${running_worlds[0]}"
  api_origin="$("${compose_command[@]}" "${compose_args[@]}" port api 8080)"
  if [[ ! "$api_origin" =~ ^127\.0\.0\.1:[0-9]{1,5}$ ]]; then
    echo "observer API must publish exactly one IPv4 loopback port, found: ${api_origin:-none}" >&2
    exit 1
  fi
  "${project_root}/scripts/observer-candidate-smoke.sh" \
    "http://${api_origin}" "$running_world_id" "$running_world_sequence"
fi

"${project_root}/scripts/verify-public-edge.sh" https://atinycivilization.com
