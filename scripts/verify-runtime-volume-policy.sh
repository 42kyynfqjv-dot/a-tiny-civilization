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

ATINY_RUNTIME_ARTIFACT_ROOT="${project_root}/scripts" \
  docker compose -f compose.yaml -f compose.hindsight.yaml config --format json | \
  python3 -c '
import json
import os
import sys

document = json.load(sys.stdin)
mounts = document.get("services", {}).get("runner", {}).get("volumes", [])
expected = os.path.realpath(sys.argv[1])
matches = [
    mount for mount in mounts
    if mount.get("type") == "bind" and mount.get("target") == "/runtime"
]
if len(matches) != 1:
    raise SystemExit("runner must have exactly one /runtime bind mount")
mount = matches[0]
if os.path.realpath(mount.get("source", "")) != expected or not mount.get("read_only"):
    raise SystemExit("explicit runtime root did not reach the read-only runner mount")
' "${project_root}/scripts"

for start_path in Makefile scripts/deploy-production-app.sh; do
  if ! rg -q 'provision-runtime-volumes.sh' "$start_path"; then
    echo "$start_path does not provision protected volumes" >&2
    exit 1
  fi
done

echo "Canonical data volumes have stable external identities and provisioning paths."
