#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose_file="${project_root}/compose.hindsight.yaml"

grep -qF 'image: ghcr.io/vectorize-io/hindsight:0.8.6@sha256:ffa391a77284e49f6b55e32c86f33529ac4257831407b14038a72b6a0a232039' "$compose_file"
grep -qE '^[[:space:]]+shm_size:[[:space:]]+1gb$' "$compose_file"
grep -qF 'HINDSIGHT_API_STARTUP_WAIT_SECONDS: "900"' "$compose_file"
grep -qF 'HINDSIGHT_API_MODEL_INIT_TIMEOUT: "900"' "$compose_file"
grep -qE '^[[:space:]]+stop_grace_period:[[:space:]]+30s$' "$compose_file"
grep -qF 'HINDSIGHT_API_LLM_PROVIDER: none' "$compose_file"
grep -qF 'HINDSIGHT_API_STORE_DOCUMENT_TEXT: "false"' "$compose_file"
grep -qF 'HF_HUB_OFFLINE: ${HINDSIGHT_HF_OFFLINE:-1}' "$compose_file"
grep -qF 'TRANSFORMERS_OFFLINE: ${HINDSIGHT_HF_OFFLINE:-1}' "$compose_file"
grep -qF 'COGNITION_CLAIM_LEASE_SECONDS: ${COGNITION_CLAIM_LEASE_SECONDS:-3600}' "$compose_file"
grep -qF 'COGNITION_REQUEST_TIMEOUT_SECONDS: ${COGNITION_REQUEST_TIMEOUT_SECONDS:-180}' "$compose_file"

echo "Hindsight and local cognition are pinned, keyless, text-minimized, and CPU-timeout provisioned."
