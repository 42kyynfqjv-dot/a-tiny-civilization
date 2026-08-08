#!/usr/bin/env bash
set -euo pipefail

# Advance a tick-zero qualification world without racing the first local cognition
# result past its immutable simulation deadline. Memory and cognition workers must
# already be running. This wrapper never starts, initializes, or serves a world.

if [[ $# -ne 2 ]]; then
  echo "usage: $0 WORLD_ID TOTAL_TICKS" >&2
  exit 2
fi
if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "DATABASE_URL is required" >&2
  exit 2
fi

world_id="$1"
total_ticks="$2"
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
advance_executable="${ATINY_ADVANCE_QUALIFICATION_EXECUTABLE:-${project_root}/scripts/advance-qualification-world.sh}"
timeout_seconds="${ATINY_FIRST_MODEL_RESULT_TIMEOUT_SECONDS:-300}"
poll_seconds="${ATINY_FIRST_MODEL_RESULT_POLL_SECONDS:-2}"

if [[ ! "$world_id" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]]; then
  echo "WORLD_ID must be a UUID" >&2
  exit 2
fi
if [[ ! "$total_ticks" =~ ^[0-9]+$ ]] \
    || (( 10#$total_ticks < 2 || 10#$total_ticks > 1000000 )); then
  echo "TOTAL_TICKS must be an integer from 2 through 1000000" >&2
  exit 2
fi
if [[ ! "$timeout_seconds" =~ ^[1-9][0-9]*$ || ! "$poll_seconds" =~ ^[1-9][0-9]*$ ]]; then
  echo "model-result timeout and poll interval must be positive integers" >&2
  exit 2
fi
if [[ ! -x "$advance_executable" ]]; then
  echo "missing qualification advance executable: $advance_executable" >&2
  exit 2
fi
if ! command -v psql >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1; then
  echo "psql and python3 are required" >&2
  exit 2
fi

mapfile -d '' -t connection_fields < <(python3 <<'PY'
import os
import sys
from urllib.parse import parse_qs, unquote, urlsplit

url = urlsplit(os.environ["DATABASE_URL"])
if url.scheme not in {"postgres", "postgresql"}:
    sys.exit("DATABASE_URL must use postgres:// or postgresql://")
if not url.hostname or url.username is None or not url.path.startswith("/") or len(url.path) == 1:
    sys.exit("DATABASE_URL must include host, user, and database")
parameters = parse_qs(url.query, keep_blank_values=True, strict_parsing=True) if url.query else {}
if set(parameters) - {"sslmode"} or any(len(values) != 1 for values in parameters.values()):
    sys.exit("DATABASE_URL supports only one optional sslmode parameter")
fields = (
    url.hostname,
    str(url.port or 5432),
    unquote(url.username),
    unquote(url.password or ""),
    unquote(url.path[1:]),
    parameters.get("sslmode", [""])[0],
)
for field in fields:
    sys.stdout.buffer.write(field.encode("utf-8") + b"\0")
PY
)
if [[ "${#connection_fields[@]}" -ne 6 ]]; then
  echo "DATABASE_URL could not be converted to protected libpq settings" >&2
  exit 2
fi
libpq_environment=(
  "PGHOST=${connection_fields[0]}"
  "PGPORT=${connection_fields[1]}"
  "PGUSER=${connection_fields[2]}"
  "PGPASSWORD=${connection_fields[3]}"
  "PGDATABASE=${connection_fields[4]}"
)
if [[ -n "${connection_fields[5]}" ]]; then
  libpq_environment+=("PGSSLMODE=${connection_fields[5]}")
fi

cursor="$(env -u DATABASE_URL "${libpq_environment[@]}" psql -X -v ON_ERROR_STOP=1 -At \
  -v world_id="$world_id" <<'SQL'
SELECT current_tick || '|' || current_sequence
FROM worlds
WHERE id = :'world_id'::UUID;
SQL
)"
if [[ "$cursor" != "0|1" ]]; then
  echo "cognition-qualified advance requires an initialized tick-zero world at sequence 1; found ${cursor:-missing}" >&2
  exit 1
fi

DATABASE_URL="$DATABASE_URL" "$advance_executable" "$world_id" 1

started_at="$(date +%s)"
while true; do
  readiness="$(env -u DATABASE_URL "${libpq_environment[@]}" psql -X -v ON_ERROR_STOP=1 -At \
    -v world_id="$world_id" <<'SQL'
SELECT jsonb_build_object(
  'tick', world.current_tick,
  'requests', COUNT(request.request_id),
  'local_free_receipts', COUNT(result.request_id) FILTER (
    WHERE result.result_payload -> 'receipt' IS NOT NULL
      AND result.result_payload -> 'receipt' <> 'null'::JSONB
      AND result.result_payload -> 'receipt' ->> 'provider' = 'local_openai'
      AND (result.result_payload -> 'receipt' ->> 'billed_micro_usd')::BIGINT = 0
  )
)::TEXT
FROM worlds world
LEFT JOIN cognition_requests request ON request.world_id = world.id
LEFT JOIN cognition_results result ON result.request_id = request.request_id
WHERE world.id = :'world_id'::UUID
GROUP BY world.current_tick;
SQL
)"
  if [[ "$readiness" == *'"tick": 1'* && "$readiness" == *'"local_free_receipts": 1'* ]]; then
    break
  fi
  now="$(date +%s)"
  if (( now - started_at >= timeout_seconds )); then
    echo "first local cognition result was not prepared before wall timeout: ${readiness:-missing}" >&2
    exit 1
  fi
  sleep "$poll_seconds"
done

remaining_ticks=$((total_ticks - 1))
DATABASE_URL="$DATABASE_URL" "$advance_executable" "$world_id" "$remaining_ticks"
echo "advanced ${world_id} through ${total_ticks} ticks after a durable pre-deadline local model result"
