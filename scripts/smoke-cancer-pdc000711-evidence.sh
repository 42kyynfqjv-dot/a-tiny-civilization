#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_image="${ATINY_APP_IMAGE:-a-tiny-civilization-app:local}"
proteome_source="${CANCER_PDC000711_PROTEOME_SOURCE_PATH:-${project_root}/data/derived-cache/pdc000711-hcmi-gbm-proteome/pdc000711-gbm-proteome.tsv}"
metadata_source="${CANCER_PDC000711_PROTEOME_METADATA_SOURCE_PATH:-${project_root}/data/derived-cache/pdc000711-hcmi-gbm-proteome/pdc000711-gbm-proteome.metadata.json}"
runtime_root="${CANCER_PDC000711_EVIDENCE_RUNTIME_ROOT:-${project_root}/runtime-qualification/pdc000711}"

if ! docker image inspect "$app_image" >/dev/null 2>&1; then
  echo "PDC000711 container smoke requires an existing app image: $app_image" >&2
  exit 1
fi

mapfile -t staged_paths < <(
  bash "${project_root}/scripts/stage-cancer-pdc000711-evidence.sh" \
    --proteome "$proteome_source" \
    --metadata "$metadata_source" \
    --runtime-root "$runtime_root"
)
if ((${#staged_paths[@]} != 2)); then
  echo "PDC000711 staging helper did not emit exactly two paths" >&2
  exit 1
fi

proteome_sha256="$(sha256sum -- "${staged_paths[0]}" | awk '{print $1}')"
metadata_sha256="$(sha256sum -- "${staged_paths[1]}" | awk '{print $1}')"
container_root="/app/qualification/pdc000711/sha256/${proteome_sha256}/${metadata_sha256}"
proteome_container="${container_root}/pdc000711-gbm-proteome.tsv"
metadata_container="${container_root}/pdc000711-gbm-proteome.metadata.json"

mapfile -t container_digests < <(
  docker run --rm \
    --user 10001:10001 \
    --read-only \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --mount "type=bind,src=${staged_paths[0]},dst=${proteome_container},readonly" \
    --mount "type=bind,src=${staged_paths[1]},dst=${metadata_container},readonly" \
    "$app_image" sha256sum "$proteome_container" "$metadata_container" \
    | awk '{print $1}'
)

if [[ "${container_digests[0]:-}" != "$proteome_sha256" \
   || "${container_digests[1]:-}" != "$metadata_sha256" \
   || ${#container_digests[@]} -ne 2 ]]; then
  echo "uid 10001 could not read the exact PDC000711 evidence bytes" >&2
  exit 1
fi

echo "Content-addressed PDC000711 evidence is read-only and readable by container uid 10001."
