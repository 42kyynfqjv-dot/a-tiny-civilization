#!/usr/bin/env bash
set -euo pipefail

volumes=(
  a-tiny-civilization-postgres-v1
  a-tiny-civilization-hindsight-v1
  a-tiny-civilization-hindsight-model-cache-v1
)

for volume in "${volumes[@]}"; do
  if docker volume inspect "$volume" >/dev/null 2>&1; then
    owner="$(docker volume inspect --format '{{ index .Labels "com.atinycivilization.owner" }}' "$volume")"
    schema="$(docker volume inspect --format '{{ index .Labels "com.atinycivilization.volume-schema" }}' "$volume")"
    if [[ "$owner" != "a-tiny-civilization" || "$schema" != "1" ]]; then
      echo "refusing pre-existing unlabeled or foreign runtime volume: $volume" >&2
      exit 1
    fi
  else
    docker volume create \
      --label com.atinycivilization.owner=a-tiny-civilization \
      --label com.atinycivilization.volume-schema=1 \
      "$volume" >/dev/null
  fi
done

echo "Canonical PostgreSQL and Hindsight volumes exist with project ownership labels."
