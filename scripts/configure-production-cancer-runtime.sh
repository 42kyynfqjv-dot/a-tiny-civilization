#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ ${1:-} != "--env-file" || -z ${2:-} || ${3:-} != "--world-id" || -z ${4:-} ]]; then
  echo "usage: $0 --env-file PATH --world-id UUID" >&2
  exit 64
fi

env_file=$2
world_id=$4
if [[ ! -f $env_file ]]; then
  echo "environment file does not exist" >&2
  exit 66
fi
if [[ ! $world_id =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]]; then
  echo "invalid Cancer World UUID" >&2
  exit 65
fi

nci60_sha=559d52f45f18901d3ce8fb844f99cd88045ccd3fbd0c99cb7e8139b85e59f4ce
pdc_proteome_sha=469f82d518f7b351f002ff671ec139ae97a8e389e0f296a644d40872935ebeda
pdc_metadata_sha=a61f29a50b687b43416784aae41f1f0802427da17972caabc2b15d8d3f38d7c2
tcga_sha=f523989c2bec5ee14c0ff2c6dc30d193fb324e1dd234aba524bef179553294da
nci60_path="${project_root}/runtime-qualification/nci60/${nci60_sha}/nci-cellminer-2-15-cns-challenge-answer-key-v1.json"
pdc_directory="${project_root}/runtime-qualification/pdc000711/sha256/${pdc_proteome_sha}/${pdc_metadata_sha}"
pdc_proteome_path="${pdc_directory}/pdc000711-gbm-proteome.tsv"
pdc_metadata_path="${pdc_directory}/pdc000711-gbm-proteome.metadata.json"
tcga_path="${project_root}/runtime-qualification/tcga-gbm/sha256/${tcga_sha}/tcga-gbm-dr46-patient-baseline-v1.json"

verify_pinned_file() {
  local expected_sha=$1
  local path=$2
  if [[ ! -f $path || -L $path ]]; then
    echo "pinned Cancer evidence file is absent or unsafe: $path" >&2
    exit 66
  fi
  local actual_sha
  actual_sha="$(sha256sum -- "$path")"
  actual_sha=${actual_sha%% *}
  if [[ $actual_sha != "$expected_sha" ]]; then
    echo "pinned Cancer evidence checksum differs: $path" >&2
    exit 65
  fi
}

verify_pinned_file "$nci60_sha" "$nci60_path"
verify_pinned_file "$pdc_proteome_sha" "$pdc_proteome_path"
verify_pinned_file "$pdc_metadata_sha" "$pdc_metadata_path"
verify_pinned_file "$tcga_sha" "$tcga_path"

temporary_file=$(mktemp "${env_file}.XXXXXX")
trap 'rm -f "$temporary_file"' EXIT
awk '
  !/^CANCER_WORLD_ID=/ &&
  !/^CANCER_RESEARCH_EXTERNAL_EXPORT_APPROVED=/ &&
  !/^CANCER_RESEARCH_PAID_ENABLED=/ &&
  !/^CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD=/ &&
  !/^CANCER_NCI60_ANSWER_KEY_HOST_PATH=/ &&
  !/^CANCER_PDC000711_PROTEOME_HOST_PATH=/ &&
  !/^CANCER_PDC000711_PROTEOME_METADATA_HOST_PATH=/ &&
  !/^CANCER_PDC000711_PROTEOME_SHA256=/ &&
  !/^CANCER_PDC000711_PROTEOME_METADATA_SHA256=/ &&
  !/^CANCER_TCGA_GBM_TARGET_CONTEXT_HOST_PATH=/
' "$env_file" > "$temporary_file"
{
  printf 'CANCER_WORLD_ID=%s\n' "$world_id"
  printf 'CANCER_RESEARCH_EXTERNAL_EXPORT_APPROVED=true\n'
  printf 'CANCER_RESEARCH_PAID_ENABLED=true\n'
  printf 'CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD=250000\n'
  printf 'CANCER_NCI60_ANSWER_KEY_HOST_PATH=%s\n' "$nci60_path"
  printf 'CANCER_PDC000711_PROTEOME_HOST_PATH=%s\n' "$pdc_proteome_path"
  printf 'CANCER_PDC000711_PROTEOME_METADATA_HOST_PATH=%s\n' "$pdc_metadata_path"
  printf 'CANCER_PDC000711_PROTEOME_SHA256=%s\n' "$pdc_proteome_sha"
  printf 'CANCER_PDC000711_PROTEOME_METADATA_SHA256=%s\n' "$pdc_metadata_sha"
  printf 'CANCER_TCGA_GBM_TARGET_CONTEXT_HOST_PATH=%s\n' "$tcga_path"
} >> "$temporary_file"

chown --reference="$env_file" "$temporary_file"
chmod --reference="$env_file" "$temporary_file"
mv "$temporary_file" "$env_file"
trap - EXIT

echo "Cancer World runtime configuration installed."
