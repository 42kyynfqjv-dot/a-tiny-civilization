#!/usr/bin/env bash
set -euo pipefail

# The private preparation phase starts only PostgreSQL and migrations. It must not inherit the
# broader public-cutover requirement to stop web, API, or local cognition before they are touched.

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
environment_file="${ATINY_PRODUCTION_ENV_FILE:-/etc/a-tiny-civilization-production.env}"

if [[ "${1:-}" == "--env-file" ]]; then
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
database_port="$("${compose_command[@]}" "${compose_args[@]}" config --format json | python3 -c '
import json
import sys

document = json.load(sys.stdin)
ports = document.get("services", {}).get("db", {}).get("ports", [])
matches = [
    port for port in ports
    if int(port.get("target", 0)) == 5432 and port.get("protocol", "tcp") == "tcp"
]
if len(matches) != 1:
    raise SystemExit("production database must publish exactly one TCP mapping for container port 5432")
mapping = matches[0]
published = str(mapping.get("published", ""))
if mapping.get("host_ip") != "127.0.0.1" or not published.isdigit():
    raise SystemExit("production database must publish one numeric IPv4 loopback port")
value = int(published)
if value < 1 or value > 65535:
    raise SystemExit("production database loopback port is outside the valid range")
print(value)
')"

readonly expected_project='a-tiny-civilization'
failure_count=0
while IFS= read -r container_id; do
  [[ -n "$container_id" ]] || continue
  project="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.project" }}' "$container_id")"
  name="$(docker inspect --format '{{.Name}}' "$container_id")"
  name="${name#/}"
  if [[ "$project" != "$expected_project" ]]; then
    echo "private production database port ${database_port} is owned by non-production container ${name}" >&2
    failure_count=$((failure_count + 1))
  fi
done < <(docker ps --filter "publish=${database_port}" --format '{{.ID}}')

readonly postgres_volume='a-tiny-civilization-postgres-v1'
while IFS= read -r container_id; do
  [[ -n "$container_id" ]] || continue
  project="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.project" }}' "$container_id")"
  name="$(docker inspect --format '{{.Name}}' "$container_id")"
  name="${name#/}"
  if [[ "$project" != "$expected_project" ]]; then
    echo "production PostgreSQL volume is mounted by non-production container ${name}" >&2
    failure_count=$((failure_count + 1))
  fi
done < <(docker ps --filter "volume=${postgres_volume}" --format '{{.ID}}')

if ((failure_count > 0)); then
  echo "Private database preparation has ${failure_count} conflicting owner(s)." >&2
  exit 1
fi

echo "Private production PostgreSQL loopback port ${database_port} and protected volume are free or production-owned."
