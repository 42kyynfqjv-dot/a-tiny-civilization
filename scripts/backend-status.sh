#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
environment_file="${ATINY_PRODUCTION_ENV_FILE:-/etc/a-tiny-civilization-production.env}"
wait_seconds=0

while (($#)); do
  case "$1" in
    --env-file)
      environment_file="${2:-}"
      shift 2
      ;;
    --wait-seconds)
      wait_seconds="${2:-}"
      shift 2
      ;;
    *)
      echo "usage: $0 [--env-file /absolute/path/to/production.env] [--wait-seconds N]" >&2
      exit 2
      ;;
  esac
done

maximum_age_seconds="${BACKEND_HEARTBEAT_MAX_AGE_SECONDS:-60}"
if [[ ! "$wait_seconds" =~ ^[0-9]+$ ]] || ((wait_seconds > 300)); then
  echo "--wait-seconds must be an integer from 0 through 300" >&2
  exit 2
fi
if [[ ! "$maximum_age_seconds" =~ ^[1-9][0-9]*$ ]] \
   || ((maximum_age_seconds < 15 || maximum_age_seconds > 300)); then
  echo "BACKEND_HEARTBEAT_MAX_AGE_SECONDS must be an integer from 15 through 300" >&2
  exit 2
fi

cd "$project_root"
"${project_root}/scripts/production-preflight.sh" --env-file "$environment_file" >/dev/null

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi
compose_args=(--env-file "$environment_file" -f compose.yaml -f compose.hindsight.yaml)

check_once() {
  "${compose_command[@]}" "${compose_args[@]}" exec -T api \
    curl --fail --silent http://localhost:8080/health/ready >/dev/null || return 1
  "${compose_command[@]}" "${compose_args[@]}" exec -T web \
    node -e "fetch('http://localhost:3000').then(r=>{if(!r.ok)process.exit(1)}).catch(()=>process.exit(1))" \
    >/dev/null || return 1
  "${compose_command[@]}" "${compose_args[@]}" exec -T hindsight \
    curl --fail --silent http://127.0.0.1:8888/health >/dev/null || return 1
  heartbeat_count="$("${compose_command[@]}" "${compose_args[@]}" exec -T db sh -c \
    "psql -U \"\$POSTGRES_USER\" -d \"\$POSTGRES_DB\" -Atc \"SELECT COUNT(DISTINCT service_name) FROM service_heartbeats WHERE service_name IN ('simulation-runner','observer-projector','memory-worker','cognition-worker') AND last_seen_at >= NOW() - make_interval(secs => ${maximum_age_seconds})\"")" || return 1
  [[ "$heartbeat_count" == "4" ]]
}

deadline=$((SECONDS + wait_seconds))
while ! check_once; do
  if ((SECONDS >= deadline)); then
    echo "backend is not ready: web, API, Hindsight, or a required service heartbeat is unavailable" >&2
    exit 1
  fi
  sleep 1
done

echo "Backend ready: web, API, Hindsight, runner, projector, memory, and cognition are live."
