#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${project_root}"

max_age_seconds="${BACKUP_MAX_AGE_SECONDS:-93600}"
if [[ ! "${max_age_seconds}" =~ ^[1-9][0-9]*$ ]]; then
  echo "BACKUP_MAX_AGE_SECONDS must be a positive integer" >&2
  exit 2
fi

ATINY_REQUIRE_OFFSITE_BACKUP=1 ./scripts/production-preflight.sh

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi
compose_files=(-f compose.yaml -f compose.backup.yaml)

archiver="$("${compose_command[@]}" "${compose_files[@]}" exec -T db \
  psql --no-psqlrc --tuples-only --no-align \
  --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" \
  --command "SELECT current_setting('archive_mode'), archived_count, failed_count, COALESCE(EXTRACT(EPOCH FROM last_archived_time)::bigint, 0), COALESCE(EXTRACT(EPOCH FROM last_failed_time)::bigint, 0) FROM pg_stat_archiver;")"

IFS='|' read -r archive_mode archived_count failed_count last_archived_epoch last_failed_epoch \
  <<<"${archiver}"
if [[ "${archive_mode}" != "on" ]]; then
  echo "PostgreSQL archive_mode is not on" >&2
  exit 1
fi
if (( last_failed_epoch > last_archived_epoch )); then
  echo "the most recent PostgreSQL archive attempt failed" >&2
  exit 1
fi

backup_list_file="$(mktemp)"
trap 'rm -f "${backup_list_file}"' EXIT
"${compose_command[@]}" "${compose_files[@]}" exec -T --user postgres db \
  wal-g backup-list --json >"${backup_list_file}"

python3 - "${max_age_seconds}" "${backup_list_file}" <<'PY'
import datetime
import json
import pathlib
import sys

maximum_age = int(sys.argv[1])
backups = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
if not backups:
    raise SystemExit("WAL-G reports no base backups")
latest = max(backups, key=lambda backup: backup["time"])
created = datetime.datetime.fromisoformat(latest["time"].replace("Z", "+00:00"))
age = int((datetime.datetime.now(datetime.timezone.utc) - created).total_seconds())
if age < 0:
    raise SystemExit("latest WAL-G backup timestamp is in the future")
if age > maximum_age:
    raise SystemExit(f"latest WAL-G backup is stale: {age}s > {maximum_age}s")
print(f"latest base backup: {latest['backup_name']} ({age}s old)")
PY

echo "WAL archiver: ${archived_count} succeeded, ${failed_count} failed; latest attempt healthy."
