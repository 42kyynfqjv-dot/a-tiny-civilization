#!/usr/bin/env bash
set -euo pipefail

readonly expected_project='a-tiny-civilization'
readonly -a public_bindings=('3000:web' '5432:db' '8080:api')

for binding in "${public_bindings[@]}"; do
  IFS=: read -r port service <<<"$binding"
  while IFS= read -r container_id; do
    [[ -n "$container_id" ]] || continue
    project="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.project" }}' "$container_id")"
    name="$(docker inspect --format '{{.Name}}' "$container_id")"
    name="${name#/}"
    if [[ "$project" != "$expected_project" ]]; then
      echo "production port ${port} (${service}) is owned by non-production container ${name}; stop the legacy/dev listener before deployment" >&2
      exit 1
    fi
  done < <(docker ps --filter "publish=${port}" --format '{{.ID}}')
done

echo "Production loopback ports are free or owned only by the production Compose project."
