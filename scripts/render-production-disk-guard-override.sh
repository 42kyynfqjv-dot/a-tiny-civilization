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
  [[ "$path" =~ ^/[A-Za-z0-9._/-]+$ ]] || {
    echo "disk guard paths must be absolute and contain only portable path characters" >&2
    exit 2
  }
done
[[ -d "$project_root" && -x "${project_root}/scripts/production-disk-guard.sh" ]] || {
  echo "disk guard project root is invalid" >&2
  exit 2
}
[[ -d "$(dirname "$output_file")" && ! -L "$output_file" ]] || {
  echo "disk guard override destination is absent or unsafe" >&2
  exit 2
}

umask 022
printf '%s\n' \
  '[Service]' \
  "WorkingDirectory=${project_root}" \
  'EnvironmentFile=' \
  "EnvironmentFile=-${environment_file}" \
  'ExecStart=' \
  "ExecStart=/usr/bin/env bash ${project_root}/scripts/production-disk-guard.sh" \
  'ProtectHome=read-only' \
  'ReadWritePaths=' \
  "ReadWritePaths=-${project_root}/target/debug /var/lib/docker /run/a-tiny-civilization" \
  >"$output_file"
