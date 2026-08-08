#!/usr/bin/env bash
set -euo pipefail

if (($# != 3)); then
  echo "usage: $0 PROJECT_ROOT ENVIRONMENT_FILE OUTPUT_FILE" >&2
  exit 2
fi
project_root="$1"
environment_file="$2"
output_file="$3"

for path in "$project_root" "$environment_file" "$output_file"; do
  if [[ ! "$path" =~ ^/[A-Za-z0-9._/-]+$ ]]; then
    echo "monitor paths must be absolute and contain only portable path characters" >&2
    exit 2
  fi
done
if [[ ! -d "$project_root" || ! -f "${project_root}/scripts/backend-status.sh" ]]; then
  echo "monitor project root does not contain backend-status.sh" >&2
  exit 2
fi
if [[ ! -d "$(dirname "$output_file")" || -L "$output_file" ]]; then
  echo "monitor override destination is absent or unsafe" >&2
  exit 2
fi

umask 022
printf '%s\n' \
  '[Service]' \
  "WorkingDirectory=${project_root}" \
  'EnvironmentFile=' \
  "EnvironmentFile=${environment_file}" \
  'ExecStart=' \
  "ExecStart=/usr/bin/env bash ${project_root}/scripts/backend-status.sh --env-file ${environment_file}" \
  'ProtectHome=read-only' \
  >"$output_file"
