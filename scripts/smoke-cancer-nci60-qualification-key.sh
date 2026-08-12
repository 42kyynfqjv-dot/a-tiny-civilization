#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly expected_sha256='559d52f45f18901d3ce8fb844f99cd88045ccd3fbd0c99cb7e8139b85e59f4ce'
readonly container_path="/app/qualification/nci-cellminer-2-15-cns-challenge-answer-key-v1-${expected_sha256}.json"
app_image="${ATINY_APP_IMAGE:-a-tiny-civilization-app:local}"
source_path="${CANCER_NCI60_ANSWER_KEY_SOURCE_PATH:-${project_root}/data/source-cache/nci-cellminer-2026-08-12/nci-cellminer-2-15-cns-challenge-answer-key-v1.json}"
runtime_root="${CANCER_NCI60_QUALIFICATION_RUNTIME_ROOT:-${project_root}/runtime-qualification/nci60}"

if ! docker image inspect "$app_image" >/dev/null 2>&1; then
  echo "NCI-60 container smoke requires an existing app image: $app_image" >&2
  exit 1
fi

staged_path="$(
  bash "${project_root}/scripts/stage-cancer-nci60-qualification-key.sh" \
    --source "$source_path" \
    --runtime-root "$runtime_root"
)"
if [[ ! -f "$staged_path" || -L "$staged_path" ]]; then
  echo "NCI-60 staging helper did not emit a regular answer-key path" >&2
  exit 1
fi

# Run with the exact uid/gid selected by the runtime image. This catches host-mode
# regressions that Compose parsing alone cannot see.
container_digest="$(
  docker run --rm \
    --user 10001:10001 \
    --read-only \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --mount "type=bind,src=${staged_path},dst=${container_path},readonly" \
    "$app_image" sha256sum "$container_path" | awk '{print $1}'
)"
if [[ "$container_digest" != "$expected_sha256" ]]; then
  echo "uid 10001 could not read the pinned NCI-60 answer key in the runtime image" >&2
  exit 1
fi

echo "Pinned NCI-60 qualification key is read-only and readable by container uid 10001."
