#!/usr/bin/env bash
set -euo pipefail

readonly expected_project='a-tiny-civilization'
readonly -a public_bindings=('3000:web' '5432:db' '8080:api')
readonly -a protected_volumes=(
  'a-tiny-civilization-postgres-v1'
  'a-tiny-civilization-hindsight-v1'
  'a-tiny-civilization-hindsight-model-cache-v1'
  'atiny-ollama'
)
failure_count=0

for binding in "${public_bindings[@]}"; do
  IFS=: read -r port service <<<"$binding"
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

echo "Production loopback ports and protected volumes are free or owned only by the production Compose project."
