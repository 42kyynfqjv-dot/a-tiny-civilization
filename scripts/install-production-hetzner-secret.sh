#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} != "--env-file" || -z ${2:-} || ${3:-} != "--confirm-production-hetzner-secret" ]]; then
  echo "usage: $0 --env-file PATH --confirm-production-hetzner-secret" >&2
  exit 64
fi

env_file=$2
if [[ ! -f $env_file || -L $env_file ]]; then
  echo "environment file must be an existing regular non-symlink file" >&2
  exit 66
fi

IFS= read -r -s hetzner_key
if [[ ! $hetzner_key =~ ^[A-Za-z0-9_-]{24,128}$ ]]; then
  echo "invalid Hetzner Inference token" >&2
  exit 65
fi

temporary_file=$(mktemp "${env_file}.XXXXXX")
trap 'rm -f "$temporary_file"' EXIT

awk '!/^HETZNER_VLLM_API_KEY=/' "$env_file" > "$temporary_file"
printf 'HETZNER_VLLM_API_KEY=%s\n' "$hetzner_key" >> "$temporary_file"

chown --reference="$env_file" "$temporary_file"
chmod --reference="$env_file" "$temporary_file"
mv "$temporary_file" "$env_file"
trap - EXIT

echo "Hetzner Inference token installed."
