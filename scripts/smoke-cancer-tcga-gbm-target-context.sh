#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_image="${ATINY_APP_IMAGE:-a-tiny-civilization-app:local}"
source_path="${CANCER_TCGA_GBM_TARGET_CONTEXT_SOURCE_PATH:-${project_root}/data/cancer-research/tcga-gbm-dr46-patient-baseline-v1.json}"
runtime_root="${CANCER_TCGA_GBM_CONTEXT_RUNTIME_ROOT:-${project_root}/runtime-qualification/tcga-gbm}"
readonly expected_sha256='f523989c2bec5ee14c0ff2c6dc30d193fb324e1dd234aba524bef179553294da'
readonly container_path="/app/qualification/tcga-gbm/sha256/${expected_sha256}/tcga-gbm-dr46-patient-baseline-v1.json"

if ! docker image inspect "$app_image" >/dev/null 2>&1; then
  echo "TCGA-GBM context smoke requires an existing app image: $app_image" >&2
  exit 1
fi

mapfile -t staged < <(
  bash "${project_root}/scripts/stage-cancer-tcga-gbm-target-context.sh" \
    --source "$source_path" \
    --runtime-root "$runtime_root"
)
if ((${#staged[@]} != 1)); then
  echo "TCGA-GBM context staging did not emit exactly one path" >&2
  exit 1
fi

observed="$(docker run --rm \
  --user 10001:10001 \
  --read-only \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --mount "type=bind,src=${staged[0]},dst=${container_path},readonly" \
  "$app_image" sha256sum "$container_path" | awk '{print $1}')"
if [[ "$observed" != "$expected_sha256" ]]; then
  echo "uid 10001 could not read the exact TCGA-GBM aggregate bytes" >&2
  exit 1
fi

echo "Content-addressed TCGA-GBM context is read-only and readable by container uid 10001."
