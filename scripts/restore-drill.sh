#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${project_root}"

: "${RESTORE_DRILL_ID:?set RESTORE_DRILL_ID to a short unique identifier}"
: "${RESTORE_WORLD_ID:?set RESTORE_WORLD_ID to the world UUID that must replay}"

if [[ ! "${RESTORE_DRILL_ID}" =~ ^[a-z0-9][a-z0-9-]{0,30}$ ]]; then
  echo "RESTORE_DRILL_ID must match ^[a-z0-9][a-z0-9-]{0,30}$" >&2
  exit 2
fi

ATINY_REQUIRE_OFFSITE_BACKUP=1 ./scripts/production-preflight.sh

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi

project_name="atiny-restore-${RESTORE_DRILL_ID}"
compose_args=(-p "${project_name}" -f compose.restore.yaml)

if docker volume inspect "${project_name}_restore-data" >/dev/null 2>&1; then
  echo "restore project ${project_name} already has state; choose a fresh RESTORE_DRILL_ID" >&2
  exit 2
fi

"${compose_command[@]}" "${compose_args[@]}" up --build \
  --abort-on-container-exit --exit-code-from restore-verifier restore-verifier

echo "Restore and deterministic replay succeeded in isolated project ${project_name}."
echo "Retaining its stopped volume for inspection. Remove it explicitly after recording the drill."
echo "Cleanup: ${compose_command[*]} ${compose_args[*]} down --volumes"
