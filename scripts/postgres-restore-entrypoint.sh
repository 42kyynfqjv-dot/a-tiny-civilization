#!/usr/bin/env bash
set -euo pipefail

: "${PGDATA:?PGDATA is required}"

if [[ -e "${PGDATA}/PG_VERSION" ]]; then
  echo "restore target is not empty; use a fresh restore-drill project" >&2
  exit 2
fi

install -d -m 0700 -o postgres -g postgres "${PGDATA}"
gosu postgres wal-g backup-fetch "${PGDATA}" "${RESTORE_BACKUP_NAME:-LATEST}"

touch "${PGDATA}/recovery.signal"
chown postgres:postgres "${PGDATA}/recovery.signal"
printf '%s\n' \
  "restore_command = 'wal-g wal-fetch \"%f\" \"%p\"'" \
  "recovery_target_action = 'promote'" \
  >>"${PGDATA}/postgresql.auto.conf"
chown postgres:postgres "${PGDATA}/postgresql.auto.conf"

exec docker-entrypoint.sh postgres
