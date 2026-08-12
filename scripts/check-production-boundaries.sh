#!/usr/bin/env bash
set -euo pipefail

# Keep the public tunnel's network reachability structurally narrow. This check uses
# the authored Compose files rather than a running stack, so CI catches a boundary
# regression before any deployment credentials or containers exist.

service_has_network() {
  local file="$1"
  local service="$2"
  local network="$3"

  awk -v service="$service" -v network="$network" '
    $0 == "  " service ":" { inside = 1; next }
    inside && /^  [[:alnum:]_-]+:$/ { inside = 0 }
    inside && $0 == "      - " network { found = 1 }
    END { exit(found ? 0 : 1) }
  ' "$file"
}

if service_has_network compose.yaml api edge; then
  echo "Production boundary violation: the observer API must not join edge." >&2
  exit 1
fi

for required in 'api backend' 'api web-api' 'web edge' 'web web-api'; do
  read -r service network <<<"$required"
  if ! service_has_network compose.yaml "$service" "$network"; then
    echo "Production boundary violation: ${service} must join ${network}." >&2
    exit 1
  fi
done

if ! service_has_network compose.tunnel.yaml cloudflared edge; then
  echo "Production boundary violation: cloudflared must join edge." >&2
  exit 1
fi

if rg --line-number --context 8 '^  cloudflared:$' compose.tunnel.yaml | rg --line-number '^      - (backend|web-api)$'; then
  echo "Production boundary violation: cloudflared must not join backend or web-api." >&2
  exit 1
fi

if ! rg --multiline --context 4 '^  web-api:$' compose.yaml | rg --line-number '^    internal: true$' >/dev/null; then
  echo "Production boundary violation: web-api must remain an internal network." >&2
  exit 1
fi

# The held-out NCI labels are runtime qualification input, not research context.
# Resolve the optional profile as Compose will run it and prove that exactly one
# read-only, non-auto-created bind reaches the observer-side evidence worker.
docker compose --profile container-research \
  -f compose.yaml -f compose.hindsight.yaml config --format json | python3 -c '
import json
import sys

document = json.load(sys.stdin)
services = document.get("services", {})
worker_name = "cancer-evidence-worker"
answer_hash = "559d52f45f18901d3ce8fb844f99cd88045ccd3fbd0c99cb7e8139b85e59f4ce"
answer_name = "nci-cellminer-2-15-cns-challenge-answer-key-v1.json"
container_path = (
    "/app/qualification/nci-cellminer-2-15-cns-challenge-answer-key-v1-"
    + answer_hash
    + ".json"
)
source_suffix = f"/runtime-qualification/nci60/{answer_hash}/{answer_name}"
failures = []

worker = services.get(worker_name)
if not isinstance(worker, dict):
    failures.append("container-research profile has no cancer evidence worker")
else:
    environment = worker.get("environment") or {}
    if environment.get("CANCER_NCI60_ANSWER_KEY_PATH") != container_path:
        failures.append("evidence worker does not consume the pinned container path")
    matching = [
        volume
        for volume in (worker.get("volumes") or [])
        if volume.get("target") == container_path
    ]
    if len(matching) != 1:
        failures.append("evidence worker must receive exactly one answer-key bind")
    else:
        volume = matching[0]
        if volume.get("type") != "bind" or volume.get("read_only") is not True:
            failures.append("answer-key mount must be a read-only bind")
        if (volume.get("bind") or {}).get("create_host_path") is not False:
            failures.append("answer-key bind must refuse to create a missing host path")
        source = volume.get("source") or ""
        if not source.endswith(source_suffix):
            failures.append("answer-key bind source is not the pinned content-addressed path")

for name, service in services.items():
    for volume in service.get("volumes") or []:
        source = volume.get("source") or ""
        target = volume.get("target") or ""
        if (answer_name in source or "nci-cellminer-2-15-cns-challenge-answer-key" in target) and name != worker_name:
            failures.append(f"{name} can mount NCI qualification labels")
    for key, value in (service.get("environment") or {}).items():
        rendered = "" if value is None else str(value)
        if (key == "CANCER_NCI60_ANSWER_KEY_PATH" or answer_name in rendered) and name != worker_name:
            failures.append(f"{name} can address NCI qualification labels")

if failures:
    raise SystemExit(
        "NCI-60 qualification boundary violation:\n  " + "\n  ".join(failures)
    )
'

if ! rg -q --fixed-strings \
  'COPY --from=builder --chown=10001:10001 --chmod=0444 /source/data/cancer-research/nci-cellminer-2-15-cns-challenge-catalogue-v1.json' \
  Dockerfile; then
  echo "Production boundary violation: the public NCI catalogue is not pinned readable in the runtime image." >&2
  exit 1
fi
if ! rg -q --fixed-strings \
  'install -d -o civilization -g civilization -m 0555 /app/data /app/data/cancer-research' \
  Dockerfile; then
  echo "Production boundary violation: uid 10001 cannot traverse to the NCI catalogue." >&2
  exit 1
fi
if rg -n '^COPY .*nci-cellminer-2-15-cns-challenge-answer-key' Dockerfile; then
  echo "Production boundary violation: the NCI answer key must never be baked into an image." >&2
  exit 1
fi
for excluded_context in data/source-cache runtime-qualification; do
  if ! grep -Fxq "$excluded_context" .dockerignore; then
    echo "Production boundary violation: ${excluded_context} can enter the Docker build context." >&2
    exit 1
  fi
done
if ! rg -q --fixed-strings \
  'ExecStartPre=/usr/bin/env bash /home/shmuel/codex/emergent-civilization/scripts/stage-cancer-nci60-qualification-key.sh' \
  ops/systemd/atiny-cancer-evidence.service; then
  echo "Production boundary violation: the host evidence worker does not stage pinned NCI labels." >&2
  exit 1
fi
other_systemd_label_access="$(
  rg --line-number \
    'CANCER_NCI60_ANSWER_KEY_PATH|nci-cellminer-2-15-cns-challenge-answer-key' \
    ops/systemd --glob '*.service' \
    | rg -v '^ops/systemd/atiny-cancer-evidence\.service:' \
    | rg -v '^ops/systemd/atiny-cancer-research\.service:.*InaccessiblePaths=' \
    || true
)"
if [[ -n "$other_systemd_label_access" ]]; then
  printf '%s\n' "$other_systemd_label_access" >&2
  echo "Production boundary violation: another systemd service can address NCI qualification labels." >&2
  exit 1
fi
if ! rg -q --fixed-strings \
  'InaccessiblePaths=-/home/shmuel/codex/emergent-civilization/data/source-cache/nci-cellminer-2026-08-12/nci-cellminer-2-15-cns-challenge-answer-key-v1.json -/run/atiny-cancer-evidence' \
  ops/systemd/atiny-cancer-research.service; then
  echo "Production boundary violation: the host research worker can see NCI qualification labels." >&2
  exit 1
fi

echo "Production network and NCI-60 qualification boundary checks passed."
