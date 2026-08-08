#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

docker compose -f compose.yaml config --format json | python3 -c '
import json
import sys

document = json.load(sys.stdin)
database = document.get("services", {}).get("db", {})
environment = database.get("environment", {})
if environment.get("POSTGRES_INITDB_ARGS") != "--data-checksums":
    raise SystemExit("fresh PostgreSQL clusters do not require page checksums")
'

for setting in data_checksums fsync synchronous_commit full_page_writes; do
  if ! rg -q "current_setting\('${setting}'\)" scripts/backend-status.sh; then
    echo "backend status does not enforce PostgreSQL setting: $setting" >&2
    exit 1
  fi
done

echo "Fresh PostgreSQL clusters and live durability settings fail closed."
