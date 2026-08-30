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
    inside && ($0 == "      - " network || $0 ~ "^      " network ":[[:space:]]*([^#]*)?$") { found = 1 }
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

for required in \
  'cancer-research-worker backend' \
  'cancer-research-worker research-egress' \
  'cancer-evidence-worker backend' \
  'cancer-evidence-worker research-egress'; do
  read -r service network <<<"$required"
  if ! service_has_network compose.hindsight.yaml "$service" "$network"; then
    echo "Production boundary violation: ${service} must join ${network}." >&2
    exit 1
  fi
done

if service_has_network compose.hindsight.yaml cancer-evidence-worker cognition-egress; then
  echo "Production boundary violation: the evidence worker must not join cognition-egress." >&2
  exit 1
fi

# GitHub-hosted runners can carry a Compose release that accepts long bind
# syntax but omits an explicit false `create_host_path` value from rendered
# JSON. Verify all four declarations in the authored evidence-worker block
# before accepting that renderer omission below.
evidence_worker_block="$(awk '
  $0 == "  cancer-evidence-worker:" { inside = 1 }
  inside && /^  [[:alnum:]_-]+:$/ && $0 != "  cancer-evidence-worker:" { exit }
  inside { print }
' compose.hindsight.yaml)"
if [[ "$(grep -c '^          create_host_path: false$' <<<"$evidence_worker_block")" -ne 4 ]]; then
  echo "Production boundary violation: every evidence bind must explicitly refuse host-path creation." >&2
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
tcga_name = "tcga-gbm-dr46-patient-baseline-v1.json"
tcga_hash = "f523989c2bec5ee14c0ff2c6dc30d193fb324e1dd234aba524bef179553294da"
tcga_container_path = f"/app/qualification/tcga-gbm/sha256/{tcga_hash}/{tcga_name}"
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
        if (volume.get("bind") or {}).get("create_host_path") is True:
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
        if (volume.get("bind") or {}).get("create_host_path") is True:
            failures.append(f"{expected_name} bind must refuse to create a missing host path")
        expected_source_suffix = (
            f"/runtime-qualification/pdc000711/sha256/"
            f"{hash_pair[0]}/{hash_pair[1]}/{expected_name}"
        )
        source = volume.get("source") or ""
        if not source.endswith(expected_source_suffix):
            failures.append(f"{expected_name} source is not the matching content-addressed path")

    if environment.get("CANCER_TCGA_GBM_TARGET_CONTEXT_PATH") != tcga_container_path:
        failures.append("evidence worker does not consume the pinned TCGA-GBM aggregate path")
    tcga_mounts = [
        volume
        for volume in (worker.get("volumes") or [])
        if volume.get("target") == tcga_container_path
    ]
    if len(tcga_mounts) != 1:
        failures.append("evidence worker must receive exactly one TCGA-GBM aggregate bind")
    else:
        volume = tcga_mounts[0]
        if volume.get("type") != "bind" or volume.get("read_only") is not True:
            failures.append("TCGA-GBM aggregate mount must be a read-only bind")
        if (volume.get("bind") or {}).get("create_host_path") is True:
            failures.append("TCGA-GBM aggregate bind must refuse to create a missing host path")
        expected_suffix = f"/runtime-qualification/tcga-gbm/sha256/{tcga_hash}/{tcga_name}"
        if not (volume.get("source") or "").endswith(expected_suffix):
            failures.append("TCGA-GBM aggregate source is not content-addressed")

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
        if (tcga_name in source or tcga_name in target) and name != worker_name:
            failures.append(f"{name} can mount TCGA-GBM held-out context")
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
        if (key == "CANCER_TCGA_GBM_TARGET_CONTEXT_PATH" or tcga_name in rendered) and name != worker_name:
            failures.append(f"{name} can address TCGA-GBM held-out context")

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
  'ExecStartPre=/usr/bin/env bash /home/shmuel/codex/emergent-civilization/scripts/stage-cancer-tcga-gbm-target-context.sh' \
  ops/systemd/atiny-cancer-evidence.service; then
  echo "Production boundary violation: the host evidence worker does not stage TCGA-GBM context." >&2
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
    | rg -v '^ops/systemd/atiny-cancer-tissue-refinement\.service:.*InaccessiblePaths=' \
    || true
)"
if [[ -n "$other_systemd_label_access" ]]; then
  printf '%s\n' "$other_systemd_label_access" >&2
  echo "Production boundary violation: another systemd service can address NCI qualification labels." >&2
  exit 1
fi
if ! rg -q --fixed-strings \
  'InaccessiblePaths=-/home/shmuel/codex/emergent-civilization/data/source-cache/nci-cellminer-2026-08-12/nci-cellminer-2-15-cns-challenge-answer-key-v1.json -/home/shmuel/codex/emergent-civilization/data/source-cache/aacr-gbm5k-dependency-v1 -/home/shmuel/codex/emergent-civilization/data/derived-cache/pdc000711-hcmi-gbm-proteome -/home/shmuel/codex/emergent-civilization/data/cancer-research/tcga-gbm-dr46-patient-baseline-v1.json -/run/atiny-cancer-evidence' \
  ops/systemd/atiny-cancer-research.service; then
  echo "Production boundary violation: the host research worker can see qualification inputs." >&2
  exit 1
fi
if ! rg -q --fixed-strings \
  'data/source-cache/aacr-gbm5k-dependency-v1' \
  ops/systemd/atiny-cancer-research.service; then
  echo "Production boundary violation: the host research worker can see the GBM5K answer key." >&2
  exit 1
fi
if ! rg -q --fixed-strings \
  'UnsetEnvironment=CANCER_PDC000711_PROTEOME_PATH CANCER_PDC000711_PROTEOME_METADATA_PATH CANCER_TCGA_GBM_TARGET_CONTEXT_PATH' \
  ops/systemd/atiny-cancer-research.service; then
  echo "Production boundary violation: the host research worker can inherit PDC000711 paths." >&2
  exit 1
fi

other_systemd_tcga_access="$(
  rg --line-number \
    'CANCER_TCGA_GBM_TARGET_CONTEXT_PATH|tcga-gbm-dr46-patient-baseline-v1.json' \
    ops/systemd --glob '*.service' \
    | rg -v '^ops/systemd/atiny-cancer-evidence\.service:' \
    | rg -v '^ops/systemd/atiny-cancer-research\.service:.*(InaccessiblePaths|UnsetEnvironment)=' \
    | rg -v '^ops/systemd/atiny-cancer-tissue-refinement\.service:.*(InaccessiblePaths|UnsetEnvironment)=' \
    || true
)"
if [[ -n "$other_systemd_tcga_access" ]]; then
  printf '%s\n' "$other_systemd_tcga_access" >&2
  echo "Production boundary violation: another systemd service can address TCGA-GBM context." >&2
  exit 1
fi

other_systemd_pdc_access="$(
  rg --line-number \
    'CANCER_PDC000711_PROTEOME_PATH|CANCER_PDC000711_PROTEOME_METADATA_PATH|pdc000711-gbm-proteome' \
    ops/systemd --glob '*.service' \
    | rg -v '^ops/systemd/atiny-cancer-evidence\.service:' \
    | rg -v '^ops/systemd/atiny-cancer-research\.service:.*(InaccessiblePaths|UnsetEnvironment)=' \
    | rg -v '^ops/systemd/atiny-cancer-tissue-refinement\.service:.*(InaccessiblePaths|UnsetEnvironment)=' \
    || true
)"
if [[ -n "$other_systemd_pdc_access" ]]; then
  printf '%s\n' "$other_systemd_pdc_access" >&2
  echo "Production boundary violation: another systemd service can address PDC000711 evidence." >&2
  exit 1
fi

tissue_unit=ops/systemd/atiny-cancer-tissue-refinement.service
for tissue_boundary in \
  'CPUQuota=100%' \
  'MemoryMax=1536M' \
  'MemorySwapMax=0' \
  'TasksMax=32' \
  'RuntimeMaxSec=30m' \
  'IPAddressAllow=localhost' \
  'IPAddressDeny=any' \
  'ExecStart=/home/shmuel/codex/emergent-civilization/target/release/civilization-runner cancer-tissue-refinement-worker' \
  'UnsetEnvironment=CANCER_OPENROUTER_API_KEY CANCER_FIREWORKS_API_KEY HETZNER_VLLM_API_KEY OPENROUTER_API_KEY HINDSIGHT_API_KEY' \
  'CLOUDFLARE_WORKERS_AI_API_KEY' \
  'GROQ_API_KEY' \
  'CEREBRAS_API_KEY' \
  'CANCER_CONSOLE_TOKEN' \
  'CLOUDFLARE_TUNNEL_TOKEN' \
  'R2_SECRET_ACCESS_KEY' \
  'WALG_LIBSODIUM_KEY' \
  'STRIPE_SECRET_KEY' \
  'STRIPE_WEBHOOK_SECRET' \
  'GOOGLE_OAUTH_CLIENT_SECRET' \
  'APPLE_PRIVATE_KEY'; do
  if ! rg -q --fixed-strings "$tissue_boundary" "$tissue_unit"; then
    echo "Production boundary violation: tissue worker is missing ${tissue_boundary}." >&2
    exit 1
  fi
done
if ! rg -q --fixed-strings \
  'command: ["/usr/bin/timeout", "--signal=TERM", "--kill-after=30s", "30m", "/app/civilization-runner", "cancer-tissue-refinement-worker"]' \
  compose.yaml; then
  echo "Production boundary violation: Compose tissue execution has no hard process lifetime cap." >&2
  exit 1
fi
if rg -n 'OpenAiCompatibleCognition|HindsightMemory|CancerResearchModelAdapters|dyn CancerResearchModel' \
  crates/application/src/research_tissue_worker.rs \
  crates/postgres-store/src/cancer_tissue_refinement.rs; then
  echo "Production boundary violation: tissue worker imports model or memory capabilities." >&2
  exit 1
fi

echo "Production network and cancer qualification boundary checks passed."
