#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

docker compose -f compose.yaml -f compose.hindsight.yaml config --format json | python3 -c '
import json
import sys

document = json.load(sys.stdin)
if document.get("name") != "a-tiny-civilization":
    raise SystemExit("Compose project identity is not stable")
expected = {
    "postgres-data": "a-tiny-civilization-postgres-v1",
    "hindsight-data": "a-tiny-civilization-hindsight-v1",
    "hindsight-model-cache": "a-tiny-civilization-hindsight-model-cache-v1",
}
volumes = document.get("volumes", {})
failures = []
for logical_name, physical_name in expected.items():
    volume = volumes.get(logical_name, {})
    if volume.get("name") != physical_name or volume.get("external") is not True:
        failures.append(f"{logical_name} is not protected as external {physical_name}")
if failures:
    raise SystemExit("invalid durable-volume policy:\n  " + "\n  ".join(failures))
'

for start_path in Makefile scripts/deploy-production-app.sh; do
  if ! rg -q 'provision-runtime-volumes.sh' "$start_path"; then
    echo "$start_path does not provision protected volumes" >&2
    exit 1
  fi
done

echo "Canonical data volumes have stable external identities and provisioning paths."
