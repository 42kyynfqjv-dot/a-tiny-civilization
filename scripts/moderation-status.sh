#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${project_root}"

maximum_age_minutes="${MODERATION_MAX_AGE_MINUTES:-60}"
queue_limit="${MODERATION_QUEUE_LIMIT:-1000}"
if [[ ! "${maximum_age_minutes}" =~ ^[1-9][0-9]*$ ]]; then
  echo "MODERATION_MAX_AGE_MINUTES must be a positive integer" >&2
  exit 2
fi
if [[ ! "${queue_limit}" =~ ^[1-9][0-9]*$ ]] || ((queue_limit > 1000)); then
  echo "MODERATION_QUEUE_LIMIT must be an integer from 1 through 1000" >&2
  exit 2
fi

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi

"${compose_command[@]}" -f compose.yaml -f compose.hindsight.yaml exec -T api \
  /app/civilization-api moderation-queue \
  --limit "${queue_limit}" \
  --max-age-minutes "${maximum_age_minutes}"
