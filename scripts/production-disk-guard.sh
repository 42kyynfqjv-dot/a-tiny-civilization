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

readonly -a protected_state_volumes=(
  a-tiny-civilization-postgres-ruleset33-v1
  a-tiny-civilization-hindsight-v1
)

capacity_paths=("$project_root")
for volume in "${protected_state_volumes[@]}"; do
  mountpoint="$(docker inspect --type volume --format '{{.Mountpoint}}' "$volume" 2>/dev/null || true)"
  if [[ -n "$mountpoint" && -d "$mountpoint" ]]; then
    capacity_paths+=("$mountpoint")
  fi
done

read_capacity() {
  available_mib=9223372036854775807
  used_percent_number=0
  limiting_capacity_path="$project_root"
  declare -A seen_filesystems=()
  for capacity_path in "${capacity_paths[@]}"; do
    read -r filesystem available_kib used_percent < <(
      df -Pk -- "$capacity_path" | awk 'NR == 2 { print $1, $4, $5 }'
    )
    [[ -n "$filesystem" && "$available_kib" =~ ^[0-9]+$ && "$used_percent" =~ ^[0-9]+%$ ]]
    if [[ -n "${seen_filesystems[$filesystem]:-}" ]]; then
      continue
    fi
    seen_filesystems[$filesystem]=1
    path_available_mib=$((available_kib / 1024))
    path_used_percent=$((10#${used_percent%%%}))
    if ((path_available_mib < available_mib)); then
      available_mib=$path_available_mib
      limiting_capacity_path="$capacity_path"
    fi
    if ((path_used_percent > used_percent_number)); then
      used_percent_number=$path_used_percent
    fi
  done
}

needs_cleanup() {
  ((available_mib < trigger_free_mib || used_percent_number >= maximum_used_percent))
}

reserve_is_safe() {
  ((available_mib >= required_free_mib && used_percent_number < 95))
}

read_capacity
if ! needs_cleanup; then
  echo "Disk guard healthy: lowest reserve ${available_mib} MiB at ${limiting_capacity_path}; maximum ${used_percent_number}% used."
  exit 0
fi

echo "Disk guard triggered: lowest reserve ${available_mib} MiB at ${limiting_capacity_path}; maximum ${used_percent_number}% used."
if ((dry_run == 1)); then
  echo "Dry run: would prune unused Docker build cache and clear ${project_root}/target/debug when no build is active."
  exit 0
fi

lock_directory=/run/a-tiny-civilization
# systemd creates RuntimeDirectory for timer invocations, but operators also run
# this guard directly during releases. Keep the direct path equally reliable.
install -d -m 0755 "$lock_directory"
exec 9>"${lock_directory}/disk-guard.lock"
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
  echo "disk reserve remains unsafe after bounded cleanup: lowest reserve ${available_mib} MiB at ${limiting_capacity_path}; maximum ${used_percent_number}% used" >&2
  exit 1
fi

echo "Disk guard restored reserve: lowest reserve ${available_mib} MiB at ${limiting_capacity_path}; maximum ${used_percent_number}% used."
