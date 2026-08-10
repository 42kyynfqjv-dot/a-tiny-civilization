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
maximum_projection_lag="${BACKEND_PROJECTION_MAX_LAG_SEQUENCES:-100}"
maximum_async_age_seconds="${BACKEND_ASYNC_MAX_AGE_SECONDS:-300}"
minimum_free_mib="${BACKEND_MIN_FREE_MIB:-20480}"
if [[ ! "$wait_seconds" =~ ^[0-9]+$ ]] || ((wait_seconds > 300)); then
  echo "--wait-seconds must be an integer from 0 through 300" >&2
  exit 2
fi
if [[ ! "$maximum_age_seconds" =~ ^[1-9][0-9]*$ ]] \
   || ((maximum_age_seconds < 15 || maximum_age_seconds > 300)); then
  echo "BACKEND_HEARTBEAT_MAX_AGE_SECONDS must be an integer from 15 through 300" >&2
  exit 2
fi
if [[ ! "$maximum_projection_lag" =~ ^[0-9]+$ ]] || ((maximum_projection_lag > 100000)); then
  echo "BACKEND_PROJECTION_MAX_LAG_SEQUENCES must be an integer from 0 through 100000" >&2
  exit 2
fi
if [[ ! "$maximum_async_age_seconds" =~ ^[1-9][0-9]*$ ]] \
   || ((maximum_async_age_seconds < 60 || maximum_async_age_seconds > 3600)); then
  echo "BACKEND_ASYNC_MAX_AGE_SECONDS must be an integer from 60 through 3600" >&2
  exit 2
fi
if [[ ! "$minimum_free_mib" =~ ^[1-9][0-9]*$ ]] \
   || ((minimum_free_mib < 1024 || minimum_free_mib > 1048576)); then
  echo "BACKEND_MIN_FREE_MIB must be an integer from 1024 through 1048576" >&2
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
  read -r available_kib used_percent < <(df -Pk -- "$project_root" | awk 'NR == 2 { print $4, $5 }')
  [[ "$available_kib" =~ ^[0-9]+$ && "$used_percent" =~ ^[0-9]+%$ ]] || return 1
  ((available_kib >= minimum_free_mib * 1024)) || return 1
  ((10#${used_percent%%%} < 95)) || return 1
  "${compose_command[@]}" "${compose_args[@]}" exec -T api \
    curl --fail --silent http://localhost:8080/health/ready >/dev/null || return 1
  "${compose_command[@]}" "${compose_args[@]}" exec -T web \
    node -e "fetch('http://localhost:3000').then(r=>{if(!r.ok)process.exit(1)}).catch(()=>process.exit(1))" \
    >/dev/null || return 1
  "${compose_command[@]}" "${compose_args[@]}" exec -T hindsight \
    curl --fail --silent http://127.0.0.1:8888/health >/dev/null || return 1
  local_model_status="$("${compose_command[@]}" "${compose_args[@]}" exec -T hindsight \
    curl --fail --silent http://local-cognition:11434/api/tags)" || return 1
  [[ "$local_model_status" == *'"name":"qwen2.5:1.5b"'* ]] || return 1
  [[ "$local_model_status" == \
    *'"digest":"65ec06548149b04c096a120e4a6da9d4017ea809c91734ea5631e89f96ddc57b"'* \
  ]] || return 1
  data_status="$("${compose_command[@]}" "${compose_args[@]}" exec -T db sh -c \
    "psql -U \"\$POSTGRES_USER\" -d \"\$POSTGRES_DB\" -F '|' -Atc \"
      WITH active_world AS (
        SELECT id,current_sequence FROM worlds
        WHERE status IN ('initializing','running','extinct')
      ), projection AS (
        SELECT COUNT(projection_offset.*) FILTER (WHERE projection_offset.projection_name IN (
                 'public-timeline-v1','public-organism-v1',
                 'public-finding-v2','public-world-telemetry-v1','public-artifact-v1'
               ))::BIGINT AS required_count,
               COALESCE(MAX(ABS(world.current_sequence - projection_offset.through_sequence)),0)::BIGINT AS maximum_lag
        FROM active_world world
        LEFT JOIN projection_offsets projection_offset ON projection_offset.world_id=world.id
          AND projection_offset.projection_name IN (
            'public-timeline-v1','public-organism-v1',
            'public-finding-v2','public-world-telemetry-v1','public-artifact-v1'
          )
      )
      SELECT
        (SELECT COUNT(DISTINCT service_name) FROM service_heartbeats
         WHERE service_name IN ('simulation-runner','observer-projector','memory-worker','cognition-worker')
           AND last_seen_at >= NOW() - make_interval(secs => ${maximum_age_seconds})),
        (SELECT COUNT(*) FROM active_world),
        projection.required_count,
        projection.maximum_lag,
        (SELECT COUNT(*) FROM memory_outbox memory JOIN active_world world ON world.id=memory.world_id
         WHERE memory.completed_at IS NULL
           AND memory.created_at < NOW() - make_interval(secs => ${maximum_async_age_seconds})),
        (SELECT COUNT(*) FROM cognition_route_attempts attempt
         JOIN cognition_requests request USING(request_id)
         JOIN active_world world ON world.id=request.world_id
         WHERE attempt.dispatch_state='dispatched'
           AND attempt.dispatched_at < NOW() - make_interval(secs => ${maximum_async_age_seconds}))
          +
        (SELECT COUNT(*) FROM cognition_requests request
         JOIN active_world world ON world.id=request.world_id
         LEFT JOIN cognition_results result USING(request_id)
         LEFT JOIN cognition_deadline_latches latch USING(request_id)
         WHERE result.request_id IS NULL AND latch.request_id IS NULL
           AND request.claimed_at < NOW() - make_interval(secs => ${maximum_async_age_seconds}))
        ,current_setting('data_checksums')
        ,current_setting('fsync')
        ,current_setting('synchronous_commit')
        ,current_setting('full_page_writes')
      FROM projection\"")" || return 1
  IFS='|' read -r heartbeat_count active_world_count projection_count projection_lag \
    stale_memory_count stuck_cognition_count data_checksums fsync synchronous_commit \
    full_page_writes <<<"$data_status"
  [[ "$data_checksums" == "on" ]] || return 1
  [[ "$fsync" == "on" ]] || return 1
  [[ "$synchronous_commit" == "on" ]] || return 1
  [[ "$full_page_writes" == "on" ]] || return 1
  [[ "$heartbeat_count" == "4" ]] || return 1
  [[ "$active_world_count" == "0" || "$active_world_count" == "1" ]] || return 1
  if [[ "$active_world_count" == "1" ]]; then
    [[ "$projection_count" == "5" ]] || return 1
    ((projection_lag <= maximum_projection_lag)) || return 1
    [[ "$stale_memory_count" == "0" ]] || return 1
    [[ "$stuck_cognition_count" == "0" ]] || return 1
  fi
}

deadline=$((SECONDS + wait_seconds))
while ! check_once; do
  if ((SECONDS >= deadline)); then
    echo "backend is not ready: disk capacity, PostgreSQL durability, an endpoint, local model, heartbeat, projection, memory delivery, or cognition dispatch is unhealthy" >&2
    exit 1
  fi
  sleep 1
done

echo "Backend ready: disk capacity and services are healthy, and the active world's projections, memory, and cognition are within bounds."
