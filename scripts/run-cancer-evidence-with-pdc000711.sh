#!/usr/bin/env bash
set -euo pipefail

# Resolve and re-verify the root-staged, content-addressed PDC inputs immediately
# before exec. The variables exist only in the evidence worker's process tree.

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
proteome_source="${project_root}/data/derived-cache/pdc000711-hcmi-gbm-proteome/pdc000711-gbm-proteome.tsv"
metadata_source="${project_root}/data/derived-cache/pdc000711-hcmi-gbm-proteome/pdc000711-gbm-proteome.metadata.json"
runtime_root="${CANCER_PDC000711_EVIDENCE_RUNTIME_ROOT:-/run/atiny-cancer-evidence/pdc000711}"
runner_path="${CANCER_EVIDENCE_RUNNER_PATH:-${project_root}/target/release/civilization-runner}"

if [[ ! -x "$runner_path" || -L "$runner_path" ]]; then
  echo "Cancer evidence runner must be an executable, non-symlink file: $runner_path" >&2
  exit 1
fi

mapfile -t staged_paths < <(
  bash "${project_root}/scripts/stage-cancer-pdc000711-evidence.sh" \
    --proteome "$proteome_source" \
    --metadata "$metadata_source" \
    --runtime-root "$runtime_root" \
    --verify-only
)
if ((${#staged_paths[@]} != 2)); then
  echo "PDC000711 evidence staging did not resolve exactly two paths" >&2
  exit 1
fi

export CANCER_PDC000711_PROTEOME_PATH="${staged_paths[0]}"
export CANCER_PDC000711_PROTEOME_METADATA_PATH="${staged_paths[1]}"
exec "$runner_path" cancer-evidence-worker
