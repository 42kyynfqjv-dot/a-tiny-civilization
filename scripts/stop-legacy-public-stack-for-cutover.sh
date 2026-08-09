#!/usr/bin/env bash
set -euo pipefail

# Stop, but never remove, the superseded public proof stack immediately before the admitted
# production deployment. Container identity is established from Compose labels rather than names,
# and every discovered service must belong to the known legacy project and this checkout. Stopped
# containers and their volumes remain available for an explicit incident rollback.

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly legacy_project='emergent-civilization'
readonly production_project='a-tiny-civilization'
readonly -a allowed_services=(
  runner cognition-worker memory-worker projector web api migrate hindsight local-cognition db
)
confirmed=0

if [[ "${1:-}" == '--confirm-legacy-public-cutover' && $# -eq 1 ]]; then
  confirmed=1
fi
if ((confirmed != 1)); then
  echo "stopping the legacy public stack requires the literal --confirm-legacy-public-cutover argument" >&2
  exit 2
fi
if ((EUID != 0)); then
  echo "run this cutover helper as root so Docker ownership cannot change between inspection and stop" >&2
  exit 2
fi

service_is_allowed() {
  local candidate="$1"
  local expected
  for expected in "${allowed_services[@]}"; do
    [[ "$candidate" == "$expected" ]] && return 0
  done
  return 1
}

mapfile -t legacy_ids < <(
  docker ps --filter "label=com.docker.compose.project=${legacy_project}" --format '{{.ID}}' | sort
)
if ((${#legacy_ids[@]} == 0)); then
  "${project_root}/scripts/production-port-preflight.sh"
  echo "No running legacy public containers remain; production cutover resources are clear."
  exit 0
fi

declare -A service_ids=()
for container_id in "${legacy_ids[@]}"; do
  labels="$(docker inspect --format \
    '{{ index .Config.Labels "com.docker.compose.project" }}|{{ index .Config.Labels "com.docker.compose.service" }}|{{ index .Config.Labels "com.docker.compose.project.working_dir" }}' \
    "$container_id")"
  IFS='|' read -r project service working_directory <<<"$labels"
  if [[ "$project" != "$legacy_project" ]]; then
    echo "legacy cutover inspection changed project identity for container ${container_id}" >&2
    exit 1
  fi
  if ! service_is_allowed "$service"; then
    echo "refusing to stop unknown service ${service:-unset} in legacy Compose project" >&2
    exit 1
  fi
  if [[ "$working_directory" != "$project_root" ]]; then
    echo "refusing to stop legacy service ${service}: unexpected working directory ${working_directory:-unset}" >&2
    exit 1
  fi
  if [[ -n "${service_ids[$service]:-}" ]]; then
    echo "refusing ambiguous legacy service ${service}: more than one running container" >&2
    exit 1
  fi
  service_ids[$service]="$container_id"
done

# A separately running admitted production container would make stopping the public proof stack an
# unsafe partial cutover. Production is deployed only after this helper has completed.
if docker ps --filter "label=com.docker.compose.project=${production_project}" --format '{{.ID}}' | grep -q .; then
  echo "refusing legacy cutover while the production Compose project is already running" >&2
  exit 1
fi

stopped_names=()
for service in "${allowed_services[@]}"; do
  container_id="${service_ids[$service]:-}"
  [[ -n "$container_id" ]] || continue
  name="$(docker inspect --format '{{.Name}}' "$container_id")"
  name="${name#/}"
  docker stop --time 60 "$container_id" >/dev/null
  stopped_names+=("$name")
done

"${project_root}/scripts/production-port-preflight.sh"
printf 'Stopped %d exact legacy container(s) without removing containers or volumes.\n' \
  "${#stopped_names[@]}"
printf 'Rollback identities: %s\n' "${stopped_names[*]}"

