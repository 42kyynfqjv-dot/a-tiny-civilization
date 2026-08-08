#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

docker compose -f compose.yaml -f compose.hindsight.yaml config --format json | python3 -c '
import json
import sys

document = json.load(sys.stdin)
services = document.get("services", {})
first_party = ("api", "cognition-worker", "memory-worker", "migrate", "projector", "runner", "web")
failures = []
for name in first_party:
    service = services.get(name, {})
    if service.get("read_only") is not True:
        failures.append(f"{name} root filesystem is writable")
    if service.get("cap_drop") != ["ALL"]:
        failures.append(f"{name} does not drop every Linux capability")
    if "no-new-privileges:true" not in (service.get("security_opt") or []):
        failures.append(f"{name} permits privilege escalation")
    if not any(entry.startswith("/tmp:") for entry in (service.get("tmpfs") or [])):
        failures.append(f"{name} has no bounded ephemeral /tmp")
if failures:
    raise SystemExit("invalid first-party container privilege policy:\n  " + "\n  ".join(failures))
'

if ! rg -q '^USER civilization$' Dockerfile; then
  echo "Rust runtime image does not select its unprivileged account" >&2
  exit 1
fi
if ! rg -q '^USER node$' web/Dockerfile; then
  echo "web runtime image does not select its unprivileged account" >&2
  exit 1
fi

echo "First-party containers are non-root, read-only, and capability-free."
