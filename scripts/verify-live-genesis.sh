#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
environment_file="${ATINY_PRODUCTION_ENV_FILE:-/etc/a-tiny-civilization-production.env}"
world_id="b3ea736d-7a5a-5161-a74b-fa8c4302d333"
wait_seconds=180

while (($#)); do
  case "$1" in
    --env-file)
      environment_file="${2:-}"
      shift 2
      ;;
    --world-id)
      world_id="${2:-}"
      shift 2
      ;;
    --wait-seconds)
      wait_seconds="${2:-}"
      shift 2
      ;;
    *)
      echo "usage: $0 [--env-file /absolute/path] [--world-id UUID] [--wait-seconds 1..300]" >&2
      exit 2
      ;;
  esac
done

if [[ "$environment_file" != /* \
      || ! "$world_id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]]; then
  echo "live-genesis verification requires an absolute env path and lowercase UUID" >&2
  exit 2
fi
if [[ ! "$wait_seconds" =~ ^[1-9][0-9]*$ ]] || ((wait_seconds > 300)); then
  echo "--wait-seconds must be an integer from 1 through 300" >&2
  exit 2
fi
if ((EUID != 0)); then
  echo "run live-genesis verification as root so it can read the protected environment" >&2
  exit 2
fi

cd "$project_root"
"${project_root}/scripts/production-preflight.sh" --env-file "$environment_file" >/dev/null
compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi
compose_args=(--env-file "$environment_file" -f compose.yaml -f compose.hindsight.yaml)

query_state() {
  "${compose_command[@]}" "${compose_args[@]}" exec -T db sh -c \
    "psql -U \"\$POSTGRES_USER\" -d \"\$POSTGRES_DB\" -F '|' -Atc \"
      SELECT world.status::text,
             world.current_tick::text,
             world.current_sequence::text,
             (SELECT COUNT(*)::text FROM memory_outbox memory
              WHERE memory.world_id=world.id AND memory.completed_at IS NULL),
             (SELECT COUNT(*)::text FROM memory_outbox memory
              WHERE memory.world_id=world.id AND memory.last_error IS NOT NULL)
      FROM worlds world
      WHERE world.id='${world_id}'::UUID
        AND (SELECT COUNT(*) FROM worlds)=1\""
}

deadline=$((SECONDS + wait_seconds))
last_state="absent"
while ((SECONDS <= deadline)); do
  state="$(query_state 2>/dev/null || true)"
  if [[ -n "$state" && "$state" != *$'\n'* ]]; then
    IFS='|' read -r status tick sequence pending_memory memory_errors <<<"$state"
    last_state="status=${status:-unknown},tick=${tick:-unknown},sequence=${sequence:-unknown},pending_memory=${pending_memory:-unknown},memory_errors=${memory_errors:-unknown}"
    if [[ "$status" == "running" \
          && "$tick" =~ ^[0-9]+$ && "$sequence" =~ ^[1-9][0-9]*$ \
          && "$pending_memory" == "0" && "$memory_errors" == "0" ]] \
       && ((tick >= 1)); then
      api_origin="$("${compose_command[@]}" "${compose_args[@]}" port api 8080)"
      if [[ "$api_origin" =~ ^127\.0\.0\.1:[0-9]{1,5}$ ]] \
         && "${project_root}/scripts/observer-candidate-smoke.sh" \
              "http://${api_origin}" "$world_id" "$sequence" >/dev/null 2>&1; then
        break
      fi
    fi
  fi
  sleep 1
done
if ((SECONDS > deadline)); then
  echo "live genesis did not reach tick 1 with drained memory and current safe projections: $last_state" >&2
  exit 1
fi

"${compose_command[@]}" "${compose_args[@]}" exec -T runner \
  /app/civilization-runner verify-world --world-id "$world_id"
"${project_root}/scripts/backend-status.sh" --env-file "$environment_file" --wait-seconds 0
echo "Live genesis is replay-verified through tick ${tick}, with drained Hindsight delivery and current privacy-safe projections."
