#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dry_run=0

while (($#)); do
  case "$1" in
    --dry-run) dry_run=1; shift ;;
    *) echo "usage: $0 [--dry-run]" >&2; exit 2 ;;
  esac
done

trigger_free_mib="${DISK_GUARD_TRIGGER_FREE_MIB:-23552}"
required_free_mib="${DISK_GUARD_REQUIRED_FREE_MIB:-20480}"
maximum_used_percent="${DISK_GUARD_MAX_USED_PERCENT:-90}"
for value in "$trigger_free_mib" "$required_free_mib" "$maximum_used_percent"; do
  [[ "$value" =~ ^[1-9][0-9]*$ ]] || {
    echo "disk guard thresholds must be positive integers" >&2
    exit 2
  }
done
((trigger_free_mib >= required_free_mib)) || {
  echo "DISK_GUARD_TRIGGER_FREE_MIB must be at least DISK_GUARD_REQUIRED_FREE_MIB" >&2
  exit 2
}
((required_free_mib >= 10240 && maximum_used_percent >= 50 && maximum_used_percent <= 95)) || {
  echo "disk guard requires at least 10 GiB free and a used-percent limit from 50 through 95" >&2
  exit 2
}

read_capacity() {
  read -r available_kib used_percent < <(df -Pk -- "$project_root" | awk 'NR == 2 { print $4, $5 }')
  [[ "$available_kib" =~ ^[0-9]+$ && "$used_percent" =~ ^[0-9]+%$ ]]
  available_mib=$((available_kib / 1024))
  used_percent_number=$((10#${used_percent%%%}))
}

needs_cleanup() {
  ((available_mib < trigger_free_mib || used_percent_number >= maximum_used_percent))
}

reserve_is_safe() {
  ((available_mib >= required_free_mib && used_percent_number < 95))
}

read_capacity
if ! needs_cleanup; then
  echo "Disk guard healthy: ${available_mib} MiB free and ${used_percent_number}% used."
  exit 0
fi

echo "Disk guard triggered: ${available_mib} MiB free and ${used_percent_number}% used."
if ((dry_run == 1)); then
  echo "Dry run: would prune unused Docker build cache and clear ${project_root}/target/debug when no build is active."
  exit 0
fi

exec 9>/run/a-tiny-civilization/disk-guard.lock
if ! flock -n 9; then
  echo "another disk guard run is active" >&2
  exit 0
fi

if ! pgrep -f '(^|/)(docker|docker-buildx)( |$).*(build|bake)' >/dev/null 2>&1; then
  docker builder prune --force --filter until=168h
else
  echo "Docker build is active; skipping old build-cache cleanup."
fi
read_capacity

development_target="${project_root}/target/debug"
if ! reserve_is_safe && [[ -d "$development_target" && ! -L "$development_target" ]]; then
  if pgrep -x cargo >/dev/null 2>&1 || pgrep -x rustc >/dev/null 2>&1; then
    echo "Rust build is active; preserving development artifacts."
  else
    echo "Clearing reproducible Rust development artifacts from ${development_target}."
    find "$development_target" -mindepth 1 -delete
  fi
fi
read_capacity

if ! reserve_is_safe && ! pgrep -f '(^|/)(docker|docker-buildx)( |$).*(build|bake)' >/dev/null 2>&1; then
  docker builder prune --force
fi
read_capacity

if ! reserve_is_safe; then
  echo "disk reserve remains unsafe after bounded cleanup: ${available_mib} MiB free and ${used_percent_number}% used" >&2
  exit 1
fi

echo "Disk guard restored reserve: ${available_mib} MiB free and ${used_percent_number}% used."
