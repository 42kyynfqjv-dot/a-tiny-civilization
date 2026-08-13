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

# The held-out NCI labels and patient-derived PDC matrix are runtime
# qualification input, not research context. Resolve the optional profile as
# Compose will run it and prove that only read-only, non-auto-created binds reach
# the observer-side evidence worker.
docker compose --profile container-research \
  -f compose.yaml -f compose.hindsight.yaml config --format json | python3 -c '
import json
import re
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
pdc_proteome_name = "pdc000711-gbm-proteome.tsv"
pdc_metadata_name = "pdc000711-gbm-proteome.metadata.json"
pdc_container_pattern = re.compile(
    r"^/app/qualification/pdc000711/sha256/"
    r"([0-9a-f]{64})/([0-9a-f]{64})/"
    r"(pdc000711-gbm-proteome(?:\.metadata\.json|\.tsv))$"
)
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

    pdc_paths = {
        "CANCER_PDC000711_PROTEOME_PATH": pdc_proteome_name,
        "CANCER_PDC000711_PROTEOME_METADATA_PATH": pdc_metadata_name,
    }
    observed_hash_pair = None
    for key, expected_name in pdc_paths.items():
        container_value = environment.get(key)
        match = pdc_container_pattern.fullmatch(container_value or "")
        if match is None or match.group(3) != expected_name:
            failures.append(f"evidence worker {key} is not content-addressed")
            continue
        hash_pair = match.group(1), match.group(2)
        if observed_hash_pair is None:
            observed_hash_pair = hash_pair
        elif observed_hash_pair != hash_pair:
            failures.append("PDC000711 matrix and metadata paths do not share one hash pair")
        pdc_mounts = [
            volume
            for volume in (worker.get("volumes") or [])
            if volume.get("target") == container_value
        ]
        if len(pdc_mounts) != 1:
            failures.append(f"evidence worker must receive exactly one {expected_name} bind")
            continue
        volume = pdc_mounts[0]
        if volume.get("type") != "bind" or volume.get("read_only") is not True:
            failures.append(f"{expected_name} mount must be a read-only bind")
        if (volume.get("bind") or {}).get("create_host_path") is not False:
            failures.append(f"{expected_name} bind must refuse to create a missing host path")
        expected_source_suffix = (
            f"/runtime-qualification/pdc000711/sha256/"
            f"{hash_pair[0]}/{hash_pair[1]}/{expected_name}"
        )
        source = volume.get("source") or ""
        if not source.endswith(expected_source_suffix):
            failures.append(f"{expected_name} source is not the matching content-addressed path")

for name, service in services.items():
    for volume in service.get("volumes") or []:
        source = volume.get("source") or ""
        target = volume.get("target") or ""
        if (answer_name in source or "nci-cellminer-2-15-cns-challenge-answer-key" in target) and name != worker_name:
            failures.append(f"{name} can mount NCI qualification labels")
        if (
            pdc_proteome_name in source
            or pdc_metadata_name in source
            or pdc_proteome_name in target
            or pdc_metadata_name in target
        ) and name != worker_name:
            failures.append(f"{name} can mount PDC000711 patient-derived evidence")
    for key, value in (service.get("environment") or {}).items():
        rendered = "" if value is None else str(value)
        if (key == "CANCER_NCI60_ANSWER_KEY_PATH" or answer_name in rendered) and name != worker_name:
            failures.append(f"{name} can address NCI qualification labels")
        if (
            key in (
                "CANCER_PDC000711_PROTEOME_PATH",
                "CANCER_PDC000711_PROTEOME_METADATA_PATH",
            )
            or pdc_proteome_name in rendered
            or pdc_metadata_name in rendered
        ) and name != worker_name:
            failures.append(f"{name} can address PDC000711 patient-derived evidence")

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
if ! rg -q --fixed-strings \
  'ExecStartPre=/usr/bin/env bash /home/shmuel/codex/emergent-civilization/scripts/stage-cancer-pdc000711-evidence.sh' \
  ops/systemd/atiny-cancer-evidence.service; then
  echo "Production boundary violation: the host evidence worker does not stage PDC000711 evidence." >&2
  exit 1
fi
if ! rg -q --fixed-strings \
  'ExecStart=/usr/bin/env bash /home/shmuel/codex/emergent-civilization/scripts/run-cancer-evidence-with-pdc000711.sh' \
  ops/systemd/atiny-cancer-evidence.service; then
  echo "Production boundary violation: the host evidence worker does not resolve exact PDC000711 paths at exec." >&2
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
  'InaccessiblePaths=-/home/shmuel/codex/emergent-civilization/data/source-cache/nci-cellminer-2026-08-12/nci-cellminer-2-15-cns-challenge-answer-key-v1.json -/home/shmuel/codex/emergent-civilization/data/derived-cache/pdc000711-hcmi-gbm-proteome -/run/atiny-cancer-evidence' \
  ops/systemd/atiny-cancer-research.service; then
  echo "Production boundary violation: the host research worker can see qualification inputs." >&2
  exit 1
fi
if ! rg -q --fixed-strings \
  'UnsetEnvironment=CANCER_PDC000711_PROTEOME_PATH CANCER_PDC000711_PROTEOME_METADATA_PATH' \
  ops/systemd/atiny-cancer-research.service; then
  echo "Production boundary violation: the host research worker can inherit PDC000711 paths." >&2
  exit 1
fi

other_systemd_pdc_access="$(
  rg --line-number \
    'CANCER_PDC000711_PROTEOME_PATH|CANCER_PDC000711_PROTEOME_METADATA_PATH|pdc000711-gbm-proteome' \
    ops/systemd --glob '*.service' \
    | rg -v '^ops/systemd/atiny-cancer-evidence\.service:' \
    | rg -v '^ops/systemd/atiny-cancer-research\.service:.*(InaccessiblePaths|UnsetEnvironment)=' \
    || true
)"
if [[ -n "$other_systemd_pdc_access" ]]; then
  printf '%s\n' "$other_systemd_pdc_access" >&2
  echo "Production boundary violation: another systemd service can address PDC000711 evidence." >&2
  exit 1
fi

echo "Production network and cancer qualification boundary checks passed."
