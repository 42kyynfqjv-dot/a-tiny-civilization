#!/usr/bin/env bash
set -euo pipefail

# Stage the ignored NCI-60 qualification labels behind one pinned, content-addressed
# runtime path. The model-facing workers receive the public catalogue from the image;
# only the evidence worker is allowed to mount the file emitted here.

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly expected_catalogue_sha256='ab9f8087135aeb6a62c1d351d088a492b3dafb1c01dd4c37af0d0659be5362a5'
readonly expected_answer_key_sha256='559d52f45f18901d3ce8fb844f99cd88045ccd3fbd0c99cb7e8139b85e59f4ce'
readonly answer_key_name='nci-cellminer-2-15-cns-challenge-answer-key-v1.json'

source_path="${project_root}/data/source-cache/nci-cellminer-2026-08-12/${answer_key_name}"
catalogue_path="${project_root}/data/cancer-research/nci-cellminer-2-15-cns-challenge-catalogue-v1.json"
runtime_root="${CANCER_NCI60_QUALIFICATION_RUNTIME_ROOT:-${project_root}/runtime-qualification/nci60}"

while (($#)); do
  case "$1" in
    --source)
      source_path="${2:-}"
      shift 2
      ;;
    --catalogue)
      catalogue_path="${2:-}"
      shift 2
      ;;
    --runtime-root)
      runtime_root="${2:-}"
      shift 2
      ;;
    *)
      echo "usage: $0 [--source /absolute/answer-key.json] [--catalogue /absolute/catalogue.json] [--runtime-root /absolute/directory]" >&2
      exit 2
      ;;
  esac
done

if [[ "$runtime_root" != /* ]]; then
  echo "NCI-60 qualification runtime root must be absolute: $runtime_root" >&2
  exit 2
fi
for input in "$source_path" "$catalogue_path"; do
  if [[ ! -f "$input" || -L "$input" ]]; then
    echo "NCI-60 qualification input must be a regular, non-symlink file: $input" >&2
    exit 1
  fi
done
if [[ -e "$runtime_root" && ( ! -d "$runtime_root" || -L "$runtime_root" ) ]]; then
  echo "NCI-60 qualification runtime root must be a non-symlink directory: $runtime_root" >&2
  exit 1
fi

actual_catalogue_sha256="$(sha256sum -- "$catalogue_path" | awk '{print $1}')"
if [[ "$actual_catalogue_sha256" != "$expected_catalogue_sha256" ]]; then
  echo "NCI-60 challenge catalogue hash mismatch: expected $expected_catalogue_sha256, found $actual_catalogue_sha256" >&2
  exit 1
fi
actual_answer_key_sha256="$(sha256sum -- "$source_path" | awk '{print $1}')"
if [[ "$actual_answer_key_sha256" != "$expected_answer_key_sha256" ]]; then
  echo "NCI-60 qualification answer-key hash mismatch: expected $expected_answer_key_sha256, found $actual_answer_key_sha256" >&2
  exit 1
fi

target_directory="${runtime_root}/${expected_answer_key_sha256}"
target_path="${target_directory}/${answer_key_name}"
umask 022
mkdir -p -- "$target_directory"
if [[ -L "$target_directory" ]]; then
  echo "NCI-60 content-addressed target must not be a symlink: $target_directory" >&2
  exit 1
fi
chmod 0755 -- "$runtime_root" "$target_directory"

if [[ -e "$target_path" ]]; then
  if [[ ! -f "$target_path" || -L "$target_path" ]]; then
    echo "NCI-60 staged answer key is not a regular, non-symlink file: $target_path" >&2
    exit 1
  fi
  staged_sha256="$(sha256sum -- "$target_path" | awk '{print $1}')"
  if [[ "$staged_sha256" != "$expected_answer_key_sha256" ]]; then
    echo "NCI-60 content-addressed target is corrupt: $target_path" >&2
    exit 1
  fi
else
  temporary_path="$(mktemp "${target_directory}/.answer-key.XXXXXX")"
  cleanup() {
    rm -f -- "$temporary_path"
  }
  trap cleanup EXIT
  install -m 0444 -- "$source_path" "$temporary_path"
  staged_sha256="$(sha256sum -- "$temporary_path" | awk '{print $1}')"
  if [[ "$staged_sha256" != "$expected_answer_key_sha256" ]]; then
    echo "NCI-60 staged answer-key verification failed before publication" >&2
    exit 1
  fi
  mv -- "$temporary_path" "$target_path"
  temporary_path=''
  trap - EXIT
fi

# The container account is deliberately not mapped to the host owner. Read-only
# world-read permission makes this one bind-mounted file readable by uid 10001
# without granting that account write access or exposing the surrounding source cache.
chmod 0444 -- "$target_path"
if [[ "$(stat -c '%a' -- "$target_path")" != '444' ]]; then
  echo "NCI-60 staged answer key is not read-only and uid-10001-readable" >&2
  exit 1
fi
if [[ "$(sha256sum -- "$target_path" | awk '{print $1}')" != "$expected_answer_key_sha256" ]]; then
  echo "NCI-60 staged answer key changed after publication" >&2
  exit 1
fi

printf '%s\n' "$target_path"
