#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

verify_profile() {
  local label="$1"
  shift
  docker compose "$@" config --format json | python3 -c '
import json
import sys

label = sys.argv[1]
document = json.load(sys.stdin)
services = document.get("services", {})
if not services:
    raise SystemExit(f"{label}: composed project has no services")

failures = []
for name, service in sorted(services.items()):
    logging = service.get("logging") or {}
    options = logging.get("options") or {}
    driver = logging.get("driver")
    maximum_size = options.get("max-size")
    maximum_files = options.get("max-file")
    if driver != "json-file":
        failures.append(f"{name} has logging driver {driver!r}")
    if maximum_size != "10m":
        failures.append(f"{name} has max-size {maximum_size!r}")
    if str(maximum_files) != "5":
        failures.append(f"{name} has max-file {maximum_files!r}")

if failures:
    raise SystemExit(label + ": unbounded container logging:\n  " + "\n  ".join(failures))
print(f"{label}: {len(services)} services have bounded local logs")
' "$label"
}

verify_profile base -f compose.yaml
verify_profile hindsight -f compose.yaml -f compose.hindsight.yaml
verify_profile backup -f compose.yaml -f compose.backup.yaml
verify_profile tunnel -f compose.yaml -f compose.tunnel.yaml
RESTORE_WORLD_ID=00000000-0000-0000-0000-000000000001 \
  verify_profile restore -f compose.restore.yaml

echo "Every Compose service has a bounded local log policy."
