#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose_file="${project_root}/compose.hindsight.yaml"

grep -qF 'image: ghcr.io/vectorize-io/hindsight:0.8.6' "$compose_file"
grep -qE '^[[:space:]]+shm_size:[[:space:]]+1gb$' "$compose_file"
grep -qF 'HINDSIGHT_API_STARTUP_WAIT_SECONDS: "900"' "$compose_file"
grep -qF 'HINDSIGHT_API_MODEL_INIT_TIMEOUT: "900"' "$compose_file"
grep -qE '^[[:space:]]+stop_grace_period:[[:space:]]+30s$' "$compose_file"
grep -qF 'HINDSIGHT_API_LLM_PROVIDER: none' "$compose_file"
grep -qF 'HINDSIGHT_API_STORE_DOCUMENT_TEXT: "false"' "$compose_file"

echo "Hindsight runtime is pinned, keyless, text-minimized, and provisioned with sufficient shared memory."
