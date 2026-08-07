#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${project_root}"

ATINY_REQUIRE_OFFSITE_BACKUP=1 ./scripts/production-preflight.sh

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi

compose_files=(-f compose.yaml -f compose.backup.yaml)
"${compose_command[@]}" "${compose_files[@]}" exec -T --user postgres db \
  wal-g backup-push /var/lib/postgresql/data
"${compose_command[@]}" "${compose_files[@]}" exec -T --user postgres db \
  wal-g backup-list --pretty --detail
