#!/usr/bin/env bash
set -euo pipefail

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

temporary_file=$(mktemp "${env_file}.XXXXXX")
trap 'rm -f "$temporary_file"' EXIT
awk '
  !/^CANCER_WORLD_ID=/ &&
  !/^CANCER_RESEARCH_EXTERNAL_EXPORT_APPROVED=/ &&
  !/^CANCER_RESEARCH_PAID_ENABLED=/ &&
  !/^CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD=/
' "$env_file" > "$temporary_file"
{
  printf 'CANCER_WORLD_ID=%s\n' "$world_id"
  printf 'CANCER_RESEARCH_EXTERNAL_EXPORT_APPROVED=true\n'
  printf 'CANCER_RESEARCH_PAID_ENABLED=true\n'
  printf 'CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD=150000\n'
} >> "$temporary_file"

chown --reference="$env_file" "$temporary_file"
chmod --reference="$env_file" "$temporary_file"
mv "$temporary_file" "$env_file"
trap - EXIT

echo "Cancer World runtime configuration installed."
