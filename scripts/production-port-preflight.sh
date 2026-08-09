#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
environment_file="${ATINY_PRODUCTION_ENV_FILE:-/etc/a-tiny-civilization-production.env}"
if [[ "${1:-}" == '--env-file' ]]; then
  environment_file="${2:-}"
  shift 2
fi
if (($#)); then
  echo "usage: $0 [--env-file /absolute/path/to/production.env]" >&2
  exit 2
fi

"${project_root}/scripts/production-preflight.sh" --env-file "$environment_file" >/dev/null

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi
compose_args=(--env-file "$environment_file" -f compose.yaml -f compose.hindsight.yaml)
cd "$project_root"

readonly expected_project='a-tiny-civilization'
readonly -a protected_volumes=(
  'a-tiny-civilization-postgres-v1'
  'a-tiny-civilization-hindsight-v1'
  'a-tiny-civilization-hindsight-model-cache-v1'
  'atiny-ollama'
)
failure_count=0

mapfile -t public_bindings < <(
  "${compose_command[@]}" "${compose_args[@]}" config --format json | python3 -c '
import json
import sys

document = json.load(sys.stdin)
expected = (("web", 3000), ("db", 5432), ("api", 8080))
services = document.get("services", {})
for service, target in expected:
    ports = services.get(service, {}).get("ports", [])
    matches = [
        port for port in ports
        if int(port.get("target", 0)) == target and port.get("protocol", "tcp") == "tcp"
    ]
    if len(matches) != 1:
        raise SystemExit(
            f"production {service} must publish exactly one TCP mapping for container port {target}"
        )
    mapping = matches[0]
    published = str(mapping.get("published", ""))
    if mapping.get("host_ip") != "127.0.0.1" or not published.isdigit():
        raise SystemExit(f"production {service} must publish one numeric IPv4 loopback port")
    value = int(published)
    if value < 1 or value > 65535:
        raise SystemExit(f"production {service} loopback port is outside the valid range")
    print(f"{value}|{service}")
')

if ((${#public_bindings[@]} != 3)); then
  echo "production Compose render did not yield all three required loopback bindings" >&2
  exit 1
fi

for binding in "${public_bindings[@]}"; do
  IFS='|' read -r port service <<<"$binding"
  while IFS= read -r container_id; do
    [[ -n "$container_id" ]] || continue
    project="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.project" }}' "$container_id")"
    name="$(docker inspect --format '{{.Name}}' "$container_id")"
    name="${name#/}"
    if [[ "$project" != "$expected_project" ]]; then
      echo "production port ${port} (${service}) is owned by non-production container ${name}; stop the legacy/dev listener before deployment" >&2
      failure_count=$((failure_count + 1))
    fi
  done < <(docker ps --filter "publish=${port}" --format '{{.ID}}')
done

for volume in "${protected_volumes[@]}"; do
  while IFS= read -r container_id; do
    [[ -n "$container_id" ]] || continue
    project="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.project" }}' "$container_id")"
    name="$(docker inspect --format '{{.Name}}' "$container_id")"
    name="${name#/}"
    if [[ "$project" != "$expected_project" ]]; then
      echo "production/shared volume ${volume} is mounted by non-production container ${name}; stop the legacy/dev consumer before deployment" >&2
      failure_count=$((failure_count + 1))
    fi
  done < <(docker ps --filter "volume=${volume}" --format '{{.ID}}')
done

if ((failure_count > 0)); then
  echo "Production cutover has ${failure_count} conflicting listener or protected-volume consumer(s)." >&2
  exit 1
fi

printf 'Production loopback ports (%s) and protected volumes are free or owned only by the production Compose project.\n' \
  "$(printf '%s ' "${public_bindings[@]}" | sed 's/ $//')"
