#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} != "--env-file" || -z ${2:-} || ${3:-} != "--confirm-production-cancer-secrets" ]]; then
  echo "usage: $0 --env-file PATH --confirm-production-cancer-secrets" >&2
  exit 64
fi

env_file=$2
if [[ ! -f $env_file ]]; then
  echo "environment file does not exist" >&2
  exit 66
fi

IFS= read -r -s cancer_key
if [[ $cancer_key != sk-or-v1-* || $cancer_key == *[[:space:]]* ]]; then
  echo "invalid Cancer World OpenRouter key" >&2
  exit 65
fi

existing_token=$(sed -n 's/^CANCER_CONSOLE_TOKEN=//p' "$env_file" | tail -n 1)
console_token=${existing_token:-$(openssl rand -hex 24)}
existing_world=$(sed -n 's/^CANCER_WORLD_ID=//p' "$env_file" | tail -n 1)
temporary_file=$(mktemp "${env_file}.XXXXXX")
trap 'rm -f "$temporary_file"' EXIT

awk '!/^CANCER_OPENROUTER_API_KEY=/ && !/^CANCER_CONSOLE_TOKEN=/ && !/^CANCER_WORLD_ID=/' "$env_file" > "$temporary_file"
{
  printf 'CANCER_OPENROUTER_API_KEY=%s\n' "$cancer_key"
  printf 'CANCER_CONSOLE_TOKEN=%s\n' "$console_token"
  printf 'CANCER_WORLD_ID=%s\n' "$existing_world"
} >> "$temporary_file"

chown --reference="$env_file" "$temporary_file"
chmod --reference="$env_file" "$temporary_file"
mv "$temporary_file" "$env_file"
trap - EXIT

printf 'CANCER_CONSOLE_URL=https://atinycivilization.com/research/%s\n' "$console_token"
