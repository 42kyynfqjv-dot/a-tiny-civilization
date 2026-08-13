#!/usr/bin/env bash
set -euo pipefail

# Stage the public aggregate behind a content-addressed path so the evidence
# worker can consume held-out context without exposing it to model-facing
# workers. The Rust qualifier revalidates the full semantic contract.

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly expected_sha256='f523989c2bec5ee14c0ff2c6dc30d193fb324e1dd234aba524bef179553294da'
readonly file_name='tcga-gbm-dr46-patient-baseline-v1.json'

source_path="${project_root}/data/cancer-research/${file_name}"
runtime_root="${CANCER_TCGA_GBM_CONTEXT_RUNTIME_ROOT:-${project_root}/runtime-qualification/tcga-gbm}"
verify_only=0

while (($#)); do
  case "$1" in
    --source)
      source_path="${2:-}"
      shift 2
      ;;
    --runtime-root)
      runtime_root="${2:-}"
      shift 2
      ;;
    --verify-only)
      verify_only=1
      shift
      ;;
    *)
      echo "usage: $0 [--source /absolute/baseline.json] [--runtime-root /absolute/directory] [--verify-only]" >&2
      exit 2
      ;;
  esac
done

assert_absolute_without_symlinks() {
  local path="$1"
  local allow_missing="$2"
  python3 - "$path" "$allow_missing" <<'PY'
import os
import stat
import sys

path = sys.argv[1]
allow_missing = sys.argv[2] == "1"
if not os.path.isabs(path):
    raise SystemExit(f"path must be absolute: {path}")
parts = path.split("/")[1:]
if any(part in ("", ".", "..") for part in parts):
    raise SystemExit(f"path must be normalized and contain no dot components: {path}")
current = "/"
missing = False
for part in parts:
    current = os.path.join(current, part)
    if missing:
        continue
    try:
        mode = os.lstat(current).st_mode
    except FileNotFoundError:
        if not allow_missing:
            raise SystemExit(f"path component does not exist: {current}")
        missing = True
        continue
    if stat.S_ISLNK(mode):
        raise SystemExit(f"path must not contain symlinks: {current}")
PY
}

assert_absolute_without_symlinks "$source_path" 0
assert_absolute_without_symlinks "$runtime_root" 1
if [[ ! -f "$source_path" || -L "$source_path" ]]; then
  echo "TCGA-GBM aggregate must be a regular, non-symlink file: $source_path" >&2
  exit 1
fi
if [[ "$(sha256sum -- "$source_path" | awk '{print $1}')" != "$expected_sha256" ]]; then
  echo "TCGA-GBM aggregate digest differs from the frozen source" >&2
  exit 1
fi

target_directory="${runtime_root}/sha256/${expected_sha256}"
target_path="${target_directory}/${file_name}"

verify_target() {
  assert_absolute_without_symlinks "$target_path" 0
  if [[ ! -f "$target_path" || -L "$target_path" ]]; then
    echo "TCGA-GBM staged aggregate is not a regular, non-symlink file" >&2
    exit 1
  fi
  if [[ "$(sha256sum -- "$target_path" | awk '{print $1}')" != "$expected_sha256" ]]; then
    echo "TCGA-GBM staged aggregate digest changed" >&2
    exit 1
  fi
  if [[ "$(stat -c '%a' -- "$target_path")" != '444' ]]; then
    echo "TCGA-GBM staged aggregate is not mode 0444" >&2
    exit 1
  fi
}

if ((verify_only == 0)); then
  umask 022
  mkdir -p -- "$target_directory"
  assert_absolute_without_symlinks "$target_directory" 0
  chmod 0755 -- "$runtime_root" "${runtime_root}/sha256" "$target_directory"
  if [[ ! -e "$target_path" ]]; then
    temporary_path="$(mktemp "${target_directory}/.context.XXXXXX")"
    cleanup() { rm -f -- "$temporary_path"; }
    trap cleanup EXIT
    install -m 0444 -- "$source_path" "$temporary_path"
    if [[ "$(sha256sum -- "$temporary_path" | awk '{print $1}')" != "$expected_sha256" ]]; then
      echo "TCGA-GBM aggregate changed during staging" >&2
      exit 1
    fi
    mv --no-clobber -- "$temporary_path" "$target_path"
    if [[ -e "$temporary_path" ]]; then
      rm -f -- "$temporary_path"
    fi
    temporary_path=''
    trap - EXIT
  fi
fi

verify_target
printf '%s\n' "$target_path"
