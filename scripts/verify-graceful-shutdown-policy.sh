#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

docker compose -f compose.yaml -f compose.hindsight.yaml config --format json | python3 -c '
import json
import sys

document = json.load(sys.stdin)
services = document.get("services", {})
minimums = {
    "api": "1m0s",
    "cognition-worker": "1m0s",
    "db": "1m0s",
    "memory-worker": "1m0s",
    "projector": "1m0s",
    "runner": "1m0s",
}
failures = []
for name, expected in minimums.items():
    observed = services.get(name, {}).get("stop_grace_period")
    if observed != expected:
        failures.append(f"{name}: expected {expected}, found {observed!r}")
if failures:
    raise SystemExit("invalid shutdown grace policy:\n  " + "\n  ".join(failures))
'

for application in api projector runner; do
  if ! rg -q 'SignalKind::terminate\(\)' "apps/${application}/src/main.rs"; then
    echo "$application does not handle the SIGTERM sent by Docker stop" >&2
    exit 1
  fi
done

echo "Canonical services handle SIGTERM within an explicit shutdown grace period."
